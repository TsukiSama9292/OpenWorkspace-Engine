import { describe, it, expect } from 'vitest';
import {
  emptyQuotaForm,
  quotaFormFromUser,
  buildQuotaOverrides,
  type UserQuotaRow,
} from '$lib/users/user-quota';

describe('user quota form', () => {
  it('maps an empty field to null (inherit role default)', () => {
    const overrides = buildQuotaOverrides(emptyQuotaForm());
    expect(overrides).toEqual({
      instance_limit: null,
      max_cpu_cores: null,
      max_ram_bytes: null,
    });
  });

  it('parses populated fields to numbers', () => {
    const overrides = buildQuotaOverrides({
      instance_limit: '7',
      max_cpu_cores: '9',
      max_ram_bytes: '10737418240',
    });
    expect(overrides).toEqual({
      instance_limit: 7,
      max_cpu_cores: 9,
      max_ram_bytes: 10737418240,
    });
  });

  it('mixes empty and populated fields independently', () => {
    const overrides = buildQuotaOverrides({
      instance_limit: '3',
      max_cpu_cores: '',
      max_ram_bytes: '5368709120',
    });
    expect(overrides).toEqual({
      instance_limit: 3,
      max_cpu_cores: null,
      max_ram_bytes: 5368709120,
    });
  });

  it('populates the form from a user row, empty when the override is null', () => {
    const row: UserQuotaRow = {
      instance_limit: 7,
      max_cpu_cores: null,
      max_ram_bytes: null,
      effective_instance_limit: 7,
      effective_max_cpu_cores: 4,
      effective_max_ram_bytes: 8589934592,
      quota_exempt: false,
    };
    expect(quotaFormFromUser(row)).toEqual({
      instance_limit: '7',
      max_cpu_cores: '',
      max_ram_bytes: '',
    });
  });

  it('round-trips an admin override through form and back', () => {
    const form = quotaFormFromUser({
      instance_limit: null,
      max_cpu_cores: 16,
      max_ram_bytes: 34359738368,
      effective_instance_limit: 5,
      effective_max_cpu_cores: 16,
      effective_max_ram_bytes: 34359738368,
      quota_exempt: false,
    });
    expect(buildQuotaOverrides(form)).toEqual({
      instance_limit: null,
      max_cpu_cores: 16,
      max_ram_bytes: 34359738368,
    });
  });
});
