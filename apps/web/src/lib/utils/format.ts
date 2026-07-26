export function formatMemory(bytes: number | null | undefined): string {
  if (!bytes) return '—';
  const gb = bytes / (1024 * 1024 * 1024);
  return gb >= 1 ? `${gb} GB` : `${bytes / (1024 * 1024)} MB`;
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
