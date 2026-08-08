import { api } from '$lib/api/client';
import type { MonitorSnapshot } from '$lib/types';

export async function fetchMonitorSnapshot(): Promise<{ snapshot?: MonitorSnapshot; error?: string }> {
  const res = await api.get<MonitorSnapshot>('/monitor/snapshot');
  if (res.error || !res.data) return { error: res.error ?? 'Failed to load monitoring data' };
  return { snapshot: res.data };
}
