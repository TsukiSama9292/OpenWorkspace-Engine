//! Instance-reservation queries and the transactional pre-flight activation.
//!
//! This is the DB-backed half of the launch/restart gate. The pure decision
//! lives in `effective_context::pre_flight`; this module supplies the active-set
//! counts and resource sums, and the atomic check-and-reserve transaction that
//! serializes each launch on its billing target's single row (spec Decision 2
//! and 3):
//!
//! 1. Read the host ceilings from the `system_settings` singleton and take a
//!    best-effort, non-locking global count + resource sum (racing launches
//!    from different users may overshoot by one or two — accepted, spec
//!    Decision 4);
//! 2. begin → `SELECT … FOR UPDATE` on the billing target's row (the charged
//!    group when the launch bills a group, else the single user row) → gather
//!    the exact billed resource sums and the owner's active count → run
//!    `pre_flight` → on rejection roll back (no instance row left) and return
//!    the `PreflightReject` → otherwise reserve as `starting` (insert for a
//!    launch, status flip for a restart) → commit.
//!
//! Lock order is always "group row, then user row": a group-billed launch locks
//! its group first and the owner's user row second, a self-billed launch locks
//! only the user row. The order never varies, so no deadlock cycle is possible,
//! while concurrent launches of the same user (exact instance ceiling) and
//! against the same group pool (exact resource billing) both serialize. The
//! persistent-instance uniqueness rule runs inside the same transaction. No
//! Docker I/O happens here — the caller builds the container only after `Ok`.

use sea_orm::{
    ActiveModelTrait, ColumnTrait, ConnectionTrait, DatabaseConnection, EntityTrait,
    PaginatorTrait, QueryFilter, QueryOrder, QuerySelect, Set, TransactionTrait,
};
use uuid::Uuid;

use crate::db::{
    user, workspace_instance, GroupRecord, WorkspaceInstance, WorkspaceTemplate, ACTIVE_STATUSES,
};
use crate::effective_context::{
    pre_flight, add_use, EffectiveContext, PreflightReject, QuotaContext, ResourceUse,
};
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

/// Sum the host resources of all active instances.
pub async fn sum_resources_global<C>(db: &C) -> Result<ResourceUse, sea_orm::DbErr>
where
    C: ConnectionTrait,
{
    let rows = workspace_instance::Entity::find()
        .filter(workspace_instance::Column::Status.is_in(ACTIVE_STATUSES))
        .all(db)
        .await?;
    Ok(rows
        .iter()
        .fold(ResourceUse::default(), |acc, r| {
            add_use(
                &acc,
                &ResourceUse {
                    cpu_cores: r.host_cpu_cores as i64,
                    memory_mb: r.host_memory_mb,
                    gpu_count: r.host_gpu_count as i64,
                },
            )
        }))
}

/// Sum the host resources of a user's active *self-billed* instances
/// (`owner_group_id IS NULL`). Self-billed launches have no pool, so their
/// usage only counts against the host caps.
pub async fn sum_resources_for_user_self_billed<C>(
    db: &C,
    user_id: Uuid,
) -> Result<ResourceUse, sea_orm::DbErr>
where
    C: ConnectionTrait,
{
    let rows = workspace_instance::Entity::find()
        .filter(workspace_instance::Column::Status.is_in(ACTIVE_STATUSES))
        .filter(workspace_instance::Column::OwnerId.eq(user_id))
        .filter(workspace_instance::Column::OwnerGroupId.is_null())
        .all(db)
        .await?;
    Ok(rows
        .iter()
        .fold(ResourceUse::default(), |acc, r| {
            add_use(
                &acc,
                &ResourceUse {
                    cpu_cores: r.host_cpu_cores as i64,
                    memory_mb: r.host_memory_mb,
                    gpu_count: r.host_gpu_count as i64,
                },
            )
        }))
}

/// Sum the host resources of all active instances billing against the group's
/// pool (`owner_group_id = group_id`). For a `dedicated` pool this is the
/// group-wide total; for `shared` it is every member's usage summed together.
pub async fn sum_resources_for_group<C>(
    db: &C,
    group_id: Uuid,
) -> Result<ResourceUse, sea_orm::DbErr>
where
    C: ConnectionTrait,
{
    let rows = workspace_instance::Entity::find()
        .filter(workspace_instance::Column::Status.is_in(ACTIVE_STATUSES))
        .filter(workspace_instance::Column::OwnerGroupId.eq(group_id))
        .all(db)
        .await?;
    Ok(rows
        .iter()
        .fold(ResourceUse::default(), |acc, r| {
            add_use(
                &acc,
                &ResourceUse {
                    cpu_cores: r.host_cpu_cores as i64,
                    memory_mb: r.host_memory_mb,
                    gpu_count: r.host_gpu_count as i64,
                },
            )
        }))
}

/// Sum the host resources of one user's active instances billing against a
/// group's pool. Used by the group-billing view to break usage down per member.
pub async fn sum_resources_for_user_in_group<C>(
    db: &C,
    user_id: Uuid,
    group_id: Uuid,
) -> Result<ResourceUse, sea_orm::DbErr>
where
    C: ConnectionTrait,
{
    let rows = workspace_instance::Entity::find()
        .filter(workspace_instance::Column::Status.is_in(ACTIVE_STATUSES))
        .filter(workspace_instance::Column::OwnerId.eq(user_id))
        .filter(workspace_instance::Column::OwnerGroupId.eq(group_id))
        .all(db)
        .await?;
    Ok(rows
        .iter()
        .fold(ResourceUse::default(), |acc, r| {
            add_use(
                &acc,
                &ResourceUse {
                    cpu_cores: r.host_cpu_cores as i64,
                    memory_mb: r.host_memory_mb,
                    gpu_count: r.host_gpu_count as i64,
                },
            )
        }))
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
    /// The group the instance bills its resources against, when one is chosen
    /// (`None` = self-billed, unlimited resources). Must be a group the user
    /// belongs to; `activate` serializes on this group's row.
    pub owner_group_id: Option<Uuid>,
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
/// The host ceilings and the global active count / resource sum are best-effort
/// and read before the transaction starts (spec Decision 4). Inside the
/// transaction the billing target's row is locked `FOR UPDATE` — the charged
/// group when the launch bills a group, else the single user row — the target's
/// billed resource sum and the user's active count are gathered, and
/// `pre_flight` runs against all of it; on rejection the transaction rolls back
/// (no row left) and the `PreflightReject` is returned. Otherwise the
/// reservation is written (insert or status flip) and committed. Lock order is
/// fixed (group, then user), so no deadlock cycle is possible. A reserve-time
/// domain conflict (the persistent-instance uniqueness rule) also rolls back
/// and is returned as `ActivationError::Conflict`.
///
/// The helper is free of Docker I/O; the caller continues with the slow
/// container build only after this returns `Ok`.
pub async fn activate(
    db: &DatabaseConnection,
    request: &ActivationRequest<'_>,
) -> Result<Reservation, ActivationError> {
    let settings = SystemSettingsRepository::new(db).get_or_create().await?;
    let host_instance_limit = settings.host_instance_limit;
    let host_capacity = ResourceUse {
        cpu_cores: settings.host_cpu_cores as i64,
        memory_mb: settings.host_memory_mb,
        gpu_count: settings.host_gpu_count as i64,
    };
    let host_active_count = count_active_instances_global(db).await?;
    let host_used = sum_resources_global(db).await?;

    let tx = db.begin().await?;

    // Lock order is fixed (group, then user), so the path can never deadlock: a
    // group-billed launch locks the charged group's row first, a self-billed
    // one skips straight to the user row. Either way the owner's user row is
    // locked second, serializing the same owner's launches for the exact
    // instance ceiling and persistent-instance uniqueness.
    let target_group = if let Some(group_id) = request.owner_group_id {
        let group = crate::db::GroupRepository::lock_for_update(&tx, group_id).await?;
        if group.is_none() {
            tx.rollback().await?;
            return Err(ActivationError::Db(sea_orm::DbErr::RecordNotFound("group".into())));
        }
        group
    } else {
        None
    };

    user::Entity::find_by_id(request.user_id)
        .lock_exclusive()
        .one(&tx)
        .await?
        .ok_or_else(|| sea_orm::DbErr::RecordNotFound("user".into()))?;

    let active_own_count = count_active_instances_for_user(&tx, request.user_id).await?;

    let (pool, billed, snapshot) = match &target_group {
        Some(group) => {
            let pool = ResourceUse {
                cpu_cores: group.pool_cpu_cores as i64,
                memory_mb: group.pool_memory_mb,
                gpu_count: group.pool_gpu_count as i64,
            };
            let billed = sum_resources_for_group(&tx, group.id).await?;
            // Freeze the pool caps in effect at launch so the UI can report what
            // the instance was launched under even after the group is
            // reconfigured. The live restart/launch check always uses the
            // *current* pool from the locked row above.
            let snapshot = Some(serde_json::json!({
                "group_id": group.id,
                "billing_model": group.billing_model,
                "pool_cpu_cores": group.pool_cpu_cores,
                "pool_memory_mb": group.pool_memory_mb,
                "pool_gpu_count": group.pool_gpu_count,
            }));
            (pool, billed, snapshot)
        }
        None => {
            let billed = sum_resources_for_user_self_billed(&tx, request.user_id).await?;
            (ResourceUse::default(), billed, None)
        }
    };

    let billing = QuotaContext {
        pool,
        billed,
        host_capacity,
        host_used,
        target_group_id: request.owner_group_id,
    };
    let requested_resources = ResourceUse {
        cpu_cores: request.template.cores as i64,
        memory_mb: request.template.memory,
        gpu_count: request.template.gpu_count as i64,
    };

    if let Err(reject) = pre_flight(
        request.context,
        host_active_count,
        host_instance_limit,
        request.template.id,
        active_own_count,
        request.template.visibility,
        &requested_resources,
        &billing,
    ) {
        tx.rollback().await?;
        return Err(ActivationError::Rejected(reject));
    }

    let reservation = match reserve(&tx, request, snapshot.as_ref()).await {
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
    billing_group_snapshot: Option<&serde_json::Value>,
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
                owner_group_id: Set(request.owner_group_id),
                billing_group_snapshot: Set(billing_group_snapshot.cloned()),
                // The instance's billed resources are exactly what its template
                // requests; the container is created to that size.
                host_cpu_cores: Set(request.template.cores),
                host_memory_mb: Set(request.template.memory),
                host_gpu_count: Set(request.template.gpu_count),
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
