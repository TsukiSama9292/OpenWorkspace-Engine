import { updateUserPolicy } from '$lib/api/rbac-actions';
import type { UserPolicyUpdate } from '$lib/types';

export interface UserPolicyFormState {
  group_ids: string[];
  direct_max_instances: string;
  loading: boolean;
  error: string;
}

export function createInitialUserPolicyForm(): UserPolicyFormState {
  return {
    group_ids: [],
    direct_max_instances: '',
    loading: false,
    error: ''
  };
}

export function userPolicyFormFromRow(row: {
  group_ids?: string[];
  direct_max_instances?: number | null;
}): UserPolicyFormState {
  return {
    group_ids: row.group_ids ? [...row.group_ids] : [],
    direct_max_instances: row.direct_max_instances == null ? '' : String(row.direct_max_instances),
    loading: false,
    error: ''
  };
}

export function buildUserPolicyUpdate(state: UserPolicyFormState): UserPolicyUpdate {
  return {
    group_ids: [...state.group_ids],
    direct_max_instances: state.direct_max_instances === '' ? null : Number(state.direct_max_instances)
  };
}

export async function submitUserPolicy(
  userId: string,
  state: UserPolicyFormState,
  opts?: { omitGroupIds?: boolean }
): Promise<{ error?: string }> {
  const ceiling = state.direct_max_instances === '' ? null : Number(state.direct_max_instances);
  if (ceiling !== null && (Number.isNaN(ceiling) || ceiling < 0)) {
    return { error: 'Personal instance ceiling must be >= 0 (blank = inherit)' };
  }

  const res = await updateUserPolicy(userId, {
    ...(opts?.omitGroupIds ? {} : { group_ids: [...state.group_ids] }),
    direct_max_instances: ceiling
  });
  if (res.error) return { error: res.error };
  return {};
}
