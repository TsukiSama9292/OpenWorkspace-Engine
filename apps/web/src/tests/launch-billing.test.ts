import { describe, it, expect } from 'vitest';
import {
  billingGroupName,
  compareBillingGroups,
  defaultBillingGroup,
  describeBillingOption,
  instanceResourceLabel
} from '$lib/launch-billing';
import type { GroupBilling, Instance } from '$lib/types';

function billing(overrides: Partial<GroupBilling> = {}): GroupBilling {
  return {
    group_id: 'g1',
    group_name: 'Devs',
    tier: 0,
    billing_model: 'shared',
    pool_cpu_cores: 16,
    pool_memory_mb: 65536,
    pool_gpu_count: 2,
    member_cpu_cores: -1,
    member_memory_mb: 8192,
    member_gpu_count: 1,
    ...overrides
  };
}

function instance(overrides: Partial<Instance> = {}): Instance {
  return {
    id: 'i1',
    name: 'box',
    template_id: 't1',
    template_name: 'Dev VM',
    remote_type: 'kasmvnc',
    owner_id: 'me',
    owner_username: 'me',
    owner_group_ids: [],
    owner_tier: 0,
    status: 'running',
    instance_number: 1,
    container_id: 'c1',
    mount_persistent: false,
    created_at: '2026-01-01T00:00:00Z',
    updated_at: '2026-01-01T00:00:00Z',
    ...overrides
  };
}

describe('compareBillingGroups / defaultBillingGroup', () => {
  it('ranks an unlimited membership above a finite one', () => {
    const unlimited = billing({ group_id: 'a', member_cpu_cores: -1, member_memory_mb: -1, member_gpu_count: -1 });
    const finite = billing({ group_id: 'b', member_cpu_cores: 8, member_memory_mb: 16384, member_gpu_count: 0 });
    expect(compareBillingGroups(unlimited, finite)).toBeLessThan(0);
    expect(defaultBillingGroup([finite, unlimited])).toBe('a');
  });

  it('ranks a higher member cap above a lower one across CPU, memory, then GPU', () => {
    const moreCpu = billing({ group_id: 'a', member_cpu_cores: 16, member_memory_mb: 8192, member_gpu_count: 0 });
    const moreMem = billing({ group_id: 'b', member_cpu_cores: 8, member_memory_mb: 16384, member_gpu_count: 0 });
    expect(defaultBillingGroup([moreMem, moreCpu])).toBe('a');

    const moreGpu = billing({ group_id: 'c', member_cpu_cores: 8, member_memory_mb: 8192, member_gpu_count: 2 });
    expect(defaultBillingGroup([moreGpu, moreMem])).toBe('b');
  });

  it('treats zero (blocked) as the lowest cap', () => {
    const blocked = billing({ group_id: 'a', member_cpu_cores: 0, member_memory_mb: 0, member_gpu_count: 0 });
    const finite = billing({ group_id: 'b', member_cpu_cores: 4, member_memory_mb: 8192, member_gpu_count: 1 });
    expect(defaultBillingGroup([blocked, finite])).toBe('b');
  });

  it('breaks equal caps by tier, then group name', () => {
    const manager = billing({ group_id: 'a', group_name: 'Mgrs', tier: 1 });
    const tenant = billing({ group_id: 'b', group_name: 'Devs', tier: 0 });
    expect(defaultBillingGroup([tenant, manager])).toBe('a');
  });

  it('falls back to the single membership when there is only one', () => {
    expect(defaultBillingGroup([billing()])).toBe('g1');
    expect(defaultBillingGroup([])).toBe('');
  });
});

describe('billingGroupName', () => {
  it('prefers the frozen snapshot name over live membership', () => {
    const inst = instance({
      owner_group_id: 'g1',
      billing_group_snapshot: {
        group_id: 'g1',
        group_name: 'Old Name',
        billing_model: 'shared',
        pool_cpu_cores: 4,
        pool_memory_mb: 8192,
        pool_gpu_count: 0
      }
    });
    expect(billingGroupName(inst, [billing({ group_name: 'New Name' })])).toBe('Old Name');
  });

  it('falls back to live membership by owner_group_id', () => {
    const inst = instance({ owner_group_id: 'g1' });
    expect(billingGroupName(inst, [billing()])).toBe('Devs');
  });

  it('falls back to a short group id when no name is available', () => {
    const inst = instance({ owner_group_id: '0123456789abcdef' });
    expect(billingGroupName(inst, [])).toBe('01234567');
  });

  it('returns null when the instance has no billing target at all', () => {
    expect(billingGroupName(instance({ owner_group_id: null }), [])).toBeNull();
  });
});

describe('instanceResourceLabel', () => {
  it('renders a compact resource usage label', () => {
    const inst = instance({ host_cpu_cores: 4, host_memory_mb: 16384, host_gpu_count: 1 });
    expect(instanceResourceLabel(inst)).toBe('4 CPU · 16 GB · 1 GPU');
  });

  it('renders memory below 1 GB in MB', () => {
    const inst = instance({ host_cpu_cores: 2, host_memory_mb: 512, host_gpu_count: 0 });
    expect(instanceResourceLabel(inst)).toBe('2 CPU · 512 MB · 0 GPU');
  });

  it('returns null when the host snapshot columns are absent', () => {
    expect(instanceResourceLabel(instance())).toBeNull();
  });
});

describe('describeBillingOption', () => {
  it('describes unlimited, blocked and finite caps', () => {
    expect(describeBillingOption(billing())).toBe('Devs — CPU unlimited · Mem 8 GB · GPU 1');
    expect(
      describeBillingOption(billing({ member_cpu_cores: 0, member_memory_mb: 0, member_gpu_count: 0 }))
    ).toBe('Devs — CPU blocked · Mem blocked · GPU blocked');
    expect(
      describeBillingOption(billing({ member_memory_mb: -1, member_gpu_count: -1 }))
    ).toBe('Devs — CPU unlimited · Mem unlimited · GPU unlimited');
  });
});
