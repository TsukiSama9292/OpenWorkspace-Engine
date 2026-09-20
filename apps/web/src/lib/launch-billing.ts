import type { GroupBilling, Instance } from '$lib/types';

/**
 * Compare two of the user's memberships by the personal cap they offer. A `-1`
 * (unlimited) member cap ranks above any finite cap; `0` (blocked) is the
 * lowest. Ties break by tier (higher first), then by group name (ascending)
 * for a deterministic picker default.
 */
export function compareBillingGroups(a: GroupBilling, b: GroupBilling): number {
  const caps: Array<[number, number]> = [
    [a.member_cpu_cores, b.member_cpu_cores],
    [a.member_memory_mb, b.member_memory_mb],
    [a.member_gpu_count, b.member_gpu_count],
  ];
  for (const [av, bv] of caps) {
    const diff = capCompare(av, bv);
    if (diff !== 0) return diff;
  }
  if (a.tier !== b.tier) return b.tier - a.tier;
  return a.group_name.localeCompare(b.group_name);
}

function capCompare(a: number, b: number): number {
  const ar = capRank(a);
  const br = capRank(b);
  if (ar === br) return 0;
  return ar > br ? -1 : 1;
}

function capRank(v: number): number {
  return v < 0 ? Number.POSITIVE_INFINITY : v;
}

/** The billing group id the launch form defaults to: the highest-cap membership. */
export function defaultBillingGroup(groups: GroupBilling[]): string {
  if (groups.length <= 1) return groups[0]?.group_id ?? '';
  return [...groups].sort(compareBillingGroups)[0].group_id;
}

/**
 * The display name of the group an instance bills to. Prefers the frozen
 * launch snapshot (which carries the group name when present), falls back to
 * the live membership list, and finally to the short group id.
 */
export function billingGroupName(inst: Instance, groups: GroupBilling[]): string | null {
  const snapshot = inst.billing_group_snapshot;
  if (snapshot?.group_name) return snapshot.group_name;
  const groupId = inst.owner_group_id ?? snapshot?.group_id ?? null;
  if (!groupId) return null;
  const live = groups.find((g) => g.group_id === groupId);
  return live?.group_name ?? groupId.slice(0, 8);
}

/** A compact resource-usage label, e.g. "4 CPU · 16 GB · 1 GPU". Null when the instance lacks the snapshot columns. */
export function instanceResourceLabel(inst: Instance): string | null {
  if (inst.host_cpu_cores == null || inst.host_memory_mb == null || inst.host_gpu_count == null) {
    return null;
  }
  const mem =
    inst.host_memory_mb >= 1024
      ? `${Math.round(inst.host_memory_mb / 1024)} GB`
      : `${inst.host_memory_mb} MB`;
  return `${inst.host_cpu_cores} CPU · ${mem} · ${inst.host_gpu_count} GPU`;
}

/** Describes a billing option in the picker, e.g. "Devs — CPU unlimited · Mem 16 GB · GPU blocked". */
export function describeBillingOption(g: GroupBilling): string {
  const parts = [
    `CPU ${describeCap(g.member_cpu_cores)}`,
    `Mem ${describeMemCap(g.member_memory_mb)}`,
    `GPU ${describeCap(g.member_gpu_count)}`,
  ];
  return `${g.group_name} — ${parts.join(' · ')}`;
}

function describeCap(v: number): string {
  if (v < 0) return 'unlimited';
  if (v === 0) return 'blocked';
  return String(v);
}

function describeMemCap(mb: number): string {
  if (mb < 0) return 'unlimited';
  if (mb === 0) return 'blocked';
  return `${Math.round(mb / 1024)} GB`;
}
