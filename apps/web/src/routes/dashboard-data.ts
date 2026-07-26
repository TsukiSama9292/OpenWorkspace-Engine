import { api } from '$lib/api/client';
import type { Config, Instance } from '$lib/types';

export async function loadDashboard() {
  const [configRes, instanceRes] = await Promise.all([
    api.get<{ configs: Config[] }>('/configs'),
    api.get<{ instances: Instance[] }>('/instances'),
  ]);
  return {
    configs: configRes.data?.configs ?? [],
    instances: instanceRes.data?.instances ?? [],
  };
}
