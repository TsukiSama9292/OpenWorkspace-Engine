//! Effective-context policy engine: the pure successor to the `quota.rs`
//! policy module. Every permission and ceiling decision lives behind the two
//! functions in this module — no DB, no Docker, no locks — so the whole
//! policy surface is unit-testable in isolation (spec Decision 2).

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::str::FromStr;
use uuid::Uuid;

/// The derived tier of a group kind, and of the user that belongs to it.
/// `admin` = 2, `manager` = 1, everything else (user/custom/none) = 0.
pub const TIER_USER: i32 = 0;
pub const TIER_MANAGER: i32 = 1;
pub const TIER_ADMIN: i32 = 2;

/// The group `kind` is the machine identity the tier hierarchy is built on:
/// names are cosmetic (spec Decision 2 / 9).
pub fn group_kind_tier(kind: Option<&str>) -> i32 {
    match kind {
        Some("admin") => TIER_ADMIN,
        Some("manager") => TIER_MANAGER,
        _ => TIER_USER,
    }
}

/// The resolved permissions and ceilings for a user at a point in time. This
/// struct is serialized as the `/auth/me` envelope, so its field names are
/// part of the public API contract.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, utoipa::ToSchema)]
pub struct EffectiveContext {
    pub user_id: Uuid,
    pub username: String,
    /// Root: derived from Admin-group membership, never stored (spec Decision
    /// 4). Replaces the dropped `is_system_admin` column.
    pub is_admin: bool,
    /// The maximum tier across the user's group memberships (0/1/2).
    pub tier: i32,
    pub can_create_template: bool,
    pub can_manage_users: bool,
    pub can_manage_group_instances: bool,
    pub can_manage_docker: bool,
    pub can_manage_registry: bool,
    /// Monitor-dashboard gate: seeing the Monitor tab / snapshot endpoint
    /// (admin or a group flag; Manager system group defaults on).
    pub can_view_monitoring: bool,
    /// Audit-log viewer gate: the Logs tab / audit query endpoint (admin or a
    /// group flag; Manager system group defaults on). Audit data is never
    /// leaked to tenants without the flag (spec Decision 2).
    pub can_view_audit_logs: bool,
    /// `-1` means "no ceiling" (matches the `host_instance_limit = -1`
    /// convention); `0` blocks every launch; positive values are exact per-user
    /// limits. Resolved as the *maximum* of the personal ceiling and every
    /// group ceiling, where `-1`/NULL = unlimited is the highest (spec
    /// Decision 4).
    pub effective_max_instances: i32,
    /// Union of every member group's whitelist (group-only authorization). No
    /// personal whitelist and no creator self-whitelist (spec Decision 4/5).
    /// Hidden templates are always excluded — they never appear in the
    /// whitelist, so clients can treat this list as "everything I may launch".
    pub allowed_template_ids: Vec<Uuid>,
    pub group_ids: Vec<Uuid>,
    /// The personal override, if any. The effective ceiling is the max of this
    /// and the group maxima, so a personal ceiling can only raise, never lower,
    /// what the groups grant.
    pub direct_max_instances: Option<i32>,
    /// The resource pools and billing model of every group the user belongs to,
    /// so the launch form can offer a billing target and show its pool. Read
    /// directly from the group rows; informational only.
    pub group_billing: Vec<GroupBilling>,
    /// The aggregate resource pool across the user's member groups (`-1` =
    /// unlimited, and an unlimited pool wins over every finite one). Reporting
    /// only — the launch check uses the pool of the *chosen* billing group.
    pub resource_quotas: ResourceUse,
}

/// The resource pool a group offers its members, as serialized on
/// `EffectiveContext.group_billing`. Carries the group's display name and
/// tier plus the user's own per-membership cap so the launch form can offer a
/// billing target, show its pool, and default to the highest-cap membership.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, utoipa::ToSchema)]
pub struct GroupBilling {
    pub group_id: Uuid,
    pub group_name: String,
    /// Derived from the group kind (Admin=2, Manager=1, else 0); used only for
    /// picker tie-breaks.
    pub tier: i32,
    /// `shared` | `dedicated`.
    pub billing_model: String,
    /// `-1` means "unlimited".
    pub pool_cpu_cores: i64,
    pub pool_memory_mb: i64,
    pub pool_gpu_count: i64,
    /// The user's own per-member cap inside this group (`-1` = unlimited,
    /// `0` = blocked), so the picker can default to the highest-cap
    /// membership. Mirrors the member layer of the quota pre-flight.
    pub member_cpu_cores: i64,
    pub member_memory_mb: i64,
    pub member_gpu_count: i64,
}

/// The pure inputs for `calculate_effective_context`: the user's identity and
/// personal policy row (personal ceiling only — the admin flag and owned
/// templates are no longer inputs).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UserPolicy {
    pub user_id: Uuid,
    pub username: String,
    pub direct_max_instances: Option<i32>,
}

/// A group's policy as seen by the effective-context computation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GroupPolicy {
    pub id: Uuid,
    /// Display name, surfaced on the billing picker and tie-breaks.
    pub name: String,
    /// `admin` | `manager` | `user` | `None` (custom groups). Only the kind
    /// feeds tier derivation; names are cosmetic.
    pub kind: Option<String>,
    /// `None` (NULL, "inherit") and `-1` both mean "unlimited"; `0` blocks;
    /// any positive value is a hard ceiling.
    pub max_instances: Option<i32>,
    /// How member instances bill resources against the pool: `shared` (the
    /// whole group's active instances sum together) or `dedicated` (each
    /// member's own instances sum against the pool).
    pub billing_model: String,
    /// The group's resource pool; `-1` means "unlimited" (matches the ceiling
    /// convention), `0` is blocked.
    pub pool_cpu_cores: i64,
    pub pool_memory_mb: i64,
    pub pool_gpu_count: i64,
    /// The user's own per-member cap inside this group (`-1` = unlimited,
    /// `0` = blocked). Surfaced on the billing picker so it can default to
    /// the highest-cap membership.
    pub member_cpu_cores: i64,
    pub member_memory_mb: i64,
    pub member_gpu_count: i64,
    pub can_create_template: bool,
    pub can_manage_users: bool,
    pub can_manage_group_instances: bool,
    pub can_manage_docker: bool,
    pub can_manage_registry: bool,
    pub can_view_monitoring: bool,
    pub can_view_audit_logs: bool,
}

/// Compute a user's effective context (spec Decision 4):
///
/// 1. `tier` = the maximum kind tier across the user's groups;
///    `is_admin` = tier 2 (Admin-group membership).
/// 2. Flags = OR across all the user's groups (admin groups carry every flag).
/// 3. Whitelist = the union of every member group's whitelist — no personal
///    whitelist, no creator self-whitelist, no admin bypass. Hidden templates
///    are stripped from the union, so they are never exposed as launchable
///    (spec Decision 3); `pre_flight` still hard-rejects them independently.
/// 4. `effective_max_instances` = the maximum of the personal ceiling and
///    every group ceiling, where `-1`/NULL = unlimited is the highest.
///
/// An empty whitelist is default-deny: `pre_flight` rejects every template for
/// every user with no group grants — admins included.
pub fn calculate_effective_context(
    user: &UserPolicy,
    groups: &[GroupPolicy],
    group_template_ids: &HashMap<Uuid, Vec<Uuid>>,
    hidden_template_ids: &[Uuid],
) -> EffectiveContext {
    let tier = groups
        .iter()
        .map(|g| group_kind_tier(g.kind.as_deref()))
        .max()
        .unwrap_or(TIER_USER);

    // No group memberships and no personal ceiling: no constraint at all →
    // unlimited (matching the legacy `0` = unlimited semantics, now `-1`).
    let mut unlimited = user.direct_max_instances.is_none() && groups.is_empty();
    let mut max_finite = 0;
    if let Some(direct) = user.direct_max_instances {
        if direct < 0 {
            unlimited = true;
        } else {
            max_finite = max_finite.max(direct);
        }
    }
    for group in groups {
        match group.max_instances {
            None => unlimited = true,
            Some(limit) if limit < 0 => unlimited = true,
            Some(ceiling) => max_finite = max_finite.max(ceiling),
        }
    }
    let effective_max_instances = if unlimited { -1 } else { max_finite };

    let mut allowed_template_ids = Vec::new();
    for group in groups {
        if let Some(ids) = group_template_ids.get(&group.id) {
            for &template_id in ids {
                if !hidden_template_ids.contains(&template_id)
                    && !allowed_template_ids.contains(&template_id)
                {
                    allowed_template_ids.push(template_id);
                }
            }
        }
    }

    let group_ids: Vec<Uuid> = groups.iter().map(|g| g.id).collect();

    // Per-group billing surfaces: the launch form offers one of the user's own
    // groups as a billing target and shows its pool.
    let group_billing: Vec<GroupBilling> = groups
        .iter()
        .map(|g| GroupBilling {
            group_id: g.id,
            group_name: g.name.clone(),
            tier: group_kind_tier(g.kind.as_deref()),
            billing_model: g.billing_model.clone(),
            pool_cpu_cores: g.pool_cpu_cores,
            pool_memory_mb: g.pool_memory_mb,
            pool_gpu_count: g.pool_gpu_count,
            member_cpu_cores: g.member_cpu_cores,
            member_memory_mb: g.member_memory_mb,
            member_gpu_count: g.member_gpu_count,
        })
        .collect();

    // Aggregate pool: any unlimited group pool makes the aggregate unlimited;
    // otherwise the maximum finite pool wins (mirrors the ceiling resolution).
    let mut pool_unlimited = [false, false, false];
    let mut pool_max = [0i64, 0i64, 0i64];
    for g in groups {
        let pools = [g.pool_cpu_cores, g.pool_memory_mb, g.pool_gpu_count];
        for (i, pool) in pools.into_iter().enumerate() {
            if pool < 0 {
                pool_unlimited[i] = true;
            } else {
                pool_max[i] = pool_max[i].max(pool);
            }
        }
    }
    let resource_quotas = ResourceUse {
        cpu_cores: if pool_unlimited[0] { -1 } else { pool_max[0] },
        memory_mb: if pool_unlimited[1] { -1 } else { pool_max[1] },
        gpu_count: if pool_unlimited[2] { -1 } else { pool_max[2] },
    };

    EffectiveContext {
        user_id: user.user_id,
        username: user.username.clone(),
        is_admin: tier == TIER_ADMIN,
        tier,
        can_create_template: groups.iter().any(|g| g.can_create_template),
        can_manage_users: groups.iter().any(|g| g.can_manage_users),
        can_manage_group_instances: groups
            .iter()
            .any(|g| g.can_manage_group_instances),
        can_manage_docker: groups.iter().any(|g| g.can_manage_docker),
        can_manage_registry: groups.iter().any(|g| g.can_manage_registry),
        can_view_monitoring: groups.iter().any(|g| g.can_view_monitoring),
        can_view_audit_logs: groups.iter().any(|g| g.can_view_audit_logs),
        effective_max_instances,
        allowed_template_ids,
        group_ids,
        direct_max_instances: user.direct_max_instances,
        group_billing,
        resource_quotas,
    }
}

/// Why a launch attempt was rejected, in check order (spec Decision 2).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PreflightReject {
    /// Template is not in the effective whitelist (default-deny, 403).
    TemplateNotAllowed { requested_template_id: Uuid },
    /// Template is hidden: an absolute off-switch checked before the whitelist
    /// (403). No tier is exempt, admins included (spec Decision 3).
    TemplateHidden { requested_template_id: Uuid },
    /// The user is at their exact effective ceiling (409).
    InstanceCeilingExceeded { current: i32, limit: i32 },
    /// The global active count is at the host ceiling (409, best-effort).
    HostCeilingExceeded { current: i32, limit: i32 },
    /// The host-wide usage of a resource would exceed the host ceiling (409).
    /// `0` caps are disabled and never reject.
    HostResourceExceeded {
        resource: ResourceKind,
        current: i64,
        limit: i64,
        requested: i64,
    },
    /// The billing target's pool cannot fit the requested instance (409). For
    /// a group target, `group_id` is the charged group; `None` means a
    /// self-billed launch (never rejected — the pool is unlimited).
    PoolResourceExceeded {
        resource: ResourceKind,
        current: i64,
        limit: i64,
        requested: i64,
        group_id: Option<Uuid>,
    },
    /// The launch's owner is at their personal resource cap inside the billing
    /// group (409). This is the per-member layer, checked before the group
    /// pool (spec Decision 5).
    MemberResourceExceeded {
        resource: ResourceKind,
        current: i64,
        limit: i64,
        requested: i64,
        group_id: Option<Uuid>,
    },
}

/// Which resource failed a resource-cap check. `as_str` is the machine-readable
/// value on the rejection body.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResourceKind {
    Cpu,
    Memory,
    Gpu,
}

impl ResourceKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            ResourceKind::Cpu => "cpu",
            ResourceKind::Memory => "memory",
            ResourceKind::Gpu => "gpu",
        }
    }
}

/// A snapshot of resource usage and its caps at a point in time. `-1` always
/// means "unlimited" for caps; `0` is a real zero (a blocked cap for quota
/// fields, a zero-cost request for template resources).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, utoipa::ToSchema)]
pub struct ResourceUse {
    pub cpu_cores: i64,
    pub memory_mb: i64,
    pub gpu_count: i64,
}

/// The resource context for a launch/restart pre-flight: the caps and current
/// usage of the host and the launch's billing target (the charged group, or a
/// self-bill). For a group-billed launch, the member cap is the owner's
/// personal quota inside the charged group.
#[derive(Debug, Clone)]
pub struct QuotaContext {
    /// The pool the charged target offers (`-1` = unlimited).
    pub pool: ResourceUse,
    /// Resources already billed to the charged target.
    pub billed: ResourceUse,
    /// The launch owner's personal cap inside the charged group (`-1` =
    /// unlimited). Ignored for self-billed launches.
    pub member_quota: ResourceUse,
    /// The owner's usage already billed against their personal cap.
    pub member_used: ResourceUse,
    /// Host-wide capacity (`-1` = disabled).
    pub host_capacity: ResourceUse,
    /// Host-wide usage across all active instances.
    pub host_used: ResourceUse,
    /// The group this launch bills against (`None` = self-billed).
    pub target_group_id: Option<Uuid>,
}

/// Sum two `ResourceUse` values elementwise.
pub fn add_use(a: &ResourceUse, b: &ResourceUse) -> ResourceUse {
    ResourceUse {
        cpu_cores: a.cpu_cores + b.cpu_cores,
        memory_mb: a.memory_mb + b.memory_mb,
        gpu_count: a.gpu_count + b.gpu_count,
    }
}

/// Convert a template memory value (bytes; `-1` = unlimited) to the quota
/// layer's MB. `-1` is preserved; a positive byte count rounds up to a whole
/// MB so a request is never under-counted against a quota.
pub fn memory_bytes_to_mb(bytes: i64) -> i64 {
    if bytes < 0 {
        return bytes;
    }
    const MB: i64 = 1024 * 1024;
    bytes / MB + i64::from(bytes % MB != 0)
}

/// A template's launch visibility — the per-template override that sits above
/// the group whitelist (spec Decision 3):
///
/// - `public` — every authenticated user may launch it, whitelist skipped;
/// - `private` (default) — only users whose groups are whitelisted may launch;
/// - `hidden` — nobody may launch it, not even the owner or admins.
///
/// The three literals are the serialized values of the `visibility` column and
/// the API contract.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
#[derive(Default)]
pub enum TemplateVisibility {
    Public,
    #[default]
    Private,
    Hidden,
}

impl TemplateVisibility {
    pub fn as_str(&self) -> &'static str {
        match self {
            TemplateVisibility::Public => "public",
            TemplateVisibility::Private => "private",
            TemplateVisibility::Hidden => "hidden",
        }
    }
}


impl FromStr for TemplateVisibility {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "public" => Ok(TemplateVisibility::Public),
            "private" => Ok(TemplateVisibility::Private),
            "hidden" => Ok(TemplateVisibility::Hidden),
            _ => Err(format!("invalid template visibility: {s}")),
        }
    }
}

/// Run the launch pre-flight checks as a pure decision function, with the
/// template's visibility applied first (spec Decision 3):
///
/// 1. `hidden` → `403` before the whitelist is consulted (no bypass, admins
///    included); `public` → skip the whitelist check; `private` → whitelist.
/// 2. `requested_template_id` not in the whitelist → `403` (every tier —
///    admins are authorized group-only, spec Decision 4/5).
/// 3. `active_own_count + 1 > effective_max_instances` → `409` (unless the
///    ceiling is `-1`, meaning no limit; `0` blocks every launch). Public
///    grants permission, not quota.
/// 4. `host_instance_limit >= 0` and `host_active_count + 1 > host_instance_limit`
///    → `409` (`-1` = no limit, `0` = blocked). No tier is exempt: admin
///    instances still count toward the host limit, so the global check always
///    runs.
/// 5. For each resource, when the launch owner's member cap is enabled
///    (`>= 0`): `member_used + requested > member_quota` → `409`. The member
///    layer sits between the instance ceiling and the pool (spec Decision 5):
///    a member with a small personal cap is stopped before their own group's
///    pool is even consulted.
/// 6. For each resource, when the billing target's pool is enabled (`>= 0`):
///    `billed + requested > pool` → `409`. The `pool`/`billed` come from the
///    launch's billing target — the charged group, or an unlimited self-bill
///    (`QuotaContext.pool` all `-1`) that never rejects.
/// 7. For each resource, when the host ceiling is enabled (`>= 0`):
///    `host_used + requested > host_capacity` → `409` for a positive request.
///    Host resource caps are global and apply to every tier.
///
/// Resource check order per resource is fixed: member cap, then pool, then
/// host caps.
pub fn pre_flight(
    context: &EffectiveContext,
    host_active_count: i32,
    host_instance_limit: i32,
    requested_template_id: Uuid,
    active_own_count: i32,
    template_visibility: TemplateVisibility,
    requested_resources: &ResourceUse,
    billing: &QuotaContext,
) -> Result<(), PreflightReject> {
    match template_visibility {
        TemplateVisibility::Hidden => {
            return Err(PreflightReject::TemplateHidden {
                requested_template_id,
            });
        }
        TemplateVisibility::Public => {}
        TemplateVisibility::Private => {
            if !context.allowed_template_ids.contains(&requested_template_id) {
                return Err(PreflightReject::TemplateNotAllowed {
                    requested_template_id,
                });
            }
        }
    }

    if context.effective_max_instances >= 0
        && active_own_count + 1 > context.effective_max_instances
    {
        return Err(PreflightReject::InstanceCeilingExceeded {
            current: active_own_count,
            limit: context.effective_max_instances,
        });
    }

    if host_instance_limit >= 0 && host_active_count + 1 > host_instance_limit {
        return Err(PreflightReject::HostCeilingExceeded {
            current: host_active_count,
            limit: host_instance_limit,
        });
    }

    // Resource checks run per resource so the `-1` request rule (spec Decision
    // on unlimited requests) can inspect every layer before deciding. Order per
    // resource is fixed: member cap, then chosen group pool, then host caps.
    let resources = [
        (
            ResourceKind::Cpu,
            billing.member_quota.cpu_cores,
            billing.member_used.cpu_cores,
            billing.pool.cpu_cores,
            billing.billed.cpu_cores,
            billing.host_capacity.cpu_cores,
            billing.host_used.cpu_cores,
            requested_resources.cpu_cores,
        ),
        (
            ResourceKind::Memory,
            billing.member_quota.memory_mb,
            billing.member_used.memory_mb,
            billing.pool.memory_mb,
            billing.billed.memory_mb,
            billing.host_capacity.memory_mb,
            billing.host_used.memory_mb,
            requested_resources.memory_mb,
        ),
        (
            ResourceKind::Gpu,
            billing.member_quota.gpu_count,
            billing.member_used.gpu_count,
            billing.pool.gpu_count,
            billing.billed.gpu_count,
            billing.host_capacity.gpu_count,
            billing.host_used.gpu_count,
            requested_resources.gpu_count,
        ),
    ];

    for (
        resource,
        member_limit,
        member_current,
        pool_limit,
        pool_current,
        host_limit,
        host_current,
        requested,
    ) in resources
    {
        // `-1` request rule (spec Decision): a template resource of `-1` asks
        // for unlimited of that resource and is accepted only when that
        // resource's limit is `-1` (unlimited) at every checked layer —
        // otherwise the launch is refused at the first binding layer (member
        // before pool before host). A request of `0` is a finite zero-cost
        // request (it consumes nothing) and is always fine. A positive request
        // is compared as `current + requested ≤ limit` against every finite
        // layer.
        if requested < 0 {
            if member_limit >= 0 {
                return Err(PreflightReject::MemberResourceExceeded {
                    resource,
                    current: member_current,
                    limit: member_limit,
                    requested,
                    group_id: billing.target_group_id,
                });
            }
            if pool_limit >= 0 {
                return Err(PreflightReject::PoolResourceExceeded {
                    resource,
                    current: pool_current,
                    limit: pool_limit,
                    requested,
                    group_id: billing.target_group_id,
                });
            }
            if host_limit >= 0 {
                return Err(PreflightReject::HostResourceExceeded {
                    resource,
                    current: host_current,
                    limit: host_limit,
                    requested,
                });
            }
        } else if requested > 0 {
            if member_limit >= 0 && member_current + requested > member_limit {
                return Err(PreflightReject::MemberResourceExceeded {
                    resource,
                    current: member_current,
                    limit: member_limit,
                    requested,
                    group_id: billing.target_group_id,
                });
            }
            if pool_limit >= 0 && pool_current + requested > pool_limit {
                return Err(PreflightReject::PoolResourceExceeded {
                    resource,
                    current: pool_current,
                    limit: pool_limit,
                    requested,
                    group_id: billing.target_group_id,
                });
            }
            if host_limit >= 0 && host_current + requested > host_limit {
                return Err(PreflightReject::HostResourceExceeded {
                    resource,
                    current: host_current,
                    limit: host_limit,
                    requested,
                });
            }
        }
    }

    Ok(())
}

/// Whether an actor of `actor_tier` may assign a target to the given groups:
/// every group's kind tier must be strictly below the actor's tier, so a
/// Manager (1) can never place anyone in Manager/Admin groups, and a tier-0
/// actor can place nobody anywhere (spec Decision 6).
pub fn can_assign_groups(actor_tier: i32, group_kinds: &[Option<String>]) -> bool {
    group_kinds
        .iter()
        .all(|kind| group_kind_tier(kind.as_deref()) < actor_tier)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn uuid(n: u128) -> Uuid {
        Uuid::from_u128(n)
    }

    fn user(id: Uuid, direct_max_instances: Option<i32>) -> UserPolicy {
        UserPolicy {
            user_id: id,
            username: format!("user-{}", id),
            direct_max_instances,
        }
    }

    fn group(
        id: Uuid,
        kind: Option<&str>,
        max_instances: Option<i32>,
        create: bool,
        manage_users: bool,
        group_instances: bool,
        docker: bool,
        registry: bool,
        monitoring: bool,
        audit_logs: bool,
    ) -> GroupPolicy {
        GroupPolicy {
            id,
            name: format!("group-{}", id),
            kind: kind.map(|k| k.to_string()),
            max_instances,
            billing_model: "shared".to_string(),
            pool_cpu_cores: 0,
            pool_memory_mb: 0,
            pool_gpu_count: 0,
            member_cpu_cores: -1,
            member_memory_mb: -1,
            member_gpu_count: -1,
            can_create_template: create,
            can_manage_users: manage_users,
            can_manage_group_instances: group_instances,
            can_manage_docker: docker,
            can_manage_registry: registry,
            can_view_monitoring: monitoring,
            can_view_audit_logs: audit_logs,
        }
    }

    fn map(pairs: &[(Uuid, &[Uuid])]) -> HashMap<Uuid, Vec<Uuid>> {
        pairs
            .iter()
            .map(|(g, ids)| (*g, ids.to_vec()))
            .collect()
    }

    #[test]
    fn flags_or_across_groups() {
        let alice = user(uuid(1), None);
        let g1 = group(uuid(10), None, Some(2), true, false, false, false, false, false, false);
        let g2 = group(uuid(11), None, Some(4), false, true, false, true, false, false, false);
        let g3 = group(uuid(12), None, Some(6), false, false, false, false, true, false, false);

        let ctx = calculate_effective_context(&alice, &[g1, g2, g3], &map(&[]), &[]);

        assert!(!ctx.is_admin);
        assert_eq!(ctx.tier, TIER_USER);
        assert!(ctx.can_create_template);
        assert!(ctx.can_manage_users);
        assert!(!ctx.can_manage_group_instances);
        assert!(ctx.can_manage_docker);
        assert!(ctx.can_manage_registry);
        assert!(!ctx.can_view_monitoring);
        assert!(!ctx.can_view_audit_logs);
    }

    #[test]
    fn monitoring_flag_or_across_groups() {
        let alice = user(uuid(1), None);
        let g1 = group(uuid(10), None, Some(2), false, false, false, false, false, true, false);
        let g2 = group(uuid(11), None, Some(2), false, false, false, false, false, false, false);

        let ctx = calculate_effective_context(&alice, &[g1, g2], &map(&[]), &[]);
        assert!(ctx.can_view_monitoring);
        assert!(!ctx.can_view_audit_logs);
    }

    #[test]
    fn audit_logs_flag_or_across_groups() {
        let alice = user(uuid(1), None);
        let g1 = group(uuid(10), None, Some(2), false, false, false, false, false, false, true);
        let g2 = group(uuid(11), None, Some(2), false, false, false, false, false, false, false);

        let ctx = calculate_effective_context(&alice, &[g1, g2], &map(&[]), &[]);
        assert!(ctx.can_view_audit_logs);
        assert!(!ctx.can_view_monitoring);
    }

    #[test]
    fn whitelist_union_is_group_only() {
        let t1 = uuid(100);
        let t2 = uuid(101);
        let t3 = uuid(102);
        let alice = user(uuid(1), None);
        let g1 = group(uuid(10), None, Some(2), false, false, false, false, false, false, false);
        let g2 = group(uuid(11), None, Some(2), false, false, false, false, false, false, false);

        // `user` no longer carries owned/personal template ids — only the
        // groups' whitelists feed the union.
        let ctx = calculate_effective_context(
            &alice,
            &[g1, g2],
            &map(&[(uuid(10), &[t2, t3]), (uuid(11), &[t1])]),
            &[],
        );

        assert_eq!(ctx.allowed_template_ids, vec![t2, t3, t1]);
    }

    #[test]
    fn effective_ceiling_is_max_of_direct_and_groups() {
        // A personal ceiling can raise above the groups but never below them.
        let alice = user(uuid(1), Some(8));
        let g1 = group(uuid(10), None, Some(2), false, false, false, false, false, false, false);
        let g2 = group(uuid(11), None, Some(4), false, false, false, false, false, false, false);

        let ctx = calculate_effective_context(&alice, &[g1, g2], &map(&[]), &[]);
        assert_eq!(ctx.effective_max_instances, 8);
        assert_eq!(ctx.direct_max_instances, Some(8));
    }

    #[test]
    fn group_ceiling_raises_personal_ceiling() {
        let alice = user(uuid(1), Some(2));
        let g1 = group(uuid(10), None, Some(8), false, false, false, false, false, false, false);

        let ctx = calculate_effective_context(&alice, &[g1], &map(&[]), &[]);
        assert_eq!(ctx.effective_max_instances, 8);
    }

    #[test]
    fn group_max_fallback_when_no_direct() {
        let alice = user(uuid(1), None);
        let g1 = group(uuid(10), None, Some(2), false, false, false, false, false, false, false);
        let g2 = group(uuid(11), None, Some(6), false, false, false, false, false, false, false);

        let ctx = calculate_effective_context(&alice, &[g1, g2], &map(&[]), &[]);
        assert_eq!(ctx.effective_max_instances, 6);
        assert_eq!(ctx.direct_max_instances, None);
    }

    #[test]
    fn direct_minus_one_means_no_ceiling() {
        let alice = user(uuid(1), Some(-1));
        let g1 = group(uuid(10), None, Some(2), false, false, false, false, false, false, false);

        let ctx = calculate_effective_context(&alice, &[g1], &map(&[]), &[]);
        assert_eq!(ctx.effective_max_instances, -1);
    }

    #[test]
    fn direct_zero_contributes_nothing_to_the_max() {
        // `0` is a real zero: a blocked personal ceiling adds nothing to the
        // maximum — the group ceiling still applies.
        let alice = user(uuid(1), Some(0));
        let g1 = group(uuid(10), None, Some(2), false, false, false, false, false, false, false);

        let ctx = calculate_effective_context(&alice, &[g1], &map(&[]), &[]);
        assert_eq!(ctx.effective_max_instances, 2);
    }

    #[test]
    fn null_group_ceiling_means_no_ceiling() {
        // Admin group: NULL max_instances → unlimited (-1), even next to a
        // finite personal ceiling.
        let admin = user(uuid(1), Some(3));
        let g1 = group(uuid(10), Some("admin"), None, true, true, true, true, true, true, false);

        let ctx = calculate_effective_context(&admin, &[g1], &map(&[]), &[]);
        assert_eq!(ctx.effective_max_instances, -1);
        assert!(ctx.is_admin);
        assert_eq!(ctx.tier, TIER_ADMIN);
    }

    #[test]
    fn no_groups_no_direct_is_unlimited_but_default_deny() {
        let alice = user(uuid(1), None);

        let ctx = calculate_effective_context(&alice, &[], &map(&[]), &[]);

        assert!(!ctx.is_admin);
        assert_eq!(ctx.tier, TIER_USER);
        assert_eq!(ctx.effective_max_instances, -1);
        assert!(ctx.allowed_template_ids.is_empty());
        assert!(!ctx.can_create_template);
        assert!(!ctx.can_manage_users);
        assert!(!ctx.can_manage_group_instances);
        assert!(!ctx.can_manage_docker);
        assert!(!ctx.can_manage_registry);
        assert!(!ctx.can_view_monitoring);
        assert!(!ctx.can_view_audit_logs);
        assert!(ctx.group_ids.is_empty());
    }

    #[test]
    fn admin_membership_is_root_but_not_bypass() {
        // Admin group grants root and every flag, but the whitelist still comes
        // only from group grants — no bypass.
        let admin = user(uuid(1), None);
        let g1 = group(uuid(10), Some("admin"), Some(2), true, true, true, true, true, true, true);

        let ctx = calculate_effective_context(&admin, &[g1], &map(&[]), &[]);

        assert!(ctx.is_admin);
        assert_eq!(ctx.tier, TIER_ADMIN);
        assert!(ctx.can_create_template);
        assert!(ctx.can_manage_users);
        assert!(ctx.can_manage_group_instances);
        assert!(ctx.can_manage_docker);
        assert!(ctx.can_manage_registry);
        assert!(ctx.can_view_monitoring);
        assert!(ctx.can_view_audit_logs);
        assert!(ctx.allowed_template_ids.is_empty(), "admins are not exempt");
        assert_eq!(ctx.effective_max_instances, 2);
        assert_eq!(ctx.group_ids, vec![uuid(10)]);
    }

    #[test]
    fn manager_membership_tier() {
        let alice = user(uuid(1), None);
        let g1 = group(uuid(10), Some("manager"), Some(4), false, false, false, false, false, false, false);
        let g2 = group(uuid(11), Some("user"), Some(1), false, false, false, false, false, false, false);

        let ctx = calculate_effective_context(&alice, &[g1, g2], &map(&[]), &[]);
        assert_eq!(ctx.tier, TIER_MANAGER);
        assert!(!ctx.is_admin);
        // The User group's cap of 1 must not drag a Manager down.
        assert_eq!(ctx.effective_max_instances, 4);
    }

    #[test]
    fn custom_group_named_admin_is_not_root() {
        // Identity is by kind: a custom group named "Admin" is tier 0.
        let alice = user(uuid(1), None);
        let g1 = group(uuid(10), None, Some(2), false, false, false, false, false, false, false);

        let ctx = calculate_effective_context(&alice, &[g1], &map(&[]), &[]);
        assert!(!ctx.is_admin);
        assert_eq!(ctx.tier, TIER_USER);
    }

    #[test]
    fn group_ids_collected() {
        let alice = user(uuid(1), None);
        let g1 = group(uuid(10), None, Some(2), false, false, false, false, false, false, false);
        let g2 = group(uuid(11), None, Some(2), false, false, false, false, false, false, false);

        let ctx = calculate_effective_context(&alice, &[g1, g2], &map(&[]), &[]);
        assert_eq!(ctx.group_ids, vec![uuid(10), uuid(11)]);
    }

    #[test]
    fn duplicate_whitelist_ids_deduplicated() {
        let t1 = uuid(100);
        let alice = user(uuid(1), None);
        let g1 = group(uuid(10), None, Some(2), false, false, false, false, false, false, false);

        let ctx = calculate_effective_context(
            &alice,
            &[g1],
            &map(&[(uuid(10), &[t1, t1])]),
            &[],
        );

        assert_eq!(ctx.allowed_template_ids, vec![t1]);
    }

    #[test]
    fn hidden_templates_excluded_from_whitelist() {
        // A group whitelist that names a hidden template must not surface it in
        // `allowed_template_ids` — hidden is an absolute off-switch (spec
        // Decision 3), so the launch list never advertises it.
        let t1 = uuid(100);
        let t2 = uuid(101);
        let t3 = uuid(102);
        let alice = user(uuid(1), None);
        let g1 = group(uuid(10), None, Some(2), false, false, false, false, false, false, false);
        let g2 = group(uuid(11), None, Some(2), false, false, false, false, false, false, false);

        let ctx = calculate_effective_context(
            &alice,
            &[g1, g2],
            &map(&[(uuid(10), &[t2, t3]), (uuid(11), &[t1])]),
            &[t2],
        );

        assert_eq!(ctx.allowed_template_ids, vec![t3, t1]);
    }

    #[test]
    fn hidden_exclusion_is_an_absolute_off_switch() {
        // Even a whitelist naming *only* a hidden template yields an empty
        // launch list, admins included.
        let t1 = uuid(100);
        let alice = user(uuid(1), None);
        let g1 = group(uuid(10), Some("admin"), None, true, true, true, true, true, true, false);

        let ctx = calculate_effective_context(
            &alice,
            &[g1],
            &map(&[(uuid(10), &[t1])]),
            &[t1],
        );

        assert!(ctx.is_admin);
        assert!(
            ctx.allowed_template_ids.is_empty(),
            "hidden template must not appear even for an admin"
        );
    }

    #[test]
    fn can_assign_groups_requires_strictly_lower_tier() {
        // A Manager (1) can assign to user/custom (0) but never manager/admin.
        assert!(can_assign_groups(TIER_MANAGER, &[None, Some("user".into())]));
        assert!(!can_assign_groups(TIER_MANAGER, &[Some("manager".into())]));
        assert!(!can_assign_groups(TIER_MANAGER, &[Some("admin".into())]));
        // A tier-0 actor can assign nobody anywhere.
        assert!(!can_assign_groups(TIER_USER, &[Some("user".into())]));
        assert!(!can_assign_groups(TIER_USER, &[None]));
        // An admin (2) can assign to manager/user/custom, but never into the
        // Admin group itself (tier >= actor is off-limits even for admins).
        assert!(can_assign_groups(TIER_ADMIN, &[Some("manager".into()), Some("user".into()), None]));
        assert!(!can_assign_groups(TIER_ADMIN, &[Some("admin".into())]));
    }

    // ── pre_flight ─────────────────────────────────────────────

    fn allow_all() -> EffectiveContext {
        calculate_effective_context(
            &user(uuid(1), Some(4)),
            &[group(uuid(10), None, Some(4), false, false, false, false, false, false, false)],
            &map(&[(uuid(10), &[uuid(200)])]),
            &[],
        )
    }

    fn res(cpu: i64, mem: i64, gpu: i64) -> ResourceUse {
        ResourceUse {
            cpu_cores: cpu,
            memory_mb: mem,
            gpu_count: gpu,
        }
    }

    /// An unlimited quota context: host caps disabled and a self-billed target
    /// with an unlimited pool — never rejects on resources. `-1` = unlimited.
    fn quota() -> QuotaContext {
        QuotaContext {
            pool: res(-1, -1, -1),
            billed: res(0, 0, 0),
            member_quota: res(-1, -1, -1),
            member_used: res(0, 0, 0),
            host_capacity: res(-1, -1, -1),
            host_used: res(0, 0, 0),
            target_group_id: None,
        }
    }

    #[test]
    fn memory_bytes_to_mb_rounds_up_and_preserves_unlimited() {
        // 4 GiB exactly → 4096 MB.
        assert_eq!(memory_bytes_to_mb(4_294_967_296), 4096);
        // 1 byte over a GiB boundary rounds up so a request is never
        // under-counted.
        assert_eq!(memory_bytes_to_mb(4_294_967_297), 4097);
        // Zero stays zero.
        assert_eq!(memory_bytes_to_mb(0), 0);
        // `-1` (unlimited) is preserved verbatim.
        assert_eq!(memory_bytes_to_mb(-1), -1);
    }

    #[test]
    fn pre_flight_allows_whitelisted_template_under_limits() {
        let ctx = allow_all();
        assert!(pre_flight(&ctx, 0, -1, uuid(200), 2, TemplateVisibility::Private, &res(0, 0, 0), &quota()).is_ok());
    }

    #[test]
    fn pre_flight_rejects_unlisted_template_for_everyone() {
        let ctx = allow_all();
        assert_eq!(
            pre_flight(&ctx, 0, -1, uuid(999), 0, TemplateVisibility::Private, &res(0, 0, 0), &quota()),
            Err(PreflightReject::TemplateNotAllowed {
                requested_template_id: uuid(999)
            })
        );
    }

    #[test]
    fn pre_flight_rejects_admin_on_unlisted_template() {
        // Admins are not exempt from the whitelist: an admin with no whitelist
        // is denied every private template.
        let admin = calculate_effective_context(
            &user(uuid(1), None),
            &[group(uuid(10), Some("admin"), None, true, true, true, true, true, true, false)],
            &map(&[]),
            &[],
        );
        assert_eq!(
            pre_flight(&admin, 0, -1, uuid(500), 0, TemplateVisibility::Private, &res(0, 0, 0), &quota()),
            Err(PreflightReject::TemplateNotAllowed {
                requested_template_id: uuid(500)
            })
        );
        assert!(admin.is_admin);
    }

    #[test]
    fn pre_flight_rejects_over_ceiling() {
        let ctx = allow_all();
        assert_eq!(
            pre_flight(&ctx, 0, -1, uuid(200), 4, TemplateVisibility::Private, &res(0, 0, 0), &quota()),
            Err(PreflightReject::InstanceCeilingExceeded { current: 4, limit: 4 })
        );
    }

    #[test]
    fn pre_flight_minus_one_ceiling_means_unlimited() {
        let ctx = calculate_effective_context(
            &user(uuid(1), Some(-1)),
            &[group(uuid(10), None, Some(-1), false, false, false, false, false, false, false)],
            &map(&[(uuid(10), &[uuid(200)])]),
            &[],
        );
        assert!(pre_flight(&ctx, 0, -1, uuid(200), 999, TemplateVisibility::Private, &res(0, 0, 0), &quota()).is_ok());
    }

    #[test]
    fn pre_flight_zero_ceiling_blocks() {
        // `0` is a real zero under the new convention: a blocked ceiling.
        let ctx = calculate_effective_context(
            &user(uuid(1), Some(0)),
            &[group(uuid(10), None, Some(0), false, false, false, false, false, false, false)],
            &map(&[(uuid(10), &[uuid(200)])]),
            &[],
        );
        assert_eq!(
            pre_flight(&ctx, 0, -1, uuid(200), 0, TemplateVisibility::Private, &res(0, 0, 0), &quota()),
            Err(PreflightReject::InstanceCeilingExceeded { current: 0, limit: 0 })
        );
    }

    #[test]
    fn pre_flight_rejects_host_ceiling_for_every_tier() {
        let ctx = allow_all();
        assert_eq!(
            pre_flight(&ctx, 9, 9, uuid(200), 0, TemplateVisibility::Private, &res(0, 0, 0), &quota()),
            Err(PreflightReject::HostCeilingExceeded { current: 9, limit: 9 })
        );
    }

    #[test]
    fn pre_flight_host_ceiling_minus_one_is_disabled() {
        let ctx = allow_all();
        assert!(pre_flight(&ctx, 500, -1, uuid(200), 0, TemplateVisibility::Private, &res(0, 0, 0), &quota()).is_ok());
    }

    #[test]
    fn pre_flight_host_ceiling_zero_blocks() {
        let ctx = allow_all();
        assert_eq!(
            pre_flight(&ctx, 0, 0, uuid(200), 0, TemplateVisibility::Private, &res(0, 0, 0), &quota()),
            Err(PreflightReject::HostCeilingExceeded { current: 0, limit: 0 })
        );
    }

    #[test]
    fn pre_flight_public_launches_without_whitelist() {
        // Public overrides the group whitelist: an empty whitelist still
        // launches (spec Decision 3).
        let ctx = calculate_effective_context(
            &user(uuid(1), Some(4)),
            &[group(uuid(10), None, Some(4), false, false, false, false, false, false, false)],
            &map(&[]),
            &[],
        );
        assert!(ctx.allowed_template_ids.is_empty());
        assert!(pre_flight(&ctx, 0, -1, uuid(200), 0, TemplateVisibility::Public, &res(0, 0, 0), &quota()).is_ok());
    }

    #[test]
    fn pre_flight_public_still_respects_ceiling() {
        // Public grants permission, not quota: ceilings still apply.
        let ctx = allow_all();
        assert_eq!(
            pre_flight(&ctx, 0, -1, uuid(200), 4, TemplateVisibility::Public, &res(0, 0, 0), &quota()),
            Err(PreflightReject::InstanceCeilingExceeded { current: 4, limit: 4 })
        );
        assert_eq!(
            pre_flight(&ctx, 9, 9, uuid(200), 0, TemplateVisibility::Public, &res(0, 0, 0), &quota()),
            Err(PreflightReject::HostCeilingExceeded { current: 9, limit: 9 })
        );
    }

    #[test]
    fn pre_flight_hidden_rejects_even_whitelisted() {
        // Hidden is an absolute off-switch: the whitelist is never consulted.
        let ctx = allow_all();
        assert_eq!(
            pre_flight(&ctx, 0, -1, uuid(200), 0, TemplateVisibility::Hidden, &res(0, 0, 0), &quota()),
            Err(PreflightReject::TemplateHidden {
                requested_template_id: uuid(200)
            })
        );
    }

    #[test]
    fn pre_flight_hidden_rejects_admin() {
        // No bypass: an admin launching a hidden template is rejected too.
        let admin = calculate_effective_context(
            &user(uuid(1), None),
            &[group(uuid(10), Some("admin"), None, true, true, true, true, true, true, false)],
            &map(&[(uuid(10), &[uuid(500)])]),
            &[],
        );
        assert!(admin.is_admin);
        assert_eq!(
            pre_flight(&admin, 0, -1, uuid(500), 0, TemplateVisibility::Hidden, &res(0, 0, 0), &quota()),
            Err(PreflightReject::TemplateHidden {
                requested_template_id: uuid(500)
            })
        );
    }

    // ── resource quota pre-flight ─────────────────────────────

    #[test]
    fn pre_flight_rejects_host_resource_cap() {
        let ctx = allow_all();
        let billing = QuotaContext {
            host_capacity: res(8, 0, 0),
            host_used: res(7, 0, 0),
            ..quota()
        };
        assert_eq!(
            pre_flight(&ctx, 0, -1, uuid(200), 0, TemplateVisibility::Private, &res(2, 0, 0), &billing),
            Err(PreflightReject::HostResourceExceeded {
                resource: ResourceKind::Cpu,
                current: 7,
                limit: 8,
                requested: 2,
            })
        );
    }

    #[test]
    fn pre_flight_host_resource_cap_disabled_at_minus_one() {
        let ctx = allow_all();
        let billing = QuotaContext {
            host_capacity: res(-1, -1, -1),
            host_used: res(900, 0, 0),
            ..quota()
        };
        assert!(pre_flight(&ctx, 0, -1, uuid(200), 0, TemplateVisibility::Private, &res(2, 0, 0), &billing).is_ok());
    }

    #[test]
    fn pre_flight_host_resource_cap_zero_blocks() {
        // `0` is a real zero: a blocked host cap rejects every finite request.
        let ctx = allow_all();
        let billing = QuotaContext {
            host_capacity: res(0, 0, 0),
            host_used: res(0, 0, 0),
            ..quota()
        };
        assert_eq!(
            pre_flight(&ctx, 0, -1, uuid(200), 0, TemplateVisibility::Private, &res(2, 0, 0), &billing),
            Err(PreflightReject::HostResourceExceeded {
                resource: ResourceKind::Cpu,
                current: 0,
                limit: 0,
                requested: 2,
            })
        );
    }

    #[test]
    fn pre_flight_rejects_pool_resource_cap() {
        let ctx = allow_all();
        let billing = QuotaContext {
            pool: res(4, 4096, 2),
            billed: res(3, 2048, 1),
            target_group_id: Some(uuid(10)),
            ..quota()
        };
        assert_eq!(
            pre_flight(&ctx, 0, -1, uuid(200), 0, TemplateVisibility::Private, &res(2, 2048, 1), &billing),
            Err(PreflightReject::PoolResourceExceeded {
                resource: ResourceKind::Cpu,
                current: 3,
                limit: 4,
                requested: 2,
                group_id: Some(uuid(10)),
            })
        );
        // Memory is the binding resource here: 2048 billed + 2049 requested > 4096 cap.
        assert_eq!(
            pre_flight(&ctx, 0, -1, uuid(200), 0, TemplateVisibility::Private, &res(0, 2049, 0), &billing),
            Err(PreflightReject::PoolResourceExceeded {
                resource: ResourceKind::Memory,
                current: 2048,
                limit: 4096,
                requested: 2049,
                group_id: Some(uuid(10)),
            })
        );
    }

    #[test]
    fn pre_flight_pool_resource_exact_fit_allowed() {
        let ctx = allow_all();
        let billing = QuotaContext {
            pool: res(4, 4096, 2),
            billed: res(3, 2048, 1),
            target_group_id: Some(uuid(10)),
            ..quota()
        };
        assert!(pre_flight(&ctx, 0, -1, uuid(200), 0, TemplateVisibility::Private, &res(1, 2048, 1), &billing).is_ok());
    }

    // ── per-member resource cap (spec Decision 5: the personal layer) ────────

    #[test]
    fn pre_flight_member_cap_rejects_when_over() {
        let ctx = allow_all();
        let billing = QuotaContext {
            member_quota: res(4, 4096, 2),
            member_used: res(3, 2048, 1),
            target_group_id: Some(uuid(10)),
            ..quota()
        };
        assert_eq!(
            pre_flight(&ctx, 0, -1, uuid(200), 0, TemplateVisibility::Private, &res(2, 0, 0), &billing),
            Err(PreflightReject::MemberResourceExceeded {
                resource: ResourceKind::Cpu,
                current: 3,
                limit: 4,
                requested: 2,
                group_id: Some(uuid(10)),
            })
        );
        // Memory is the binding resource here: 2048 used + 2049 requested > 4096 cap.
        assert_eq!(
            pre_flight(&ctx, 0, -1, uuid(200), 0, TemplateVisibility::Private, &res(0, 2049, 0), &billing),
            Err(PreflightReject::MemberResourceExceeded {
                resource: ResourceKind::Memory,
                current: 2048,
                limit: 4096,
                requested: 2049,
                group_id: Some(uuid(10)),
            })
        );
    }

    #[test]
    fn pre_flight_member_cap_binds_before_pool() {
        // The member cap is checked before the pool (spec Decision 5): a small
        // personal cap rejects even when the group pool would fit the request.
        let ctx = allow_all();
        let billing = QuotaContext {
            member_quota: res(2, 1024, 1),
            member_used: res(1, 512, 0),
            pool: res(16, 16384, 8),
            billed: res(0, 0, 0),
            target_group_id: Some(uuid(10)),
            ..quota()
        };
        assert_eq!(
            pre_flight(&ctx, 0, -1, uuid(200), 0, TemplateVisibility::Private, &res(2, 512, 1), &billing),
            Err(PreflightReject::MemberResourceExceeded {
                resource: ResourceKind::Cpu,
                current: 1,
                limit: 2,
                requested: 2,
                group_id: Some(uuid(10)),
            })
        );
    }

    #[test]
    fn pre_flight_member_cap_exact_fit_allowed() {
        let ctx = allow_all();
        let billing = QuotaContext {
            member_quota: res(4, 4096, 2),
            member_used: res(3, 2048, 1),
            target_group_id: Some(uuid(10)),
            ..quota()
        };
        assert!(pre_flight(&ctx, 0, -1, uuid(200), 0, TemplateVisibility::Private, &res(1, 2048, 1), &billing).is_ok());
    }

    #[test]
    fn pre_flight_member_cap_unlimited_allows() {
        let ctx = allow_all();
        let billing = QuotaContext {
            member_quota: res(-1, -1, -1),
            member_used: res(999, 999, 999),
            target_group_id: Some(uuid(10)),
            ..quota()
        };
        assert!(pre_flight(&ctx, 0, -1, uuid(200), 0, TemplateVisibility::Private, &res(500, 500, 500), &billing).is_ok());
    }

    #[test]
    fn pre_flight_member_cap_zero_blocks() {
        // `0` is a real zero: a blocked personal cap rejects every finite request.
        let ctx = allow_all();
        let billing = QuotaContext {
            member_quota: res(0, 0, 0),
            member_used: res(0, 0, 0),
            target_group_id: Some(uuid(10)),
            ..quota()
        };
        assert_eq!(
            pre_flight(&ctx, 0, -1, uuid(200), 0, TemplateVisibility::Private, &res(1, 0, 0), &billing),
            Err(PreflightReject::MemberResourceExceeded {
                resource: ResourceKind::Cpu,
                current: 0,
                limit: 0,
                requested: 1,
                group_id: Some(uuid(10)),
            })
        );
    }

    #[test]
    fn pre_flight_unlimited_request_rejected_by_finite_member() {
        // An unlimited (-1) request is refused at the member layer before the
        // pool is consulted.
        let ctx = allow_all();
        let billing = QuotaContext {
            member_quota: res(4, 0, 0),
            member_used: res(0, 0, 0),
            pool: res(-1, -1, -1),
            target_group_id: Some(uuid(10)),
            ..quota()
        };
        assert_eq!(
            pre_flight(&ctx, 0, -1, uuid(200), 0, TemplateVisibility::Private, &res(-1, 0, 0), &billing),
            Err(PreflightReject::MemberResourceExceeded {
                resource: ResourceKind::Cpu,
                current: 0,
                limit: 4,
                requested: -1,
                group_id: Some(uuid(10)),
            })
        );
    }

    #[test]
    fn pre_flight_unlimited_pool_never_rejects() {
        let ctx = allow_all();
        let billing = QuotaContext {
            pool: res(-1, -1, -1),
            billed: res(999, 999, 999),
            target_group_id: Some(uuid(10)),
            ..quota()
        };
        assert!(pre_flight(&ctx, 0, -1, uuid(200), 0, TemplateVisibility::Private, &res(500, 500, 500), &billing).is_ok());
    }

    // ── `-1` request rule (spec Decision: an unlimited instance request) ──────

    #[test]
    fn pre_flight_unlimited_request_allowed_when_every_layer_unlimited() {
        let ctx = allow_all();
        let billing = QuotaContext {
            pool: res(-1, -1, -1),
            target_group_id: Some(uuid(10)),
            ..quota()
        };
        assert!(pre_flight(&ctx, 0, -1, uuid(200), 0, TemplateVisibility::Private, &res(-1, -1, -1), &billing).is_ok());
    }

    #[test]
    fn pre_flight_unlimited_request_rejected_by_finite_pool() {
        let ctx = allow_all();
        let billing = QuotaContext {
            pool: res(4, 0, 0),
            billed: res(0, 0, 0),
            target_group_id: Some(uuid(10)),
            ..quota()
        };
        assert_eq!(
            pre_flight(&ctx, 0, -1, uuid(200), 0, TemplateVisibility::Private, &res(-1, 0, 0), &billing),
            Err(PreflightReject::PoolResourceExceeded {
                resource: ResourceKind::Cpu,
                current: 0,
                limit: 4,
                requested: -1,
                group_id: Some(uuid(10)),
            })
        );
    }

    #[test]
    fn pre_flight_unlimited_request_rejected_by_finite_host() {
        let ctx = allow_all();
        let billing = QuotaContext {
            host_capacity: res(8, 0, 0),
            host_used: res(0, 0, 0),
            ..quota()
        };
        assert_eq!(
            pre_flight(&ctx, 0, -1, uuid(200), 0, TemplateVisibility::Private, &res(-1, 0, 0), &billing),
            Err(PreflightReject::HostResourceExceeded {
                resource: ResourceKind::Cpu,
                current: 0,
                limit: 8,
                requested: -1,
            })
        );
    }

    #[test]
    fn pre_flight_zero_request_is_zero_cost() {
        // `0` is a real zero under the new convention: a finite zero-cost
        // request. It consumes nothing and is always fine — even under finite
        // limits and into a blocked (`0`) pool.
        let ctx = allow_all();
        let billing = QuotaContext {
            pool: res(0, 0, 0),
            billed: res(0, 0, 0),
            target_group_id: Some(uuid(10)),
            ..quota()
        };
        assert!(pre_flight(&ctx, 0, -1, uuid(200), 0, TemplateVisibility::Private, &res(0, 0, 0), &billing).is_ok());
        let finite = QuotaContext {
            pool: res(2, 2048, 1),
            billed: res(2, 2048, 1),
            target_group_id: Some(uuid(10)),
            ..quota()
        };
        assert!(pre_flight(&ctx, 0, -1, uuid(200), 0, TemplateVisibility::Private, &res(0, 0, 0), &finite).is_ok());
    }

    #[test]
    fn pre_flight_public_still_respects_resource_caps() {
        let ctx = allow_all();
        let billing = QuotaContext {
            host_capacity: res(4, 0, 0),
            host_used: res(4, 0, 0),
            ..quota()
        };
        assert_eq!(
            pre_flight(&ctx, 0, -1, uuid(200), 0, TemplateVisibility::Public, &res(1, 0, 0), &billing),
            Err(PreflightReject::HostResourceExceeded {
                resource: ResourceKind::Cpu,
                current: 4,
                limit: 4,
                requested: 1,
            })
        );
    }

    // ── aggregate resource quotas on the context ──────────────

    #[test]
    fn resource_quotas_is_max_finite_pool_across_groups() {
        let alice = user(uuid(1), None);
        let g1 = group(uuid(10), None, Some(2), false, false, false, false, false, false, false);
        let g2 = group(uuid(11), None, Some(2), false, false, false, false, false, false, false);
        let mut g1p = g1.clone();
        g1p.pool_cpu_cores = 4;
        g1p.pool_memory_mb = 2048;
        let mut g2p = g2.clone();
        g2p.pool_cpu_cores = 8;
        g2p.pool_memory_mb = 4096;

        let ctx = calculate_effective_context(&alice, &[g1p, g2p], &map(&[]), &[]);
        assert_eq!(ctx.resource_quotas.cpu_cores, 8);
        assert_eq!(ctx.resource_quotas.memory_mb, 4096);
        assert_eq!(ctx.resource_quotas.gpu_count, 0, "no group sets a gpu pool");
    }

    #[test]
    fn resource_quotas_unlimited_group_wins() {
        let alice = user(uuid(1), None);
        let mut g1 = group(uuid(10), None, Some(2), false, false, false, false, false, false, false);
        g1.pool_cpu_cores = 4;
        let mut g2 = group(uuid(11), None, Some(2), false, false, false, false, false, false, false);
        g2.pool_cpu_cores = -1; // unlimited

        let ctx = calculate_effective_context(&alice, &[g1, g2], &map(&[]), &[]);
        assert_eq!(ctx.resource_quotas.cpu_cores, -1, "unlimited pool dominates");
    }

    #[test]
    fn group_billing_surfaces_each_group_pool() {
        let alice = user(uuid(1), None);
        let mut g1 = group(uuid(10), Some("manager"), Some(2), false, false, false, false, false, false, false);
        g1.name = "ML Team".to_string();
        g1.billing_model = "dedicated".to_string();
        g1.pool_gpu_count = 1;
        g1.member_cpu_cores = 4;
        g1.member_memory_mb = 8192;
        g1.member_gpu_count = 1;

        let ctx = calculate_effective_context(&alice, &[g1], &map(&[]), &[]);
        assert_eq!(ctx.group_billing.len(), 1);
        assert_eq!(ctx.group_billing[0].group_id, uuid(10));
        assert_eq!(ctx.group_billing[0].group_name, "ML Team");
        assert_eq!(ctx.group_billing[0].tier, TIER_MANAGER);
        assert_eq!(ctx.group_billing[0].billing_model, "dedicated");
        assert_eq!(ctx.group_billing[0].pool_gpu_count, 1);
        assert_eq!(ctx.group_billing[0].member_cpu_cores, 4);
        assert_eq!(ctx.group_billing[0].member_memory_mb, 8192);
        assert_eq!(ctx.group_billing[0].member_gpu_count, 1);
    }
}
