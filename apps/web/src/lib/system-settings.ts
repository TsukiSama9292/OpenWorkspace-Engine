import {
  memoryMbFromTriState,
  memoryMbToTriState,
  triStateFromValue,
  valueFromTriState,
  type TriState
} from '$lib/tri-state';

export type SystemSettingsValue = {
  host_instance_limit: number;
  host_cpu_cores: number;
  host_memory_mb: number;
  host_gpu_count: number;
};

export interface AdminSettingsFormState {
  hostInstanceLimit: TriState;
  hostCpu: TriState;
  hostMemory: TriState;
  hostGpu: TriState;
}

export function settingsFormFromValue(settings: SystemSettingsValue): AdminSettingsFormState {
  return {
    hostInstanceLimit: triStateFromValue(settings.host_instance_limit),
    hostCpu: triStateFromValue(settings.host_cpu_cores),
    hostMemory: memoryMbToTriState(settings.host_memory_mb),
    hostGpu: triStateFromValue(settings.host_gpu_count)
  };
}

export function settingsValueFromForm(form: AdminSettingsFormState): SystemSettingsValue {
  return {
    host_instance_limit: valueFromTriState(form.hostInstanceLimit),
    host_cpu_cores: valueFromTriState(form.hostCpu),
    host_memory_mb: memoryMbFromTriState(form.hostMemory),
    host_gpu_count: valueFromTriState(form.hostGpu)
  };
}
