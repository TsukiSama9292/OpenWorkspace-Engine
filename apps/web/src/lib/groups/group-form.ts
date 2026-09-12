import { createGroup, updateGroup } from '$lib/api/rbac-actions';
import { DISABLED, UNLIMITED, isTriStateValid, memoryMbFromTriState, memoryMbToTriState, triStateFromValue, valueFromTriState, type TriState } from '$lib/tri-state';
import type { Group, GroupInput } from '$lib/types';

export type BillingModel = 'shared' | 'dedicated';

export const GROUP_FLAGS = [
  'can_create_template',
  'can_manage_users',
  'can_manage_group_instances',
  'can_manage_docker',
  'can_manage_registry',
  'can_view_monitoring',
  'can_view_audit_logs'
] as const;

export type GroupFlag = (typeof GROUP_FLAGS)[number];

export interface GroupFormState {
  name: string;
  description: string;
  kind: Group['kind'];
  can_create_template: boolean;
  can_manage_users: boolean;
  can_manage_group_instances: boolean;
  can_manage_docker: boolean;
  can_manage_registry: boolean;
  can_view_monitoring: boolean;
  can_view_audit_logs: boolean;
  max_instances: TriState;
  billing_model: BillingModel;
  poolCpu: TriState;
  poolMemory: TriState;
  poolGpu: TriState;
  template_ids: string[];
  loading: boolean;
  error: string;
}

export function isSystemGroup(group: Group): boolean {
  return group.kind !== null;
}

export function describePoolValue(v: number | undefined): string {
  const n = v ?? -1;
  if (n < 0) return 'unlimited';
  if (n === 0) return 'disabled';
  return String(n);
}

export function describeGroupPool(group: Pick<Group, 'billing_model' | 'pool_cpu_cores' | 'pool_memory_mb' | 'pool_gpu_count'>): string {
  const memMb = group.pool_memory_mb;
  const mem = memMb == null || memMb < 0 || memMb === 0 ? memMb : Math.round(memMb / 1024);
  return `${group.billing_model ?? 'shared'} · CPU ${describePoolValue(group.pool_cpu_cores)} · Mem ${describePoolValue(mem)} GB · GPU ${describePoolValue(group.pool_gpu_count)}`;
}

export function createInitialGroupForm(): GroupFormState {
  return {
    name: '',
    description: '',
    kind: null,
    can_create_template: false,
    can_manage_users: false,
    can_manage_group_instances: false,
    can_manage_docker: false,
    can_manage_registry: false,
    can_view_monitoring: false,
    can_view_audit_logs: false,
    max_instances: { ...UNLIMITED },
    billing_model: 'shared',
    // New groups default to blocked pools (spec Story 16): an admin opens
    // the pool explicitly instead of launching into an ungoverned one.
    poolCpu: { ...DISABLED },
    poolMemory: { ...DISABLED },
    poolGpu: { ...DISABLED },
    template_ids: [],
    loading: false,
    error: ''
  };
}

export function groupFormFromGroup(group: Group): GroupFormState {
  return {
    name: group.name,
    description: group.description ?? '',
    kind: group.kind,
    can_create_template: group.can_create_template,
    can_manage_users: group.can_manage_users,
    can_manage_group_instances: group.can_manage_group_instances,
    can_manage_docker: group.can_manage_docker,
    can_manage_registry: group.can_manage_registry,
    can_view_monitoring: group.can_view_monitoring,
    can_view_audit_logs: group.can_view_audit_logs,
    max_instances: triStateFromValue(group.max_instances, -1),
    billing_model: group.billing_model ?? 'shared',
    poolCpu: triStateFromValue(group.pool_cpu_cores),
    poolMemory: memoryMbToTriState(group.pool_memory_mb),
    poolGpu: triStateFromValue(group.pool_gpu_count),
    template_ids: [...group.template_ids],
    loading: false,
    error: ''
  };
}

export function buildGroupInput(state: GroupFormState): GroupInput {
  const systemFlags = systemGroupFlags(state.kind);
  return {
    name: state.name.trim(),
    description: state.description.trim() || null,
    can_create_template: systemFlags.can_create_template ?? state.can_create_template,
    can_manage_users: systemFlags.can_manage_users ?? state.can_manage_users,
    can_manage_group_instances: systemFlags.can_manage_group_instances ?? state.can_manage_group_instances,
    can_manage_docker: systemFlags.can_manage_docker ?? state.can_manage_docker,
    can_manage_registry: systemFlags.can_manage_registry ?? state.can_manage_registry,
    can_view_monitoring: systemFlags.can_view_monitoring ?? state.can_view_monitoring,
    can_view_audit_logs: systemFlags.can_view_audit_logs ?? state.can_view_audit_logs,
    max_instances: valueFromTriState(state.max_instances),
    billing_model: state.billing_model,
    pool_cpu_cores: valueFromTriState(state.poolCpu),
    pool_memory_mb: memoryMbFromTriState(state.poolMemory),
    pool_gpu_count: valueFromTriState(state.poolGpu),
    template_ids: [...state.template_ids]
  };
}

export function systemGroupFlags(kind: Group['kind']): {
  can_create_template?: boolean;
  can_manage_users?: boolean;
  can_manage_group_instances?: boolean;
  can_manage_docker?: boolean;
  can_manage_registry?: boolean;
  can_view_monitoring?: boolean;
  can_view_audit_logs?: boolean;
} {
  if (kind === 'admin') return { can_create_template: true, can_manage_users: true, can_manage_group_instances: true, can_manage_docker: true, can_manage_registry: true, can_view_monitoring: true, can_view_audit_logs: true };
  if (kind === 'user') return { can_create_template: false, can_manage_users: false, can_manage_group_instances: false, can_manage_docker: false, can_manage_registry: false, can_view_monitoring: false, can_view_audit_logs: false };
  return {};
}

function validate(state: GroupFormState): string | undefined {
  if (!state.name.trim()) return 'Name is required';
  if (!isTriStateValid(state.max_instances)) {
    return 'Max instances must be -1 (unlimited), 0 (disabled), or a positive number';
  }
  if (!isTriStateValid(state.poolCpu)) return 'Pool CPU must be -1 (unlimited), 0 (disabled), or a positive number';
  if (!isTriStateValid(state.poolMemory)) return 'Pool memory must be -1 (unlimited), 0 (disabled), or a positive number';
  if (!isTriStateValid(state.poolGpu)) return 'Pool GPU must be -1 (unlimited), 0 (disabled), or a positive number';
  return undefined;
}

export async function submitGroup(state: GroupFormState): Promise<{ id?: string; error?: string }> {
  const validationError = validate(state);
  if (validationError) return { error: validationError };

  const res = await createGroup(buildGroupInput(state));
  if (res.error) return { error: res.error };
  if (res.group) return { id: res.group.id };
  return { error: 'Failed to create group' };
}

export async function submitGroupUpdate(id: string, state: GroupFormState): Promise<{ error?: string }> {
  const validationError = validate(state);
  if (validationError) return { error: validationError };

  const res = await updateGroup(id, buildGroupInput(state));
  if (res.error) return { error: res.error };
  if (res.group) return {};
  return { error: 'Failed to update group' };
}
