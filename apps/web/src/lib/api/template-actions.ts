import type { Instance } from '$lib/types';
import { api } from '$lib/api/client';

export async function launchInstance(templateId: string): Promise<{ error?: string; instance?: Instance }> {
  const res = await api.post<{ instance: Instance }>('/instances', { template_id: templateId });
  if (res.error) return { error: res.error };
  if (res.data?.instance) return { instance: res.data.instance };
  return { error: 'Failed to launch instance' };
}

export async function deleteTemplate(templateId: string): Promise<{ error?: string }> {
  if (!confirm('Delete this template? Instances must be stopped first.')) return {};
  const res = await api.delete(`/templates/${templateId}`);
  if (res.error) return { error: res.error };
  return {};
}
