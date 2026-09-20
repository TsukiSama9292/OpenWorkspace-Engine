import { describe, it, expect } from 'vitest';
import {
  settingsFormFromValue,
  settingsValueFromForm,
  type SystemSettingsValue
} from '$lib/system-settings';

const settings: SystemSettingsValue = {
  host_instance_limit: 5,
  host_cpu_cores: -1,
  host_memory_mb: 0,
  host_gpu_count: 2
};

describe('settingsFormFromValue', () => {
  it('maps host caps into tri-state (memory in whole GB)', () => {
    const form = settingsFormFromValue(settings);
    expect(form.hostInstanceLimit).toEqual({ mode: 'custom', value: 5 });
    expect(form.hostCpu).toEqual({ mode: 'unlimited', value: 1 });
    expect(form.hostMemory).toEqual({ mode: 'disabled', value: 0 });
    expect(form.hostGpu).toEqual({ mode: 'custom', value: 2 });
  });
});

describe('settingsValueFromForm', () => {
  it('round-trips -1 / 0 / positive caps back to API values (memory in MB)', () => {
    const form = settingsFormFromValue({ ...settings, host_memory_mb: 16384 });
    expect(form.hostMemory).toEqual({ mode: 'custom', value: 16 });
    expect(settingsValueFromForm(form)).toEqual({
      host_instance_limit: 5,
      host_cpu_cores: -1,
      host_memory_mb: 16384,
      host_gpu_count: 2
    });
  });

  it('keeps -1 memory passthrough as -1, not scaled', () => {
    const form = settingsFormFromValue({ ...settings, host_memory_mb: -1 });
    expect(settingsValueFromForm(form).host_memory_mb).toBe(-1);
  });
});
