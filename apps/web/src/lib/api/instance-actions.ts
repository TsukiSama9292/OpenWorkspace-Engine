import type { Instance } from '$lib/types';
import { api } from '$lib/api/client';
import { goto } from '$app/navigation';

const STATUS_MAP: Record<string, Instance['status']> = {
  start: 'running',
  stop: 'stopped',
  pause: 'paused',
  unpause: 'running',
};

export async function performAction(
  instanceId: string,
  action: string,
): Promise<{ error?: string; status?: Instance['status'] }> {
  const res = await api.post(`/instances/${instanceId}/${action}`);
  if (res.error) return { error: res.error };
  return { status: STATUS_MAP[action] };
}

export async function deleteInstance(instanceId: string): Promise<{ error?: string }> {
  if (!confirm('Delete this instance? The container will be removed.')) return {};
  const res = await api.delete(`/instances/${instanceId}`);
  if (res.error) return { error: res.error };
  goto('/');
  return {};
}
