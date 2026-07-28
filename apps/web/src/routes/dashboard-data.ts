import { api } from '$lib/api/client';
import type { Template, Instance } from '$lib/types';

export async function loadDashboard() {
  const [configRes, instanceRes] = await Promise.all([
    api.get<{ templates: Template[] }>('/templates'),
    api.get<{ instances: Instance[] }>('/instances'),
  ]);
  return {
    configs: configRes.data?.templates ?? [],
    instances: instanceRes.data?.instances ?? [],
  };
}
