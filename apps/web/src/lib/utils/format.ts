export function formatMemory(bytes: number | null | undefined): string {
  if (!bytes) return '—';
  const gb = bytes / (1024 * 1024 * 1024);
  return gb >= 1 ? `${gb} GB` : `${bytes / (1024 * 1024)} MB`;
}

export function formatBytes(bytes: number | null | undefined): string {
  if (!bytes || bytes <= 0) return '—';
  const units = ['B', 'KB', 'MB', 'GB', 'TB'];
  let value = bytes;
  let unit = 0;
  while (value >= 1024 && unit < units.length - 1) {
    value /= 1024;
    unit += 1;
  }
  return `${Number.isInteger(value) ? value : value.toFixed(1)} ${units[unit]}`;
}

export function formatPercent(value: number | null | undefined): string {
  if (value == null || Number.isNaN(value)) return '—';
  return `${Math.round(value)}%`;
}

export function formatUptime(secs: number | null | undefined): string {
  if (secs == null || secs < 0) return '—';
  const s = Math.floor(secs);
  const d = Math.floor(s / 86400);
  const h = Math.floor((s % 86400) / 3600);
  const m = Math.floor((s % 3600) / 60);
  const rest = s % 60;
  if (d > 0) return `${d}d ${h}h`;
  if (h > 0) return `${h}h ${m}m`;
  if (m > 0) return `${m}m ${rest}s`;
  return `${rest}s`;
}

export interface EnvVar {
  key: string;
  value: string;
}

export interface VolumeMapping {
  host: string;
  container: string;
}

export function buildRunConfig(params: {
  hostname: string;
  dns: string;
  shmSize: string;
  networkMode: string;
  envVars: EnvVar[];
}): Record<string, unknown> {
  const runConfig: Record<string, unknown> = {};
  if (params.hostname) runConfig.hostname = params.hostname;
  if (params.dns) runConfig.dns = params.dns.split(',').map(s => s.trim()).filter(Boolean);
  if (params.shmSize) runConfig.shm_size = parseInt(params.shmSize);
  if (params.networkMode) runConfig.network_mode = params.networkMode;
  const envList = params.envVars.filter(e => e.key.trim()).map(e => `${e.key}=${e.value}`);
  if (envList.length) runConfig.environment = envList;
  return runConfig;
}

export function buildExecConfig(execCommand: string): Record<string, unknown> {
  const execConfig: Record<string, unknown> = {};
  if (execCommand.trim()) execConfig.go = { cmd: execCommand.trim() };
  return execConfig;
}

export function buildVolumeMappings(volumeMappings: VolumeMapping[]): Record<string, string> {
  const volMap: Record<string, string> = {};
  volumeMappings.filter(v => v.host && v.container).forEach(v => { volMap[v.host] = v.container; });
  return volMap;
}

export function createEmptyEnvVar(): EnvVar {
  return { key: '', value: '' };
}

export function createEmptyVolume(): VolumeMapping {
  return { host: '', container: '' };
}
