import { api } from '$lib/api/client';
import type { Config, Instance } from '$lib/types';

export async function loadConfigDetail(configId: string) {
  const [configRes, instancesRes] = await Promise.all([
    api.get<{ config: Config }>(`/configs/${configId}`),
    api.get<{ instances: Instance[] }>('/instances'),
  ]);
  return {
    config: configRes.data?.config ?? null,
    instances: (instancesRes.data?.instances ?? []).filter((i) => i.config_id === configId),
  };
}
