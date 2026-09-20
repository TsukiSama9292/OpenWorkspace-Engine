import { updateUserPolicy } from '$lib/api/rbac-actions';
import type { UserPolicyUpdate } from '$lib/types';

export type CeilingMode = 'inherit' | 'unlimited' | 'disabled' | 'custom';

export interface UserCeiling {
  mode: CeilingMode;
  /** The custom cap; only meaningful when `mode === 'custom'`. */
  value: number;
}

export const INHERIT_CEILING: UserCeiling = { mode: 'inherit', value: 1 };
export const UNLIMITED_CEILING: UserCeiling = { mode: 'unlimited', value: 1 };
export const BLOCKED_CEILING: UserCeiling = { mode: 'disabled', value: 0 };

export interface UserPolicyFormState {
  group_ids: string[];
  ceiling: UserCeiling;
  loading: boolean;
  error: string;
}

export function createInitialUserPolicyForm(): UserPolicyFormState {
  return {
    group_ids: [],
    ceiling: { ...INHERIT_CEILING },
    loading: false,
    error: ''
  };
}

/** Maps the wire value (`NULL` = inherit, `-1` = unlimited, `0` = blocked) to the form. */
export function userPolicyFormFromRow(row: {
  group_ids?: string[];
  direct_max_instances?: number | null;
}): UserPolicyFormState {
  return {
    group_ids: row.group_ids ? [...row.group_ids] : [],
    ceiling: ceilingFromValue(row.direct_max_instances),
    loading: false,
    error: ''
  };
}

export function ceilingFromValue(value: number | null | undefined): UserCeiling {
  if (value == null) return { ...INHERIT_CEILING };
  if (value === -1) return { ...UNLIMITED_CEILING };
  if (value === 0) return { ...BLOCKED_CEILING };
  return { mode: 'custom', value };
}

export function valueFromCeiling(ceiling: UserCeiling): number | null {
  if (ceiling.mode === 'inherit') return null;
  if (ceiling.mode === 'unlimited') return -1;
  if (ceiling.mode === 'disabled') return 0;
  return ceiling.value;
}

export function isCeilingValid(ceiling: UserCeiling): boolean {
  return ceiling.mode !== 'custom' || ceiling.value > 0;
}

/** Human label for the table row; accepts the raw wire value directly. */
export function describeDirectMax(value: number | null | undefined): string {
  if (value == null) return 'inherit';
  if (value === -1) return 'unlimited';
  if (value === 0) return 'blocked';
  return String(value);
}

export function buildUserPolicyUpdate(state: UserPolicyFormState): UserPolicyUpdate {
  return {
    group_ids: [...state.group_ids],
    direct_max_instances: valueFromCeiling(state.ceiling)
  };
}

export async function submitUserPolicy(
  userId: string,
  state: UserPolicyFormState,
  opts?: { omitGroupIds?: boolean }
): Promise<{ error?: string }> {
  if (!isCeilingValid(state.ceiling)) {
    return {
      error:
        'Personal instance ceiling must be -1 (unlimited), 0 (blocked), a positive number, or inherit (blank)'
    };
  }

  const res = await updateUserPolicy(userId, {
    ...(opts?.omitGroupIds ? {} : { group_ids: [...state.group_ids] }),
    direct_max_instances: valueFromCeiling(state.ceiling)
  });
  if (res.error) return { error: res.error };
  return {};
}
