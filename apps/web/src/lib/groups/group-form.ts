import { createGroup, updateGroup } from '$lib/api/rbac-actions';
import type { Group, GroupInput } from '$lib/types';

export const GROUP_FLAGS = [
  'can_create_template',
  'can_manage_users',
  'can_manage_group_instances',
  'can_manage_docker',
  'can_manage_registry',
  'can_view_monitoring'
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
  max_instances: string;
  template_ids: string[];
  loading: boolean;
  error: string;
}

export function isSystemGroup(group: Group): boolean {
  return group.kind !== null;
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
    max_instances: '2',
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
    max_instances: group.max_instances == null ? '' : String(group.max_instances),
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
    max_instances: Number(state.max_instances) || 0,
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
} {
  if (kind === 'admin') return { can_create_template: true, can_manage_users: true, can_manage_group_instances: true, can_manage_docker: true, can_manage_registry: true, can_view_monitoring: true };
  if (kind === 'user') return { can_create_template: false, can_manage_users: false, can_manage_group_instances: false, can_manage_docker: false, can_manage_registry: false, can_view_monitoring: false };
  return {};
}

function validate(state: GroupFormState): string | undefined {
  if (!state.name.trim()) return 'Name is required';
  if (Number.isNaN(Number(state.max_instances)) || Number(state.max_instances) < 0) {
    return 'Max instances must be >= 0 (0 = unlimited)';
  }
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
