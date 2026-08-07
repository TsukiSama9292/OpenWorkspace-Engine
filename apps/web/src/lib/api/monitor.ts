import { api } from '$lib/api/client';
import type { MonitorRange, MonitorSnapshot } from '$lib/types';

export async function fetchMonitorSnapshot(range: MonitorRange): Promise<{ snapshot?: MonitorSnapshot; error?: string }> {
  const res = await api.get<MonitorSnapshot>(`/monitor/snapshot?range=${range}`);
  if (res.error || !res.data) return { error: res.error ?? 'Failed to load monitoring data' };
  return { snapshot: res.data };
}
