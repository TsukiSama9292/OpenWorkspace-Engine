import { api } from '$lib/api/client';
import type { EffectiveContext, Group, GroupInput, PersistentVolume, UserPolicyUpdate } from '$lib/types';

export async function fetchEffectiveContext(): Promise<{ context?: EffectiveContext; error?: string }> {
  const res = await api.get<{ context: EffectiveContext }>('/auth/me');
  if (res.error || !res.data) return { error: res.error ?? 'Failed to load effective context' };
  return { context: res.data.context };
}

export async function listGroups(): Promise<{ groups?: Group[]; error?: string }> {
  const res = await api.get<{ groups: Group[] }>('/groups');
  if (res.error || !res.data) return { error: res.error ?? 'Failed to load groups' };
  return { groups: res.data.groups };
}

export async function createGroup(input: GroupInput): Promise<{ group?: Group; error?: string }> {
  const res = await api.post<{ group: Group }>('/groups', input);
  if (res.error || !res.data) return { error: res.error ?? 'Failed to create group' };
  return { group: res.data.group };
}

export async function updateGroup(groupId: string, input: GroupInput): Promise<{ group?: Group; error?: string }> {
  const res = await api.put<{ group: Group }>(`/groups/${groupId}`, input);
  if (res.error || !res.data) return { error: res.error ?? 'Failed to update group' };
  return { group: res.data.group };
}

export async function deleteGroup(groupId: string): Promise<{ error?: string }> {
  const res = await api.delete(`/groups/${groupId}`);
  if (res.error) return { error: res.error };
  return {};
}

export async function updateUserPolicy(userId: string, update: UserPolicyUpdate): Promise<{ error?: string }> {
  const res = await api.put(`/users/${userId}`, update);
  if (res.error) return { error: res.error };
  return {};
}

export async function listOrphanedVolumes(): Promise<{ volumes?: PersistentVolume[]; error?: string }> {
  const res = await api.get<{ volumes: PersistentVolume[] }>('/persistent-volumes');
  if (res.error || !res.data) return { error: res.error ?? 'Failed to load volumes' };
  return { volumes: res.data.volumes };
}

export async function cleanupOrphanedVolume(volumeId: string): Promise<{ error?: string }> {
  const res = await api.post(`/persistent-volumes/${volumeId}/cleanup`);
  if (res.error) return { error: res.error };
  return {};
}
