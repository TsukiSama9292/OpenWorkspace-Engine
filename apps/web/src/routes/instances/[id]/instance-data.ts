import { api } from '$lib/api/client';
import type { Instance } from '$lib/types';

export async function loadInstanceDetail(instanceId: string): Promise<Instance | null> {
  const res = await api.get<{ instance: Instance }>(`/instances/${instanceId}`);
  return res.data?.instance ?? null;
}
