import { api } from '$lib/api/client';
import { goto } from '$app/navigation';
import { buildRunConfig, buildExecConfig, buildVolumeMappings, createEmptyEnvVar, createEmptyVolume } from '$lib/utils/format';
import type { EnvVar, VolumeMapping } from '$lib/utils/format';
import type { Template, RemoteType } from '$lib/types';
import { DEFAULT_IMAGES } from '../../new/template-create';

export type { TemplateFormState } from '../../new/template-create';

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

export function formStateFromTemplate(t: Template) {
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
    envVars: rc.envVars,
    execCommand: parseExecConfig(t.exec_config as Record<string, unknown>),
    volumeMappings: parseVolumeMappings(t.volume_mappings as Record<string, string>),
    showAdvanced: false,
    loading: false,
    error: '',
  };
}

export async function loadTemplate(id: string): Promise<{ state?: ReturnType<typeof formStateFromTemplate>; error?: string }> {
  const res = await api.get<{ template: Template }>('/templates/' + id);
  if (res.error) return { error: res.error };
  if (!res.data?.template) return { error: 'Template not found' };
  return { state: formStateFromTemplate(res.data.template) };
}

export async function updateTemplate(id: string, state: ReturnType<typeof formStateFromTemplate>): Promise<{ error?: string }> {
  if (!state.name.trim()) return { error: 'Name is required' };

  const body = {
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
  };

  const res = await api.put<{ template: { id: string } }>('/templates/' + id, body);
  if (res.error) return { error: res.error };
  if (res.data?.template) {
    goto('/');
    return {};
  }
  return { error: 'Failed to update template' };
}
