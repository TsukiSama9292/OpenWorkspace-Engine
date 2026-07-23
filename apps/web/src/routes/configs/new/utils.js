export function formatMemory(bytes) {
  if (!bytes) return '—';
  const gb = bytes / (1024 * 1024 * 1024);
  return gb >= 1 ? `${gb} GB` : `${bytes / (1024 * 1024)} MB`;
}

export function buildRunConfig({ hostname, dns, shmSize, networkMode, envVars }) {
  const runConfig = {};
  if (hostname) runConfig.hostname = hostname;
  if (dns) runConfig.dns = dns.split(',').map(s => s.trim()).filter(Boolean);
  if (shmSize) runConfig.shm_size = parseInt(shmSize);
  if (networkMode) runConfig.network_mode = networkMode;
  const envList = envVars.filter(e => e.key.trim()).map(e => `${e.key}=${e.value}`);
  if (envList.length) runConfig.environment = envList;
  return runConfig;
}

export function buildExecConfig(execCommand) {
  const execConfig = {};
  if (execCommand.trim()) execConfig.go = { cmd: execCommand.trim() };
  return execConfig;
}

export function buildVolumeMappings(volumeMappings) {
  const volMap = {};
  volumeMappings.filter(v => v.host && v.container).forEach(v => { volMap[v.host] = v.container; });
  return volMap;
}

export function createEmptyEnvVar() {
  return { key: '', value: '' };
}

export function createEmptyVolume() {
  return { host: '', container: '' };
}
