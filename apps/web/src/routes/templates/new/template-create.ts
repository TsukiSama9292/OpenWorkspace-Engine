import { api } from '$lib/api/client';
import { goto } from '$app/navigation';
import { buildRunConfig, buildExecConfig, buildVolumeMappings, createEmptyEnvVar, createEmptyVolume } from '$lib/utils/format';
import type { EnvVar, VolumeMapping } from '$lib/utils/format';
import type { RemoteType } from '$lib/types';

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
    image: 'kasmweb/desktop:1.19.0-rolling-daily',
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
    envVars: [createEmptyEnvVar()],
    execCommand: '',
    volumeMappings: [createEmptyVolume()],
    showAdvanced: false,
    loading: false,
    error: '',
  };
}

export async function submitTemplate(state: TemplateFormState): Promise<{ error?: string; id?: string }> {
  if (!state.name.trim()) return { error: 'Name is required' };

  const body = {
    name: state.name.trim(),
    description: state.description || null,
    image: state.image,
    cores: state.cores,
    memory: state.ramGb * 1024 * 1024 * 1024,
    gpu_count: state.gpuCount,
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

  const res = await api.post<{ template: { id: string } }>('/templates', body);
  if (res.error) return { error: res.error };
  if (res.data?.template) {
    goto('/');
    return { id: res.data.template.id };
  }
  return { error: 'Failed to create template' };
}
