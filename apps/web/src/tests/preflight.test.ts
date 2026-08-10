import { describe, it, expect } from 'vitest';
import { preflightMessage, preflightNumbers, preflightTitle } from '$lib/preflight';
import { isPreflightRejection, type PreflightRejection } from '$lib/types';

const NOT_ALLOWED: PreflightRejection = { scope: 'template_not_allowed', current: 0, limit: 0, requested: 1 };
const HIDDEN: PreflightRejection = { scope: 'template_hidden', current: 0, limit: 0, requested: 1 };
const USER_CEILING: PreflightRejection = { scope: 'user_instance', current: 2, limit: 2, requested: 1 };
const HOST_CEILING: PreflightRejection = { scope: 'host_instance', current: 5, limit: 5, requested: 1 };
const HOST_CPU: PreflightRejection = { scope: 'host_resource_cpu', current: 18, limit: 20, requested: 2 };
const GROUP_POOL_CPU: PreflightRejection = {
  scope: 'group_pool_cpu', current: 2, limit: 2, requested: 1, group_id: 'g1',
};
const MEMBER_QUOTA_MEMORY: PreflightRejection = {
  scope: 'member_quota_memory', current: 4096, limit: 8192, requested: 1,
};
const HOST_MEMORY_UNLIMITED: PreflightRejection = { scope: 'host_resource_memory', current: 4, limit: -1, requested: 1 };

describe('preflight rejection copy', () => {
  it('renders a template-not-allowed rejection (403) without ceiling numbers', () => {
    expect(preflightTitle(NOT_ALLOWED)).toBe('Template not allowed');
    expect(preflightNumbers(NOT_ALLOWED)).toBeNull();

    const msg = preflightMessage(NOT_ALLOWED, 'This template is not in your allowed templates list');
    expect(msg).toContain('Template not allowed');
    expect(msg).toContain('This template is not in your allowed templates list');
  });

  it('renders a template-hidden rejection without ceiling numbers', () => {
    expect(preflightTitle(HIDDEN)).toBe('Template is hidden');
    expect(preflightNumbers(HIDDEN)).toBeNull();
  });

  it('renders a host CPU cap rejection with the exact numbers', () => {
    expect(preflightTitle(HOST_CPU)).toBe('Host CPU cap reached');
    expect(preflightNumbers(HOST_CPU)).toBe('Current 18 / limit 20 (requested 2)');

    const msg = preflightMessage(HOST_CPU, 'Host cpu cap reached (used: 18, cap: 20)');
    expect(msg).toContain('Host CPU cap reached');
    expect(msg).toContain('Current 18 / limit 20 (requested 2)');
  });

  it('renders a group pool CPU rejection (billing-group capped)', () => {
    expect(preflightTitle(GROUP_POOL_CPU)).toBe('Group CPU pool cap reached');
    expect(preflightNumbers(GROUP_POOL_CPU)).toBe('Current 2 / limit 2 (requested 1)');
    expect(GROUP_POOL_CPU.group_id).toBe('g1');
  });

  it('renders memory quota values as human-readable sizes, not raw MB', () => {
    expect(preflightTitle(MEMBER_QUOTA_MEMORY)).toBe('Personal memory quota reached');
    expect(preflightNumbers(MEMBER_QUOTA_MEMORY)).toBe('Current 4 GB / limit 8 GB (requested 1)');
  });

  it('renders an unlimited (-1) memory cap as "unlimited"', () => {
    expect(preflightNumbers(HOST_MEMORY_UNLIMITED)).toBe('Current 4 MB / limit unlimited (requested 1)');
  });

  it('renders a user ceiling rejection (409) with the exact numbers', () => {
    expect(preflightTitle(USER_CEILING)).toBe('Your instance limit reached');
    expect(preflightNumbers(USER_CEILING)).toBe('Current 2 / limit 2 (requested 1)');

    const msg = preflightMessage(USER_CEILING, 'Per-user instance limit reached (active: 2, limit: 2)');
    expect(msg).toContain('Your instance limit reached');
    expect(msg).toContain('Current 2 / limit 2 (requested 1)');
  });

  it('renders a host ceiling rejection (409) with the exact numbers', () => {
    expect(preflightTitle(HOST_CEILING)).toBe('Host instance limit reached');
    expect(preflightNumbers(HOST_CEILING)).toBe('Current 5 / limit 5 (requested 1)');

    const msg = preflightMessage(HOST_CEILING, 'Host instance limit reached (active: 5, limit: 5)');
    expect(msg).toContain('Host instance limit reached');
    expect(msg).toContain('Current 5 / limit 5 (requested 1)');
  });

  it('works without an API error string', () => {
    expect(preflightMessage(USER_CEILING, '')).toBe('Your instance limit reached: Current 2 / limit 2 (requested 1)');
  });
});

describe('isPreflightRejection', () => {
  it('accepts a valid body for every scope', () => {
    for (const scope of [
      'template_not_allowed',
      'template_hidden',
      'user_instance',
      'host_instance',
      'host_resource_cpu',
      'host_resource_memory',
      'host_resource_gpu',
      'group_pool_cpu',
      'group_pool_memory',
      'group_pool_gpu',
      'member_quota_cpu',
      'member_quota_memory',
      'member_quota_gpu',
    ]) {
      expect(isPreflightRejection({ scope, current: 2, limit: 2, requested: 1 })).toBe(true);
    }
  });

  it('rejects null, non-objects, and malformed bodies', () => {
    expect(isPreflightRejection(null)).toBe(false);
    expect(isPreflightRejection(undefined)).toBe(false);
    expect(isPreflightRejection('user_instance')).toBe(false);
    expect(isPreflightRejection({ scope: 'user_instance', current: 2 })).toBe(false);
    expect(isPreflightRejection({ scope: 'bogus', current: 2, limit: 2, requested: 1 })).toBe(false);
    expect(isPreflightRejection({ scope: 'user_instance', current: '2', limit: 2, requested: 1 })).toBe(false);
  });
});
