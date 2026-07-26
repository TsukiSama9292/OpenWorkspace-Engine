import type { Instance } from '$lib/types';
import { api } from '$lib/api/client';

export async function launchInstance(configId: string): Promise<{ error?: string; instance?: Instance }> {
  const res = await api.post<{ instance: Instance }>('/instances', { config_id: configId });
  if (res.error) return { error: res.error };
  if (res.data?.instance) return { instance: res.data.instance };
  return { error: 'Failed to launch instance' };
}

export async function deleteConfig(configId: string): Promise<{ error?: string }> {
  if (!confirm('Delete this config? Instances must be stopped first.')) return {};
  const res = await api.delete(`/configs/${configId}`);
  if (res.error) return { error: res.error };
  return {};
}
