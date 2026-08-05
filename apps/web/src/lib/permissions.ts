import { TIER_ADMIN, TIER_MANAGER, TIER_USER, type EffectiveContext, type Group, type Instance, type Template } from '$lib/types';

export type PermissionContext = EffectiveContext | null;

export interface UserScope {
  user_id: string;
  tier?: number;
}

export function userTier(user: { tier?: number }): number {
  return user.tier ?? TIER_USER;
}

export function groupTier(group: Pick<Group, 'kind'>): number {
  if (group.kind === 'admin') return TIER_ADMIN;
  if (group.kind === 'manager') return TIER_MANAGER;
  return TIER_USER;
}

function sharesGroup(ctx: EffectiveContext, instance: Instance): boolean {
  const ownerGroups = instance.owner_group_ids ?? [];
  return ctx.group_ids.some((gid) => ownerGroups.includes(gid));
}

export function mayControlInstance(ctx: PermissionContext, instance: Instance): boolean {
  if (!ctx) return false;
  if (ctx.is_admin) return true;
  if (instance.owner_id === ctx.user_id) return true;
  // Tier guardrail (spec Decision 6): group-scoped instance control extends
  // only to owners of a strictly lower tier, even when a group is shared.
  return (
    ctx.can_manage_group_instances &&
    sharesGroup(ctx, instance) &&
    (instance.owner_tier ?? TIER_USER) < ctx.tier
  );
}

export function mayLaunchTemplate(ctx: PermissionContext, template: Template): boolean {
  if (!ctx) return false;
  // Per-template visibility overrides the group whitelist (template-visibility
  // spec Decision 4): public launches for everyone, private keeps the
  // group-union whitelist. Hidden templates are excluded from
  // `allowed_template_ids` by the API, so the whitelist check alone rejects
  // them — no client-side special case.
  if (template.visibility === 'public') return true;
  return ctx.allowed_template_ids.includes(template.id);
}

export function mayManageUsers(ctx: PermissionContext): boolean {
  return ctx !== null && (ctx.is_admin || ctx.can_manage_users);
}

export function mayManageUser(ctx: PermissionContext, user: UserScope): boolean {
  if (!mayManageUsers(ctx)) return false;
  if (ctx!.is_admin) return true;
  if (user.user_id === ctx!.user_id) return true;
  return userTier(user) < ctx!.tier;
}

export function assignableGroups(ctx: PermissionContext, groups: Group[]): Group[] {
  if (!ctx) return [];
  return groups.filter((g) => groupTier(g) < ctx.tier);
}

export function mayCreateTemplate(ctx: PermissionContext): boolean {
  return ctx !== null && (ctx.is_admin || ctx.can_create_template);
}

export function mayEditTemplate(ctx: PermissionContext, template: Template): boolean {
  if (!ctx) return false;
  if (ctx.is_admin) return true;
  return ctx.can_create_template && template.owner_id === ctx.user_id;
}
