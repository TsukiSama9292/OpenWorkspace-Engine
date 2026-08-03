import type { Instance, QuotaPayload } from '$lib/types';
import { api } from '$lib/api/client';

export type LaunchPersistence = 'use_persistent' | 'no_persistent' | 'reset_persistent';

export async function launchInstance(
  templateId: string,
  persistence: LaunchPersistence = 'no_persistent'
): Promise<{ error?: string; instance?: Instance; quota?: QuotaPayload }> {
  const wantsPersistent = persistence !== 'no_persistent';
  const res = await api.post<{ instance: Instance }>('/instances', {
    template_id: templateId,
    persistence,
    mount_persistent: wantsPersistent
  });
  if (res.error) return { error: res.error, quota: res.quota };
  if (res.data?.instance) return { instance: res.data.instance };
  return { error: 'Failed to launch instance' };
}

export async function deleteTemplate(templateId: string): Promise<{ error?: string; cancelled?: boolean }> {
  if (!confirm('Delete this template? Instances must be stopped first.')) return { cancelled: true };
  const res = await api.delete(`/templates/${templateId}`);
  if (res.error) return { error: res.error };
  return {};
}
