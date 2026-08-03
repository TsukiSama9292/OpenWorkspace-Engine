//! Quota DB queries and the transactional check-and-reserve helper.
//!
//! Two responsibilities, both consumed by the activation routes (the launch
//! and restart paths):
//!
//! * **Active-set queries** — counts and CPU/RAM sums over the Active Set
//!   (`running` / `starting` / `paused`, spec Decision 2). They are generic
//!   over any SeaORM connection (`&DatabaseConnection` or
//!   `&DatabaseTransaction`), so they run both standalone (accounting,
//!   lifecycle) and inside the activation transaction.
//! * **`activate`** — the atomic check-and-reserve sequence from spec
//!   Decision 3: begin → lock the global `system_settings` row (`FOR UPDATE`)
//!   → lock the user row → gather counters → run `quota::check` → on violation
//!   roll back (no instance row left) and return the `QuotaViolation` →
//!   otherwise reserve the instance as `starting` (insert for a launch, status
//!   flip for a restart) → commit.
//!
//! The persistent-instance uniqueness rule (one `mount_persistent` instance
//! per template-and-owner) is enforced inside the same transaction, so it is
//! serialized with the quota check by the user-row lock: a conflicting launch
//! rolls back and surfaces as `ActivationError::Conflict`, while a stale
//! `error` record is deleted and replaced.
//!
//! Lock order is strictly global-then-user (the spec's deadlock rule); nothing
//! runs between the two locks except the counter gathers and the pure check.
//! The helper performs no Docker I/O.

use sea_orm::sea_query::{Alias, Expr};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, ConnectionTrait, DatabaseConnection, EntityTrait,
    FromQueryResult, PaginatorTrait, QueryFilter, QueryOrder, QuerySelect, Set, TransactionTrait,
};
use uuid::Uuid;

use crate::auth::Role;
use crate::db::{user, workspace_instance, workspace_template, WorkspaceInstance, WorkspaceTemplate, ACTIVE_STATUSES};
use crate::quota::{self, AllocationMode, QuotaOverride, QuotaViolation};
use crate::system_settings::system_settings as settings_entity;

/// CPU cores / RAM bytes summed over a set of active instances. Cores are
/// whole numbers matching template `cores`; RAM is bytes matching `memory`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ActiveResources {
    pub cpu_cores: i32,
    pub ram_bytes: i64,
}

// ── Active-set queries ────────────────────────────────────────
//
// All four are usable with both `&DatabaseConnection` and
// `&DatabaseTransaction` via the `ConnectionTrait` bound, so the same code
// serves standalone accounting and the in-transaction counter gathers.

/// Count active instances owned by `user_id`.
pub async fn count_active_instances_for_user<C>(
    db: &C,
    user_id: Uuid,
) -> Result<i32, sea_orm::DbErr>
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

/// Sum CPU/RAM across every active instance (both modes).
pub async fn sum_active_resources_global<C>(db: &C) -> Result<ActiveResources, sea_orm::DbErr>
where
    C: ConnectionTrait,
{
    Ok(ActiveResources {
        cpu_cores: sum_active_cpu(db, None, None).await?,
        ram_bytes: sum_active_ram(db, None, None).await?,
    })
}

/// Sum CPU/RAM across active instances whose template has the given
/// `allocation_mode` — the host dedicated pool (dedicated) and the host shared
/// fuse (shared) counters.
pub async fn sum_active_resources_by_mode<C>(
    db: &C,
    mode: AllocationMode,
) -> Result<ActiveResources, sea_orm::DbErr>
where
    C: ConnectionTrait,
{
    Ok(ActiveResources {
        cpu_cores: sum_active_cpu(db, Some(mode), None).await?,
        ram_bytes: sum_active_ram(db, Some(mode), None).await?,
    })
}

/// Sum CPU/RAM across `user_id`'s active instances — the per-user personal
/// quota counters (dedicated and shared combined).
pub async fn sum_active_resources_for_user<C>(
    db: &C,
    user_id: Uuid,
) -> Result<ActiveResources, sea_orm::DbErr>
where
    C: ConnectionTrait,
{
    Ok(ActiveResources {
        cpu_cores: sum_active_cpu(db, None, Some(user_id)).await?,
        ram_bytes: sum_active_ram(db, None, Some(user_id)).await?,
    })
}

#[derive(FromQueryResult)]
struct SumRow {
    sum: Option<i64>,
}

/// A `SELECT SUM(template.cores) ... FROM instances JOIN templates` restricted
/// to active instances, optionally narrowed to a template `allocation_mode`
/// and/or an owner.
fn active_sum_query(
    mode: Option<AllocationMode>,
    user_id: Option<Uuid>,
) -> sea_orm::Select<workspace_instance::Entity> {
    let mut select = workspace_instance::Entity::find()
        .inner_join(workspace_template::Entity)
        .filter(workspace_instance::Column::Status.is_in(ACTIVE_STATUSES))
        .select_only();
    if let Some(mode) = mode {
        select = select
            .filter(workspace_template::Column::AllocationMode.eq(allocation_mode_str(mode)));
    }
    if let Some(user_id) = user_id {
        select = select.filter(workspace_instance::Column::OwnerId.eq(user_id));
    }
    select
}

/// `SUM(template.cores)` over active instances, optionally filtered. The
/// `::BIGINT` cast keeps Postgres's `numeric` aggregate output decodable as
/// `i64`.
async fn sum_active_cpu<C: ConnectionTrait>(
    db: &C,
    mode: Option<AllocationMode>,
    user_id: Option<Uuid>,
) -> Result<i32, sea_orm::DbErr> {
    let row = active_sum_query(mode, user_id)
        .column_as(
            Expr::col(workspace_template::Column::Cores)
                .sum()
                .cast_as(Alias::new("BIGINT")),
            "sum",
        )
        .into_model::<SumRow>()
        .one(db)
        .await?;
    Ok(row.and_then(|r| r.sum).unwrap_or(0) as i32)
}

/// `SUM(template.memory)` over active instances, optionally filtered.
async fn sum_active_ram<C: ConnectionTrait>(
    db: &C,
    mode: Option<AllocationMode>,
    user_id: Option<Uuid>,
) -> Result<i64, sea_orm::DbErr> {
    let row = active_sum_query(mode, user_id)
        .column_as(
            Expr::col(workspace_template::Column::Memory)
                .sum()
                .cast_as(Alias::new("BIGINT")),
            "sum",
        )
        .into_model::<SumRow>()
        .one(db)
        .await?;
    Ok(row.and_then(|r| r.sum).unwrap_or(0))
}

/// The template's `allocation_mode` string as the policy enum.
fn allocation_mode_str(mode: AllocationMode) -> &'static str {
    match mode {
        AllocationMode::Dedicated => "dedicated",
        AllocationMode::Shared => "shared",
    }
}

fn parse_allocation_mode(template: &WorkspaceTemplate) -> Result<AllocationMode, sea_orm::DbErr> {
    match template.allocation_mode.as_str() {
        "dedicated" => Ok(AllocationMode::Dedicated),
        "shared" => Ok(AllocationMode::Shared),
        other => Err(sea_orm::DbErr::Custom(format!(
            "unknown allocation_mode: {other}"
        ))),
    }
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

/// Everything `activate` needs: the template (resources + allocation_mode),
/// the user (role + quota overrides), and the reservation kind.
#[derive(Debug, Clone)]
pub struct ActivationRequest<'a> {
    pub kind: ActivationKind,
    /// The template the instance runs from: `cores`/`memory` are the requested
    /// resources, `allocation_mode` decides host-level accounting, `id` links
    /// the reservation.
    pub template: &'a WorkspaceTemplate,
    /// The activating user's id — the row `activate` locks and is charged.
    pub user_id: Uuid,
    /// The activating user's role (Admin is personally exempt).
    pub role: Role,
    /// The activating user's per-field quota overrides.
    pub user_overrides: QuotaOverride,
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
    /// The quota policy rejected the request. Propagates to a `409`.
    Quota(QuotaViolation),
    /// A domain conflict inside the transaction (currently the
    /// persistent-instance uniqueness rule). Propagates to a `409`.
    Conflict(String),
    /// A database error (including a missing `system_settings`/user row).
    Db(sea_orm::DbErr),
}

impl From<sea_orm::DbErr> for ActivationError {
    fn from(e: sea_orm::DbErr) -> Self {
        ActivationError::Db(e)
    }
}

// ── The transactional check-and-reserve helper ────────────────

/// Run the spec-mandated atomic activation sequence and, on success, return
/// the committed instance reservation (status `starting`).
///
/// Sequence: begin → `SELECT ... FOR UPDATE` the global `system_settings` row →
/// `SELECT ... FOR UPDATE` the user row → gather the active counters → run
/// `quota::check` → on violation roll back (no row left) and return the
/// `QuotaViolation` → otherwise reserve as `starting` (insert or status flip)
/// → commit. Lock order is strictly global-then-user. A reserve-time domain
/// conflict (the persistent-instance uniqueness rule) also rolls back and is
/// returned as `ActivationError::Conflict`.
///
/// The helper is free of Docker I/O; the caller continues with the slow
/// container build only after this returns `Ok`.
pub async fn activate(
    db: &DatabaseConnection,
    request: &ActivationRequest<'_>,
) -> Result<Reservation, ActivationError> {
    let tx = db.begin().await?;

    // 1. Lock the global singleton settings row first (the spec's deadlock
    //    rule: ancestors before descendants along the ownership chain).
    let settings = settings_entity::Entity::find_by_id(1)
        .lock_exclusive()
        .one(&tx)
        .await?
        .ok_or_else(|| sea_orm::DbErr::RecordNotFound("system_settings".into()))?;

    // 2. Lock the user row. Nothing runs between the two locks except the
    //    counter gathers and the pure check below.
    user::Entity::find_by_id(request.user_id)
        .lock_exclusive()
        .one(&tx)
        .await?
        .ok_or_else(|| sea_orm::DbErr::RecordNotFound("user".into()))?;

    // 3. Gather counters under the locks.
    let user_active_instances = count_active_instances_for_user(&tx, request.user_id).await?;
    let host_active_instances = count_active_instances_global(&tx).await?;
    let user_resources = sum_active_resources_for_user(&tx, request.user_id).await?;
    let dedicated = sum_active_resources_by_mode(&tx, AllocationMode::Dedicated).await?;
    let shared = sum_active_resources_by_mode(&tx, AllocationMode::Shared).await?;

    // 4. Run the pure policy check.
    let mode = parse_allocation_mode(request.template)?;
    let inputs = quota::CheckInputs {
        mode,
        requested_cores: request.template.cores,
        requested_ram_bytes: request.template.memory,
        effective_quota: quota::resolve_effective_quota(request.user_overrides, request.role.clone()),
        host_capacity: quota::HostCapacity {
            max_cpu_cores: settings.max_cpu_cores,
            max_ram_bytes: settings.max_ram_bytes,
        },
        host_instance_limit: settings.host_instance_limit,
        shared_max_cpu: settings.shared_max_cpu,
        shared_max_ram: settings.shared_max_ram,
        user_active_instances,
        user_active_cpu: user_resources.cpu_cores,
        user_active_ram: user_resources.ram_bytes,
        host_active_instances,
        host_dedicated_active_cpu: dedicated.cpu_cores,
        host_dedicated_active_ram: dedicated.ram_bytes,
        host_shared_active_cpu: shared.cpu_cores,
        host_shared_active_ram: shared.ram_bytes,
    };
    if let Err(violation) = quota::check(&inputs) {
        tx.rollback().await?;
        return Err(ActivationError::Quota(violation));
    }

    // 5. Reserve as `starting`, then commit. Any reserve failure (a domain
    //    conflict such as the persistent-uniqueness rule, or a DB error) rolls
    //    the transaction back so no partial row survives.
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
/// inside the transaction — serialized with the quota check by the user-row
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
            // can never disagree (the launch route used to generate both in a
            // single query).
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
            // Sanity: the reservation target must be the user's own instance
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
