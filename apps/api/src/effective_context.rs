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
    /// `0` means "no ceiling" (matches the `host_instance_limit = 0`
    /// convention). Non-zero values are exact per-user limits. Resolved as the
    /// *maximum* of the personal ceiling and every group ceiling, where 0/NULL
    /// = unlimited is the highest (spec Decision 4).
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
    /// `admin` | `manager` | `user` | `None` (custom groups). Only the kind
    /// feeds tier derivation; names are cosmetic.
    pub kind: Option<String>,
    /// `None` (NULL) and `0` both mean "unlimited"; any positive value is a
    /// hard ceiling.
    pub max_instances: Option<i32>,
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
///    every group ceiling, where 0/NULL = unlimited is the highest.
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

    let mut unlimited = false;
    let mut max_finite = 0;
    if let Some(direct) = user.direct_max_instances {
        if direct == 0 {
            unlimited = true;
        } else {
            max_finite = max_finite.max(direct);
        }
    }
    for group in groups {
        match group.max_instances {
            None | Some(0) => unlimited = true,
            Some(ceiling) => max_finite = max_finite.max(ceiling),
        }
    }
    let effective_max_instances = if unlimited { 0 } else { max_finite };

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

/// Run the three launch pre-flight checks as a pure decision function, with the
/// template's visibility applied first (spec Decision 3):
///
/// 1. `hidden` → `403` before the whitelist is consulted (no bypass, admins
///    included); `public` → skip the whitelist check; `private` → whitelist.
/// 2. `requested_template_id` not in the whitelist → `403` (every tier —
///    admins are authorized group-only, spec Decision 4/5).
/// 3. `active_own_count + 1 > effective_max_instances` → `409` (unless the
///    ceiling is 0, meaning no limit). Public grants permission, not quota.
/// 4. `host_instance_limit > 0` and `host_active_count + 1 > host_instance_limit`
///    → `409`. No tier is exempt: admin instances still count toward the host
///    limit, so the global check always runs.
pub fn pre_flight(
    context: &EffectiveContext,
    host_active_count: i32,
    host_instance_limit: i32,
    requested_template_id: Uuid,
    active_own_count: i32,
    template_visibility: TemplateVisibility,
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

    if context.effective_max_instances != 0
        && active_own_count + 1 > context.effective_max_instances
    {
        return Err(PreflightReject::InstanceCeilingExceeded {
            current: active_own_count,
            limit: context.effective_max_instances,
        });
    }

    if host_instance_limit > 0 && host_active_count + 1 > host_instance_limit {
        return Err(PreflightReject::HostCeilingExceeded {
            current: host_active_count,
            limit: host_instance_limit,
        });
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
            kind: kind.map(|k| k.to_string()),
            max_instances,
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
    fn direct_zero_means_no_ceiling() {
        let alice = user(uuid(1), Some(0));
        let g1 = group(uuid(10), None, Some(2), false, false, false, false, false, false, false);

        let ctx = calculate_effective_context(&alice, &[g1], &map(&[]), &[]);
        assert_eq!(ctx.effective_max_instances, 0);
    }

    #[test]
    fn null_group_ceiling_means_no_ceiling() {
        // Admin group: NULL max_instances → unlimited, even next to a finite
        // personal ceiling.
        let admin = user(uuid(1), Some(3));
        let g1 = group(uuid(10), Some("admin"), None, true, true, true, true, true, true, false);

        let ctx = calculate_effective_context(&admin, &[g1], &map(&[]), &[]);
        assert_eq!(ctx.effective_max_instances, 0);
        assert!(ctx.is_admin);
        assert_eq!(ctx.tier, TIER_ADMIN);
    }

    #[test]
    fn no_groups_no_direct_is_unlimited_but_default_deny() {
        let alice = user(uuid(1), None);

        let ctx = calculate_effective_context(&alice, &[], &map(&[]), &[]);

        assert!(!ctx.is_admin);
        assert_eq!(ctx.tier, TIER_USER);
        assert_eq!(ctx.effective_max_instances, 0);
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

    #[test]
    fn pre_flight_allows_whitelisted_template_under_limits() {
        let ctx = allow_all();
        assert!(pre_flight(&ctx, 0, 0, uuid(200), 2, TemplateVisibility::Private).is_ok());
    }

    #[test]
    fn pre_flight_rejects_unlisted_template_for_everyone() {
        let ctx = allow_all();
        assert_eq!(
            pre_flight(&ctx, 0, 0, uuid(999), 0, TemplateVisibility::Private),
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
            pre_flight(&admin, 0, 0, uuid(500), 0, TemplateVisibility::Private),
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
            pre_flight(&ctx, 0, 0, uuid(200), 4, TemplateVisibility::Private),
            Err(PreflightReject::InstanceCeilingExceeded { current: 4, limit: 4 })
        );
    }

    #[test]
    fn pre_flight_zero_ceiling_means_unlimited() {
        let ctx = calculate_effective_context(
            &user(uuid(1), Some(0)),
            &[group(uuid(10), None, Some(0), false, false, false, false, false, false, false)],
            &map(&[(uuid(10), &[uuid(200)])]),
            &[],
        );
        assert!(pre_flight(&ctx, 0, 0, uuid(200), 999, TemplateVisibility::Private).is_ok());
    }

    #[test]
    fn pre_flight_rejects_host_ceiling_for_every_tier() {
        let ctx = allow_all();
        assert_eq!(
            pre_flight(&ctx, 9, 9, uuid(200), 0, TemplateVisibility::Private),
            Err(PreflightReject::HostCeilingExceeded { current: 9, limit: 9 })
        );
    }

    #[test]
    fn pre_flight_host_ceiling_zero_is_disabled() {
        let ctx = allow_all();
        assert!(pre_flight(&ctx, 500, 0, uuid(200), 0, TemplateVisibility::Private).is_ok());
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
        assert!(pre_flight(&ctx, 0, 0, uuid(200), 0, TemplateVisibility::Public).is_ok());
    }

    #[test]
    fn pre_flight_public_still_respects_ceiling() {
        // Public grants permission, not quota: ceilings still apply.
        let ctx = allow_all();
        assert_eq!(
            pre_flight(&ctx, 0, 0, uuid(200), 4, TemplateVisibility::Public),
            Err(PreflightReject::InstanceCeilingExceeded { current: 4, limit: 4 })
        );
        assert_eq!(
            pre_flight(&ctx, 9, 9, uuid(200), 0, TemplateVisibility::Public),
            Err(PreflightReject::HostCeilingExceeded { current: 9, limit: 9 })
        );
    }

    #[test]
    fn pre_flight_hidden_rejects_even_whitelisted() {
        // Hidden is an absolute off-switch: the whitelist is never consulted.
        let ctx = allow_all();
        assert_eq!(
            pre_flight(&ctx, 0, 0, uuid(200), 0, TemplateVisibility::Hidden),
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
            pre_flight(&admin, 0, 0, uuid(500), 0, TemplateVisibility::Hidden),
            Err(PreflightReject::TemplateHidden {
                requested_template_id: uuid(500)
            })
        );
    }
}
