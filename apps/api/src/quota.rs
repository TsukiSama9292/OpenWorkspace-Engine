//! Pure pre-flight quota policy: the accept-or-reject decision logic for
//! instance/resource limits, with no database or Docker access.
//!
//! Everything here is a plain input/output transform so it can be unit-tested
//! without infrastructure (prior art: `network_qos.rs`, `instance_net.rs`).
//! The orchestration — running inside a transaction under the global and user
//! row locks, gathering the active counters, persisting a reservation — lives
//! in the routes/DB layer; this module only decides whether a request fits.
//!
//! `check` runs the five-step pipeline from the spec in fixed order and
//! returns the *first* violation:
//!
//! 1. per-user instance count (`user_instance`) — skipped when the effective
//!    quota is personally exempt (Admin);
//! 2. global host instance count (`host_instance`) — skipped when
//!    `host_instance_limit == 0` (0 means unlimited);
//! 3. per-user CPU/RAM (`user_cpu` / `user_ram`) — skipped when exempt;
//! 4. host dedicated pool (`host_dedicated_cpu` / `host_dedicated_ram`) —
//!    dedicated allocations only, always enforced;
//! 5. host shared fuse (`host_shared_cpu` / `host_shared_ram`) — shared
//!    allocations only, when the fuse is enabled (`shared_max_* > 0`).
//!
//! Personal-level checks (1 and 3) are skipped for Admin; global-level checks
//! (2, 4, 5) always run regardless of role.

use crate::auth::Role;
use serde::{Deserialize, Serialize};

/// How a template's instances are accounted at the host level.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AllocationMode {
    /// Hard reservation: deducts CPU/RAM from host capacity while active
    /// (running, starting, or paused).
    Dedicated,
    /// Overcommitted: cgroups limits are the safety net and no static host
    /// budget is deducted.
    Shared,
}

/// A per-user effective quota: the numbers a `check` compares against.
///
/// `exempt` marks the quota as personally exempt (Admin): steps 1 and 3 of the
/// pipeline are skipped entirely. Global steps (2, 4, 5) still run.
///
/// Units mirror the template model: `instance_limit` is a count, `max_cpu_cores`
/// is whole CPU cores (`i32`, matching template `cores`), `max_ram_bytes` is
/// bytes (`i64`, matching template `memory`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Quota {
    /// True when personal-level checks are skipped (Admin). Always derived
    /// from the role, never set by an override.
    pub exempt: bool,
    /// Maximum number of active instances the user may own.
    pub instance_limit: i32,
    /// Maximum CPU cores the user's active instances may sum to.
    pub max_cpu_cores: i32,
    /// Maximum RAM bytes the user's active instances may sum to.
    pub max_ram_bytes: i64,
}

/// A user's optional per-field override on top of the role default. A `None`
/// field inherits that field's role default (the override is never an
/// absolute). This is the seam a future Group tier slots into as one more
/// fallback level.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct QuotaOverride {
    /// Override for the per-user instance limit; `None` inherits the role default.
    pub instance_limit: Option<i32>,
    /// Override for the per-user CPU quota; `None` inherits the role default.
    pub max_cpu_cores: Option<i32>,
    /// Override for the per-user RAM quota (bytes); `None` inherits the role default.
    pub max_ram_bytes: Option<i64>,
}

/// Total allocatable CPU/RAM of the host. Used as the budget for the dedicated
/// pool (step 4). From the `system_settings` row, not detected here.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct HostCapacity {
    /// Total CPU cores the host can dedicate.
    pub max_cpu_cores: i32,
    /// Total RAM bytes the host can dedicate.
    pub max_ram_bytes: i64,
}

/// The scope of a quota rejection — the machine-readable key of the
/// rejection contract. One value per pipeline check.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum QuotaScope {
    /// Per-user active instance count exceeded its limit.
    UserInstance,
    /// Per-user active CPU sum exceeded its quota.
    UserCpu,
    /// Per-user active RAM sum exceeded its quota.
    UserRam,
    /// Global active instance count exceeded `host_instance_limit`.
    HostInstance,
    /// Host dedicated pool CPU exhausted.
    HostDedicatedCpu,
    /// Host dedicated pool RAM exhausted.
    HostDedicatedRam,
    /// Host shared fuse CPU exceeded.
    HostSharedCpu,
    /// Host shared fuse RAM exceeded.
    HostSharedRam,
}

/// The reason a request was rejected. All values are non-negative magnitudes
/// in the scope's unit (instance count, whole cores, or bytes).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct QuotaViolation {
    /// Which check failed.
    pub scope: QuotaScope,
    /// Usage already held (active count / sum) when the check ran.
    pub current: i64,
    /// The limit the request would exceed.
    pub limit: i64,
    /// The amount this single request would add.
    pub requested: i64,
}

/// All plain values `check` needs. Callers (activation routes) gather the
/// counters from the DB inside the transaction and fill this in.
#[derive(Debug, Clone, Copy)]
pub struct CheckInputs {
    /// Whether the request is for a dedicated or shared template.
    pub mode: AllocationMode,
    /// CPU cores the new instance requests.
    pub requested_cores: i32,
    /// RAM bytes the new instance requests.
    pub requested_ram_bytes: i64,
    /// The requester's effective quota (from `resolve_effective_quota`).
    pub effective_quota: Quota,
    /// Host total allocatable CPU/RAM for the dedicated pool.
    pub host_capacity: HostCapacity,
    /// Global active-instance cap; `0` disables step 2.
    pub host_instance_limit: i32,
    /// Host shared fuse CPU cap; `0` disables the shared CPU check.
    pub shared_max_cpu: i32,
    /// Host shared fuse RAM cap; `0` disables the shared RAM check.
    pub shared_max_ram: i64,
    /// Active instances owned by this user.
    pub user_active_instances: i32,
    /// Active CPU cores summed across this user's instances.
    pub user_active_cpu: i32,
    /// Active RAM bytes summed across this user's instances.
    pub user_active_ram: i64,
    /// Active instances across all users.
    pub host_active_instances: i32,
    /// Active dedicated CPU cores summed across all users.
    pub host_dedicated_active_cpu: i32,
    /// Active dedicated RAM bytes summed across all users.
    pub host_dedicated_active_ram: i64,
    /// Active shared CPU cores summed across all users.
    pub host_shared_active_cpu: i32,
    /// Active shared RAM bytes summed across all users.
    pub host_shared_active_ram: i64,
}

/// Resolve a user's effective quota: role defaults overridden per field.
///
/// Role defaults: `user` = 2 instances / 4 cores / 8 GiB, `manager` =
/// 5 instances / 12 cores / 32 GiB, `admin` = personally exempt. A `Some`
/// field in `override_` wins over the role default; a `None` field inherits it.
/// Admin is always personally exempt, regardless of the override.
///
/// This is the single resolution function the spec's Group seam needs: a
/// future Group tier becomes one more fallback level above the role default.
pub fn resolve_effective_quota(override_: QuotaOverride, role: Role) -> Quota {
    let base = match role {
        Role::Admin => Quota {
            exempt: true,
            instance_limit: 0,
            max_cpu_cores: 0,
            max_ram_bytes: 0,
        },
        Role::Manager => Quota {
            exempt: false,
            instance_limit: 5,
            max_cpu_cores: 12,
            max_ram_bytes: MANAGER_DEFAULT_RAM,
        },
        Role::User => Quota {
            exempt: false,
            instance_limit: 2,
            max_cpu_cores: 4,
            max_ram_bytes: USER_DEFAULT_RAM,
        },
    };
    Quota {
        exempt: base.exempt,
        instance_limit: override_.instance_limit.unwrap_or(base.instance_limit),
        max_cpu_cores: override_.max_cpu_cores.unwrap_or(base.max_cpu_cores),
        max_ram_bytes: override_.max_ram_bytes.unwrap_or(base.max_ram_bytes),
    }
}

/// Run the five-step pre-flight pipeline over plain values.
///
/// Returns `Ok(())` when the request fits, or the first `QuotaViolation`
/// encountered in the fixed step order. See the module docs for the steps and
/// their skip rules.
pub fn check(inputs: &CheckInputs) -> Result<(), QuotaViolation> {
    let quota = &inputs.effective_quota;

    // Step 1: per-user instance count (skipped for personally-exempt quotas).
    if !quota.exempt {
        let current = inputs.user_active_instances;
        let limit = quota.instance_limit;
        if current.saturating_add(1) > limit {
            return Err(QuotaViolation {
                scope: QuotaScope::UserInstance,
                current: current as i64,
                limit: limit as i64,
                requested: 1,
            });
        }
    }

    // Step 2: global host instance count (skipped when `host_instance_limit == 0`).
    if inputs.host_instance_limit > 0 {
        let current = inputs.host_active_instances;
        let limit = inputs.host_instance_limit;
        if current.saturating_add(1) > limit {
            return Err(QuotaViolation {
                scope: QuotaScope::HostInstance,
                current: current as i64,
                limit: limit as i64,
                requested: 1,
            });
        }
    }

    // Step 3: per-user CPU then RAM (skipped for personally-exempt quotas).
    if !quota.exempt {
        if inputs.user_active_cpu.saturating_add(inputs.requested_cores) > quota.max_cpu_cores {
            return Err(QuotaViolation {
                scope: QuotaScope::UserCpu,
                current: inputs.user_active_cpu as i64,
                limit: quota.max_cpu_cores as i64,
                requested: inputs.requested_cores as i64,
            });
        }
        if inputs
            .user_active_ram
            .saturating_add(inputs.requested_ram_bytes)
            > quota.max_ram_bytes
        {
            return Err(QuotaViolation {
                scope: QuotaScope::UserRam,
                current: inputs.user_active_ram,
                limit: quota.max_ram_bytes,
                requested: inputs.requested_ram_bytes,
            });
        }
    }

    // Step 4: host dedicated pool (dedicated allocations only).
    if inputs.mode == AllocationMode::Dedicated {
        let capacity = &inputs.host_capacity;
        if inputs
            .host_dedicated_active_cpu
            .saturating_add(inputs.requested_cores)
            > capacity.max_cpu_cores
        {
            return Err(QuotaViolation {
                scope: QuotaScope::HostDedicatedCpu,
                current: inputs.host_dedicated_active_cpu as i64,
                limit: capacity.max_cpu_cores as i64,
                requested: inputs.requested_cores as i64,
            });
        }
        if inputs
            .host_dedicated_active_ram
            .saturating_add(inputs.requested_ram_bytes)
            > capacity.max_ram_bytes
        {
            return Err(QuotaViolation {
                scope: QuotaScope::HostDedicatedRam,
                current: inputs.host_dedicated_active_ram,
                limit: capacity.max_ram_bytes,
                requested: inputs.requested_ram_bytes,
            });
        }
    }

    // Step 5: host shared fuse (shared allocations only, when the fuse is on).
    if inputs.mode == AllocationMode::Shared {
        if inputs.shared_max_cpu > 0
            && inputs
                .host_shared_active_cpu
                .saturating_add(inputs.requested_cores)
                > inputs.shared_max_cpu
        {
            return Err(QuotaViolation {
                scope: QuotaScope::HostSharedCpu,
                current: inputs.host_shared_active_cpu as i64,
                limit: inputs.shared_max_cpu as i64,
                requested: inputs.requested_cores as i64,
            });
        }
        if inputs.shared_max_ram > 0
            && inputs
                .host_shared_active_ram
                .saturating_add(inputs.requested_ram_bytes)
                > inputs.shared_max_ram
        {
            return Err(QuotaViolation {
                scope: QuotaScope::HostSharedRam,
                current: inputs.host_shared_active_ram,
                limit: inputs.shared_max_ram,
                requested: inputs.requested_ram_bytes,
            });
        }
    }

    Ok(())
}

/// One GiB, the unit role-default RAM quotas are expressed in.
const GIB: i64 = 1024 * 1024 * 1024;

/// Role-default RAM quotas: `user` = 8 GiB, `manager` = 32 GiB. Admin is
/// personally exempt (see `resolve_effective_quota`). A single source of
/// truth shared by `resolve_effective_quota` and the spec tests.
const USER_DEFAULT_RAM: i64 = 8 * GIB;
const MANAGER_DEFAULT_RAM: i64 = 32 * GIB;

#[cfg(test)]
mod tests {
    use super::*;

    fn quota(instance_limit: i32, max_cpu_cores: i32, max_ram_bytes: i64) -> Quota {
        Quota {
            exempt: false,
            instance_limit,
            max_cpu_cores,
            max_ram_bytes,
        }
    }

    fn exempt() -> Quota {
        Quota {
            exempt: true,
            instance_limit: 0,
            max_cpu_cores: 0,
            max_ram_bytes: 0,
        }
    }

    /// A request that fits comfortably under every limit; individual tests
    /// nudge one field at a time.
    fn base() -> CheckInputs {
        CheckInputs {
            mode: AllocationMode::Shared,
            requested_cores: 1,
            requested_ram_bytes: GIB,
            effective_quota: quota(10, 16, 64 * GIB),
            host_capacity: HostCapacity {
                max_cpu_cores: 32,
                max_ram_bytes: 128 * GIB,
            },
            host_instance_limit: 20,
            shared_max_cpu: 16,
            shared_max_ram: 64 * GIB,
            user_active_instances: 0,
            user_active_cpu: 0,
            user_active_ram: 0,
            host_active_instances: 0,
            host_dedicated_active_cpu: 0,
            host_dedicated_active_ram: 0,
            host_shared_active_cpu: 0,
            host_shared_active_ram: 0,
        }
    }

    #[test]
    fn everything_under_limits_passes() {
        assert_eq!(check(&base()), Ok(()));
    }

    #[test]
    fn user_instance_at_limit_passes() {
        let mut i = base();
        i.user_active_instances = i.effective_quota.instance_limit - 1;
        assert_eq!(check(&i), Ok(()));
    }

    #[test]
    fn user_instance_over_limit_rejects_with_scope_and_numbers() {
        let mut i = base();
        i.user_active_instances = i.effective_quota.instance_limit;
        assert_eq!(
            check(&i),
            Err(QuotaViolation {
                scope: QuotaScope::UserInstance,
                current: 10,
                limit: 10,
                requested: 1,
            })
        );
    }

    #[test]
    fn host_instance_at_limit_passes() {
        let mut i = base();
        i.host_active_instances = i.host_instance_limit - 1;
        assert_eq!(check(&i), Ok(()));
    }

    #[test]
    fn host_instance_over_limit_rejects() {
        let mut i = base();
        i.host_active_instances = i.host_instance_limit;
        assert_eq!(
            check(&i),
            Err(QuotaViolation {
                scope: QuotaScope::HostInstance,
                current: 20,
                limit: 20,
                requested: 1,
            })
        );
    }

    #[test]
    fn user_cpu_at_limit_passes() {
        let mut i = base();
        i.user_active_cpu = i.effective_quota.max_cpu_cores - i.requested_cores;
        assert_eq!(check(&i), Ok(()));
    }

    #[test]
    fn user_cpu_over_limit_rejects() {
        let mut i = base();
        i.user_active_cpu = i.effective_quota.max_cpu_cores;
        assert_eq!(
            check(&i),
            Err(QuotaViolation {
                scope: QuotaScope::UserCpu,
                current: 16,
                limit: 16,
                requested: 1,
            })
        );
    }

    #[test]
    fn user_ram_at_limit_passes() {
        let mut i = base();
        i.user_active_ram = i.effective_quota.max_ram_bytes - i.requested_ram_bytes;
        assert_eq!(check(&i), Ok(()));
    }

    #[test]
    fn user_ram_over_limit_rejects() {
        let mut i = base();
        i.user_active_ram = i.effective_quota.max_ram_bytes;
        assert_eq!(
            check(&i),
            Err(QuotaViolation {
                scope: QuotaScope::UserRam,
                current: 64 * GIB,
                limit: 64 * GIB,
                requested: GIB,
            })
        );
    }

    #[test]
    fn dedicated_cpu_at_limit_passes() {
        let mut i = base();
        i.mode = AllocationMode::Dedicated;
        i.host_dedicated_active_cpu = i.host_capacity.max_cpu_cores - i.requested_cores;
        assert_eq!(check(&i), Ok(()));
    }

    #[test]
    fn dedicated_cpu_over_limit_rejects() {
        let mut i = base();
        i.mode = AllocationMode::Dedicated;
        i.host_dedicated_active_cpu = i.host_capacity.max_cpu_cores;
        assert_eq!(
            check(&i),
            Err(QuotaViolation {
                scope: QuotaScope::HostDedicatedCpu,
                current: 32,
                limit: 32,
                requested: 1,
            })
        );
    }

    #[test]
    fn dedicated_ram_at_limit_passes() {
        let mut i = base();
        i.mode = AllocationMode::Dedicated;
        i.host_dedicated_active_ram = i.host_capacity.max_ram_bytes - i.requested_ram_bytes;
        assert_eq!(check(&i), Ok(()));
    }

    #[test]
    fn dedicated_ram_over_limit_rejects() {
        let mut i = base();
        i.mode = AllocationMode::Dedicated;
        i.host_dedicated_active_ram = i.host_capacity.max_ram_bytes;
        assert_eq!(
            check(&i),
            Err(QuotaViolation {
                scope: QuotaScope::HostDedicatedRam,
                current: 128 * GIB,
                limit: 128 * GIB,
                requested: GIB,
            })
        );
    }

    #[test]
    fn shared_cpu_at_limit_passes() {
        let mut i = base();
        i.host_shared_active_cpu = i.shared_max_cpu - i.requested_cores;
        assert_eq!(check(&i), Ok(()));
    }

    #[test]
    fn shared_cpu_over_limit_rejects() {
        let mut i = base();
        i.host_shared_active_cpu = i.shared_max_cpu;
        assert_eq!(
            check(&i),
            Err(QuotaViolation {
                scope: QuotaScope::HostSharedCpu,
                current: 16,
                limit: 16,
                requested: 1,
            })
        );
    }

    #[test]
    fn shared_ram_at_limit_passes() {
        let mut i = base();
        i.host_shared_active_ram = i.shared_max_ram - i.requested_ram_bytes;
        assert_eq!(check(&i), Ok(()));
    }

    #[test]
    fn shared_ram_over_limit_rejects() {
        let mut i = base();
        i.host_shared_active_ram = i.shared_max_ram;
        assert_eq!(
            check(&i),
            Err(QuotaViolation {
                scope: QuotaScope::HostSharedRam,
                current: 64 * GIB,
                limit: 64 * GIB,
                requested: GIB,
            })
        );
    }

    #[test]
    fn exempt_admin_skips_all_personal_checks() {
        let mut i = base();
        i.effective_quota = exempt();
        i.user_active_instances = 1000;
        i.user_active_cpu = 1000;
        i.user_active_ram = 1000 * GIB;
        assert_eq!(check(&i), Ok(()));
    }

    #[test]
    fn exempt_admin_global_instance_limit_still_enforced() {
        let mut i = base();
        i.effective_quota = exempt();
        i.user_active_instances = 1000;
        i.host_active_instances = i.host_instance_limit;
        assert_eq!(check(&i).unwrap_err().scope, QuotaScope::HostInstance);
    }

    #[test]
    fn exempt_admin_dedicated_pool_still_enforced() {
        let mut i = base();
        i.effective_quota = exempt();
        i.mode = AllocationMode::Dedicated;
        i.user_active_cpu = 1000;
        i.host_dedicated_active_cpu = i.host_capacity.max_cpu_cores;
        assert_eq!(
            check(&i).unwrap_err().scope,
            QuotaScope::HostDedicatedCpu
        );
    }

    #[test]
    fn exempt_admin_shared_fuse_still_enforced() {
        let mut i = base();
        i.effective_quota = exempt();
        i.user_active_ram = 1000 * GIB;
        i.host_shared_active_ram = i.shared_max_ram;
        assert_eq!(check(&i).unwrap_err().scope, QuotaScope::HostSharedRam);
    }

    #[test]
    fn zero_host_instance_limit_skips_global_count() {
        let mut i = base();
        i.host_instance_limit = 0;
        i.host_active_instances = 1000;
        assert_eq!(check(&i), Ok(()));
    }

    #[test]
    fn zero_host_instance_limit_keeps_per_user_check() {
        let mut i = base();
        i.host_instance_limit = 0;
        i.user_active_instances = i.effective_quota.instance_limit;
        assert_eq!(check(&i).unwrap_err().scope, QuotaScope::UserInstance);
    }

    #[test]
    fn disabled_shared_fuse_skips_both_checks() {
        let mut i = base();
        i.shared_max_cpu = 0;
        i.shared_max_ram = 0;
        i.host_shared_active_cpu = 1000;
        i.host_shared_active_ram = 1000 * GIB;
        assert_eq!(check(&i), Ok(()));
    }

    #[test]
    fn shared_fuse_field_off_skips_only_that_field() {
        let mut i = base();
        i.shared_max_cpu = 0;
        i.shared_max_ram = 16 * GIB;
        i.host_shared_active_cpu = 1000;
        i.host_shared_active_ram = 15 * GIB;
        assert_eq!(check(&i), Ok(()));
        i.host_shared_active_ram = 16 * GIB;
        assert_eq!(check(&i).unwrap_err().scope, QuotaScope::HostSharedRam);
    }

    #[test]
    fn shared_mode_skips_dedicated_pool() {
        let mut i = base();
        i.mode = AllocationMode::Shared;
        i.host_dedicated_active_cpu = i.host_capacity.max_cpu_cores;
        i.host_dedicated_active_ram = i.host_capacity.max_ram_bytes;
        assert_eq!(check(&i), Ok(()));
    }

    #[test]
    fn dedicated_mode_skips_shared_fuse() {
        let mut i = base();
        i.mode = AllocationMode::Dedicated;
        i.host_shared_active_cpu = 1000;
        i.host_shared_active_ram = 1000 * GIB;
        assert_eq!(check(&i), Ok(()));
    }

    #[test]
    fn earlier_step_wins_over_later_step() {
        let mut i = base();
        i.user_active_instances = i.effective_quota.instance_limit;
        i.host_active_instances = i.host_instance_limit;
        assert_eq!(check(&i).unwrap_err().scope, QuotaScope::UserInstance);

        let mut i = base();
        i.host_active_instances = i.host_instance_limit;
        i.user_active_cpu = i.effective_quota.max_cpu_cores;
        assert_eq!(check(&i).unwrap_err().scope, QuotaScope::HostInstance);
    }

    #[test]
    fn user_cpu_checked_before_user_ram() {
        let mut i = base();
        i.user_active_cpu = i.effective_quota.max_cpu_cores;
        i.user_active_ram = i.effective_quota.max_ram_bytes;
        assert_eq!(check(&i).unwrap_err().scope, QuotaScope::UserCpu);
    }

    #[test]
    fn user_ram_fires_when_cpu_fits() {
        let mut i = base();
        i.user_active_ram = i.effective_quota.max_ram_bytes;
        assert_eq!(check(&i).unwrap_err().scope, QuotaScope::UserRam);
    }

    #[test]
    fn dedicated_cpu_checked_before_dedicated_ram() {
        let mut i = base();
        i.mode = AllocationMode::Dedicated;
        i.host_dedicated_active_cpu = i.host_capacity.max_cpu_cores;
        i.host_dedicated_active_ram = i.host_capacity.max_ram_bytes;
        assert_eq!(check(&i).unwrap_err().scope, QuotaScope::HostDedicatedCpu);
    }

    #[test]
    fn shared_cpu_checked_before_shared_ram() {
        let mut i = base();
        i.host_shared_active_cpu = i.shared_max_cpu;
        i.host_shared_active_ram = i.shared_max_ram;
        assert_eq!(check(&i).unwrap_err().scope, QuotaScope::HostSharedCpu);
    }

    #[test]
    fn user_ram_wins_over_dedicated_pool() {
        let mut i = base();
        i.mode = AllocationMode::Dedicated;
        i.user_active_ram = i.effective_quota.max_ram_bytes;
        i.host_dedicated_active_ram = i.host_capacity.max_ram_bytes;
        assert_eq!(check(&i).unwrap_err().scope, QuotaScope::UserRam);
    }

    #[test]
    fn user_defaults_match_spec() {
        let q = resolve_effective_quota(QuotaOverride::default(), Role::User);
        assert_eq!(
            q,
            Quota {
                exempt: false,
                instance_limit: 2,
                max_cpu_cores: 4,
                max_ram_bytes: USER_DEFAULT_RAM,
            }
        );
    }

    #[test]
    fn manager_defaults_match_spec() {
        let q = resolve_effective_quota(QuotaOverride::default(), Role::Manager);
        assert_eq!(
            q,
            Quota {
                exempt: false,
                instance_limit: 5,
                max_cpu_cores: 12,
                max_ram_bytes: MANAGER_DEFAULT_RAM,
            }
        );
    }

    #[test]
    fn admin_is_personally_exempt() {
        let q = resolve_effective_quota(QuotaOverride::default(), Role::Admin);
        assert!(q.exempt);
    }

    #[test]
    fn override_wins_per_field_and_none_inherits() {
        let q = resolve_effective_quota(
            QuotaOverride {
                instance_limit: Some(7),
                max_cpu_cores: None,
                max_ram_bytes: Some(16 * GIB),
            },
            Role::User,
        );
        assert_eq!(
            q,
            Quota {
                exempt: false,
                instance_limit: 7,
                max_cpu_cores: 4,
                max_ram_bytes: 16 * GIB,
            }
        );
    }

    #[test]
    fn admin_override_does_not_lift_exemption() {
        let q = resolve_effective_quota(
            QuotaOverride {
                instance_limit: Some(99),
                max_cpu_cores: Some(99),
                max_ram_bytes: Some(999 * GIB),
            },
            Role::Admin,
        );
        assert!(q.exempt);
        assert_eq!(q.instance_limit, 99);
    }
}
