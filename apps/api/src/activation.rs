//! Instance-reservation queries and the transactional pre-flight activation.
//!
//! This is the DB-backed half of the launch/restart gate. The pure decision
//! lives in `effective_context::pre_flight`; this module supplies the two
//! active-set counts and the atomic check-and-reserve transaction that
//! serializes each user's launches on their single user row (spec Decision 2
//! and 3):
//!
//! 1. Read the host ceiling from the `system_settings` singleton and take a
//!    best-effort, non-locking global count (racing launches from different
//!    users may overshoot by one or two — accepted, spec Decision 4);
//! 2. begin → `SELECT … FOR UPDATE` on the *single* user row → count that
//!    user's active instances → run `pre_flight` → on rejection roll back
//!    (no instance row left) and return the `PreflightReject` → otherwise
//!    reserve as `starting` (insert for a launch, status flip for a restart)
//!    → commit.
//!
//! The per-user lock is the only lock in the path: concurrent launches by the
//! same user serialize (exact ceiling) while different users never contend,
//! and a single-row lock cannot form a deadlock cycle. The persistent-instance
//! uniqueness rule runs inside the same transaction. No Docker I/O happens
//! here — the caller builds the container only after `Ok`.

use sea_orm::{
    ActiveModelTrait, ColumnTrait, ConnectionTrait, DatabaseConnection, EntityTrait,
    PaginatorTrait, QueryFilter, QueryOrder, QuerySelect, Set, TransactionTrait,
};
use uuid::Uuid;

use crate::db::{user, workspace_instance, WorkspaceInstance, WorkspaceTemplate, ACTIVE_STATUSES};
use crate::effective_context::{pre_flight, EffectiveContext, PreflightReject};
use crate::system_settings::SystemSettingsRepository;

// ── Active-set queries ────────────────────────────────────────
//
// Both are usable with `&DatabaseConnection` and `&DatabaseTransaction` via
// the `ConnectionTrait` bound, so the same code serves standalone accounting
// and the in-transaction counter gather.

/// Count active instances owned by `user_id`.
pub async fn count_active_instances_for_user<C>(db: &C, user_id: Uuid) -> Result<i32, sea_orm::DbErr>
where
    C: ConnectionTrait,
{
    let count = workspace_instance::Entity::find()
        .filter(workspace_instance::Column::OwnerId.eq(user_id))
        .filter(workspace_instance::Column::Status.is_in(ACTIVE_STATUSES))
        .count(db)
        .await?;
    Ok(count as i32)
}

/// Count all active instances across every user.
pub async fn count_active_instances_global<C>(db: &C) -> Result<i32, sea_orm::DbErr>
where
    C: ConnectionTrait,
{
    let count = workspace_instance::Entity::find()
        .filter(workspace_instance::Column::Status.is_in(ACTIVE_STATUSES))
        .count(db)
        .await?;
    Ok(count as i32)
}

// ── Activation request types ──────────────────────────────────

/// What the reservation inside `activate` should do.
#[derive(Debug, Clone)]
pub enum ActivationKind {
    /// Insert a fresh instance row with status `starting` (a new launch).
    Launch(LaunchPayload),
    /// Flip an existing row back to status `starting` (a restart).
    Restart { instance_id: Uuid },
}

/// The instance-row columns supplied by the caller for a launch. The helper
/// generates `id`, `access_token`, `access_password`, and `instance_number`,
/// and derives the auto-name (`"{template}-{number}"`) together with the
/// number so the two can never disagree.
#[derive(Debug, Clone)]
pub struct LaunchPayload {
    pub mount_persistent: bool,
    pub resolved_volume_host_path: Option<String>,
}

/// Everything `activate` needs: the template, the charged user's id, and the
/// user's already-resolved effective context.
#[derive(Debug, Clone)]
pub struct ActivationRequest<'a> {
    pub kind: ActivationKind,
    /// The template the instance runs from: its `id` is the pre-flight's
    /// requested template and links the reservation.
    pub template: &'a WorkspaceTemplate,
    /// The charged user's id — the single row `activate` locks. This is the
    /// instance owner, which equals the acting user on a launch but is the
    /// owner on a restart (an Admin/Manager may manage someone else's).
    pub user_id: Uuid,
    /// The owner's effective context (whitelist + ceiling + admin flag),
    /// computed by the caller via `PolicyRepository`. Never `None`: the caller
    /// treats a missing user as an internal error before calling.
    pub context: &'a EffectiveContext,
}

/// The committed reservation from a successful activation.
#[derive(Debug, Clone)]
pub struct Reservation {
    /// The reserved instance row (status `starting`).
    pub instance: WorkspaceInstance,
    /// True when the launch replaced a stale `error` persistent instance (its
    /// record was deleted inside the transaction). The caller uses this to
    /// decide whether to wipe the persistent volume before re-preparing.
    /// Always `false` for a restart.
    pub replaced_broken: bool,
}

/// Why an activation did not commit.
#[derive(Debug)]
pub enum ActivationError {
    /// The pre-flight policy rejected the request. The caller maps it to a
    /// structured `403` (whitelist) or `409` (ceiling) body.
    Rejected(PreflightReject),
    /// A domain conflict inside the transaction (currently the
    /// persistent-instance uniqueness rule). Propagates to a `409`.
    Conflict(String),
    /// A database error (including a missing user row).
    Db(sea_orm::DbErr),
}

impl From<sea_orm::DbErr> for ActivationError {
    fn from(e: sea_orm::DbErr) -> Self {
        ActivationError::Db(e)
    }
}

// ── The transactional check-and-reserve helper ────────────────

/// Run the spec-mandated activation sequence and, on success, return the
/// committed instance reservation (status `starting`).
///
/// The host ceiling and global count are best-effort and read before the
/// transaction starts (spec Decision 4). Inside the transaction the *single*
/// user row is locked `FOR UPDATE`, the user's active count is gathered, and
/// `pre_flight` runs against both counts; on rejection the transaction rolls
/// back (no row left) and the `PreflightReject` is returned. Otherwise the
/// reservation is written (insert or status flip) and committed. Lock order
/// never spans more than one row, so no deadlock cycle is possible. A
/// reserve-time domain conflict (the persistent-instance uniqueness rule) also
/// rolls back and is returned as `ActivationError::Conflict`.
///
/// The helper is free of Docker I/O; the caller continues with the slow
/// container build only after this returns `Ok`.
pub async fn activate(
    db: &DatabaseConnection,
    request: &ActivationRequest<'_>,
) -> Result<Reservation, ActivationError> {
    let host_instance_limit = SystemSettingsRepository::new(db)
        .get_or_create()
        .await?
        .host_instance_limit;
    let host_active_count = count_active_instances_global(db).await?;

    let tx = db.begin().await?;

    // Lock the single user row. Nothing else is locked, so concurrent launches
    // from the same user serialize here (exact ceiling) while launches from
    // different users never contend on a shared lock.
    user::Entity::find_by_id(request.user_id)
        .lock_exclusive()
        .one(&tx)
        .await?
        .ok_or_else(|| sea_orm::DbErr::RecordNotFound("user".into()))?;

    let active_own_count = count_active_instances_for_user(&tx, request.user_id).await?;

    if let Err(reject) = pre_flight(
        request.context,
        host_active_count,
        host_instance_limit,
        request.template.id,
        active_own_count,
        request.template.visibility,
    ) {
        tx.rollback().await?;
        return Err(ActivationError::Rejected(reject));
    }

    let reservation = match reserve(&tx, request).await {
        Ok(reservation) => reservation,
        Err(e) => {
            tx.rollback().await?;
            return Err(e);
        }
    };
    tx.commit().await?;
    Ok(reservation)
}

/// Write the reservation: insert a fresh `starting` row for a launch, or flip
/// an existing row back to `starting` for a restart.
///
/// For a launch, the persistent-instance uniqueness rule is enforced here,
/// inside the transaction — serialized with the pre-flight by the user-row
/// lock held in `activate`. A stale `error` record (a failed launch that never
/// became a tenant) is deleted and replaced; the caller is told via
/// `Reservation::replaced_broken` so it can wipe the volume before re-preparing.
async fn reserve<C: ConnectionTrait>(
    db: &C,
    request: &ActivationRequest<'_>,
) -> Result<Reservation, ActivationError> {
    match &request.kind {
        ActivationKind::Launch(payload) => {
            // One persistent instance per (template, owner). A stale `error`
            // record occupies no real tenant slot and may be replaced.
            let mut replaced_broken = false;
            if payload.mount_persistent {
                let existing = workspace_instance::Entity::find()
                    .filter(workspace_instance::Column::TemplateId.eq(request.template.id))
                    .filter(workspace_instance::Column::OwnerId.eq(request.user_id))
                    .filter(workspace_instance::Column::MountPersistent.eq(true))
                    .one(db)
                    .await?;
                match existing {
                    Some(existing) if existing.status != "error" => {
                        return Err(ActivationError::Conflict(
                            "An instance with persistent storage already exists for this template and user"
                                .to_string(),
                        ));
                    }
                    Some(existing) => {
                        tracing::warn!(
                            "Replacing broken persistent instance {} (template={}, owner={})",
                            existing.id,
                            request.template.id,
                            request.user_id
                        );
                        workspace_instance::Entity::delete_by_id(existing.id)
                            .exec(db)
                            .await?;
                        replaced_broken = true;
                    }
                    None => {}
                }
            }

            // Derive the auto-name together with `instance_number` so the two
            // can never disagree.
            let instance_number = next_instance_number(db, request.template.id).await?;
            let name = format!("{}-{}", request.template.name, instance_number);

            let id = Uuid::new_v4();
            let model = workspace_instance::ActiveModel {
                id: Set(id),
                template_id: Set(request.template.id),
                name: Set(name),
                instance_number: Set(instance_number),
                owner_id: Set(request.user_id),
                container_id: Set(None),
                status: Set("starting".to_string()),
                access_token: Set(Uuid::new_v4().as_simple().to_string()),
                access_password: Set(crate::db::generate_access_password()),
                mount_persistent: Set(payload.mount_persistent),
                resolved_volume_host_path: Set(payload.resolved_volume_host_path.clone()),
                host_port: Set(None),
                started_at: Set(None),
                last_seen_at: Set(None),
                ..Default::default()
            };
            let inserted = model.insert(db).await?;
            Ok(Reservation {
                instance: inserted.into(),
                replaced_broken,
            })
        }
        ActivationKind::Restart { instance_id } => {
            let existing = workspace_instance::Entity::find_by_id(*instance_id)
                .one(db)
                .await?
                .ok_or_else(|| sea_orm::DbErr::RecordNotFound("workspace_instance".into()))?;
            // Sanity: the reservation target must be the owner's own instance
            // and run the supplied template, or the accounting is wrong.
            if existing.owner_id != request.user_id {
                return Err(ActivationError::Db(sea_orm::DbErr::Custom(
                    "instance does not belong to the user".into(),
                )));
            }
            if existing.template_id != request.template.id {
                return Err(ActivationError::Db(sea_orm::DbErr::Custom(
                    "instance template mismatch".into(),
                )));
            }
            workspace_instance::Entity::update(workspace_instance::ActiveModel {
                id: Set(existing.id),
                status: Set("starting".to_string()),
                ..Default::default()
            })
            .exec(db)
            .await?;
            let updated = workspace_instance::Entity::find_by_id(existing.id)
                .one(db)
                .await?
                .ok_or_else(|| sea_orm::DbErr::RecordNotFound("workspace_instance".into()))?;
            Ok(Reservation {
                instance: updated.into(),
                replaced_broken: false,
            })
        }
    }
}

/// The next `instance_number` for a template: the current max plus one.
async fn next_instance_number<C: ConnectionTrait>(
    db: &C,
    template_id: Uuid,
) -> Result<i32, sea_orm::DbErr> {
    let max = workspace_instance::Entity::find()
        .filter(workspace_instance::Column::TemplateId.eq(template_id))
        .order_by_desc(workspace_instance::Column::InstanceNumber)
        .one(db)
        .await?
        .map(|m| m.instance_number)
        .unwrap_or(0);
    Ok(max + 1)
}
