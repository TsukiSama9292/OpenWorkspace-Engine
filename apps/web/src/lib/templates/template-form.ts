import { api } from '$lib/api/client';
import { buildRunConfig, buildExecConfig, buildVolumeMappings, createEmptyEnvVar, createEmptyVolume } from '$lib/utils/format';
import type { EnvVar, VolumeMapping } from '$lib/utils/format';
import type { Template, RemoteType, TimeoutAction, TemplateAllocationMode } from '$lib/types';

export type { TimeoutAction } from '$lib/types';

export const DEFAULT_IMAGES: Record<RemoteType, string> = {
  kasmvnc: 'tsukisama9292/ow-kasmvnc-ubuntu-dini:jammy',
  ttyd: 'tsukisama9292/ow-ttyd-ubuntu-dini:jammy',
  jupyter: 'tsukisama9292/ow-jupyter-ubuntu-dini:jammy',
};

export interface TemplateFormState {
  name: string;
  description: string;
  image: string;
  cores: number;
  ramGb: number;
  gpuCount: number;
  dockerRegistry: string;
  persistentStoragePath: string;
  remoteType: RemoteType;
  hostname: string;
  dns: string;
  shmSize: string;
  networkMode: string;
  containerRuntime: string;
  maxRunSeconds: number | null;
  timeoutAction: TimeoutAction;
  keepTimeSeconds: number | null;
  keepTimeAction: TimeoutAction;
  bandwidthUpMbps: number;
  bandwidthDownMbps: number;
  dockerInInstance: boolean;
  allocationMode: TemplateAllocationMode;
  envVars: EnvVar[];
  execCommand: string;
  volumeMappings: VolumeMapping[];
  showAdvanced: boolean;
  loading: boolean;
  error: string;
}

export function createInitialFormState(): TemplateFormState {
  return {
    name: '',
    description: '',
    image: DEFAULT_IMAGES.kasmvnc,
    cores: 2,
    ramGb: 4,
    gpuCount: 0,
    dockerRegistry: '',
    persistentStoragePath: '',
    remoteType: 'kasmvnc',
    hostname: '',
    dns: '',
    shmSize: '',
    networkMode: '',
    containerRuntime: '',
    maxRunSeconds: null,
    timeoutAction: 'remove',
    keepTimeSeconds: null,
    keepTimeAction: 'pause',
    bandwidthUpMbps: 0,
    bandwidthDownMbps: 0,
    dockerInInstance: false,
    allocationMode: 'shared',
    envVars: [createEmptyEnvVar()],
    execCommand: '',
    volumeMappings: [createEmptyVolume()],
    showAdvanced: false,
    loading: false,
    error: '',
  };
}

function buildTemplateBody(state: TemplateFormState): Record<string, unknown> {
  return {
    name: state.name.trim(),
    description: state.description || null,
    image: state.image,
    cores: state.cores,
    memory: state.ramGb * 1024 * 1024 * 1024,
    gpu_count: state.gpuCount,
    container_runtime: state.containerRuntime,
    docker_registry: state.dockerRegistry || null,
    remote_type: state.remoteType,
    run_config: buildRunConfig({
      hostname: state.hostname,
      dns: state.dns,
      shmSize: state.shmSize,
      networkMode: state.networkMode,
      envVars: state.envVars,
    }),
    exec_config: buildExecConfig(state.execCommand),
    volume_mappings: buildVolumeMappings(state.volumeMappings),
    persistent_storage_path: state.persistentStoragePath || null,
    max_run_seconds: state.maxRunSeconds,
    timeout_action: state.timeoutAction,
    keep_time_seconds: state.keepTimeSeconds,
    keep_time_action: state.keepTimeAction,
    network_bandwidth_up_mbps: state.bandwidthUpMbps,
    network_bandwidth_down_mbps: state.bandwidthDownMbps,
    docker_in_instance: state.dockerInInstance,
    allocation_mode: state.allocationMode,
  };
}

function validateBandwidth(state: TemplateFormState): string | undefined {
  if (state.bandwidthUpMbps < 0) return 'Upload bandwidth must be >= 0 (0 = unlimited)';
  if (state.bandwidthDownMbps < 0) return 'Download bandwidth must be >= 0 (0 = unlimited)';
  return undefined;
}

export async function submitTemplate(state: TemplateFormState): Promise<{ error?: string; id?: string }> {
  if (!state.name.trim()) return { error: 'Name is required' };
  const bandwidthError = validateBandwidth(state);
  if (bandwidthError) return { error: bandwidthError };

  const res = await api.post<{ template: { id: string } }>('/templates', buildTemplateBody(state));
  if (res.error) return { error: res.error };
  if (res.data?.template) {
    return { id: res.data.template.id };
  }
  return { error: 'Failed to create template' };
}

function parseRunConfig(runConfig: Record<string, unknown>): { hostname: string; dns: string; shmSize: string; networkMode: string; envVars: EnvVar[] } {
  const hostname = typeof runConfig.hostname === 'string' ? runConfig.hostname : '';
  const dns = Array.isArray(runConfig.dns) ? (runConfig.dns as string[]).join(', ') : '';
  const shmSize = typeof runConfig.shm_size === 'number' ? String(runConfig.shm_size) : '';
  const networkMode = typeof runConfig.network_mode === 'string' ? runConfig.network_mode : '';
  const envVars: EnvVar[] = Array.isArray(runConfig.environment)
    ? (runConfig.environment as string[]).map(e => {
        const idx = e.indexOf('=');
        return idx > 0 ? { key: e.slice(0, idx), value: e.slice(idx + 1) } : { key: e, value: '' };
      })
    : [createEmptyEnvVar()];
  return { hostname, dns, shmSize, networkMode, envVars };
}

function parseExecConfig(execConfig: Record<string, unknown>): string {
  if (typeof execConfig.go === 'object' && execConfig.go && typeof (execConfig.go as Record<string, unknown>).cmd === 'string') {
    return (execConfig.go as Record<string, unknown>).cmd as string;
  }
  return '';
}

function parseVolumeMappings(volMappings: Record<string, string>): VolumeMapping[] {
  const entries = Object.entries(volMappings || {});
  return entries.length > 0 ? entries.map(([host, container]) => ({ host, container })) : [createEmptyVolume()];
}

export function formStateFromTemplate(t: Template): TemplateFormState {
  const rc = parseRunConfig(t.run_config as Record<string, unknown>);
  return {
    name: t.name,
    description: t.description || '',
    image: t.image,
    cores: t.cores,
    ramGb: Math.round(t.memory / (1024 * 1024 * 1024)),
    gpuCount: t.gpu_count,
    dockerRegistry: t.docker_registry || '',
    persistentStoragePath: t.persistent_storage_path || '',
    remoteType: t.remote_type as RemoteType,
    hostname: rc.hostname,
    dns: rc.dns,
    shmSize: rc.shmSize,
    networkMode: rc.networkMode,
    containerRuntime: t.container_runtime,
    maxRunSeconds: t.max_run_seconds ?? null,
    timeoutAction: t.timeout_action ?? 'remove',
    keepTimeSeconds: t.keep_time_seconds ?? null,
    keepTimeAction: (t.keep_time_action ?? 'pause') as TimeoutAction,
    bandwidthUpMbps: t.network_bandwidth_up_mbps ?? 0,
    bandwidthDownMbps: t.network_bandwidth_down_mbps ?? 0,
    dockerInInstance: t.docker_in_instance ?? false,
    allocationMode: t.allocation_mode === 'dedicated' ? 'dedicated' : 'shared',
    envVars: rc.envVars,
    execCommand: parseExecConfig(t.exec_config as Record<string, unknown>),
    volumeMappings: parseVolumeMappings(t.volume_mappings as Record<string, string>),
    showAdvanced: false,
    loading: false,
    error: '',
  };
}

export async function loadTemplate(id: string): Promise<{ state?: TemplateFormState; error?: string }> {
  const res = await api.get<{ template: Template }>('/templates/' + id);
  if (res.error) return { error: res.error };
  if (!res.data?.template) return { error: 'Template not found' };
  return { state: formStateFromTemplate(res.data.template) };
}

export async function updateTemplate(id: string, state: TemplateFormState): Promise<{ error?: string }> {
  if (!state.name.trim()) return { error: 'Name is required' };
  const bandwidthError = validateBandwidth(state);
  if (bandwidthError) return { error: bandwidthError };

  const res = await api.put<{ template: { id: string } }>('/templates/' + id, buildTemplateBody(state));
  if (res.error) return { error: res.error };
  if (res.data?.template) {
    return {};
  }
  return { error: 'Failed to update template' };
}
