import { describe, it, expect, vi, beforeEach } from 'vitest';
import { get } from 'svelte/store';
import {
  auth,
  isAuthenticated,
  isAdmin,
  canCreateTemplate,
  canManageUsers,
  canManageGroupInstances,
  canManageDocker,
  canManageRegistry,
  canViewMonitoring,
  effectiveMaxInstances,
  allowedTemplateIds
} from '$lib/stores/auth';
import type { EffectiveContext } from '$lib/types';

vi.mock('$lib/api/client', () => ({
  api: {
    get: vi.fn(),
    post: vi.fn()
  }
}));

import { api } from '$lib/api/client';
const mockApi = vi.mocked(api);

function context(overrides: Partial<EffectiveContext> = {}): EffectiveContext {
  return {
    user_id: 'u1',
    username: 'alice',
    is_admin: false,
    tier: 0,
    can_create_template: false,
    can_manage_users: false,
    can_manage_group_instances: false,
    can_manage_docker: false,
    can_manage_registry: false,
    can_view_monitoring: false,
    can_view_audit_logs: false,
    effective_max_instances: 4,
    allowed_template_ids: ['t1', 't2'],
    group_ids: ['g1'],
    direct_max_instances: null,
    ...overrides
  };
}

describe('auth store', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    auth.logout();
  });

  it('starts unauthenticated with defaulted helpers', () => {
    expect(get(auth)).toBeNull();
    expect(get(isAuthenticated)).toBe(false);
    expect(get(isAdmin)).toBe(false);
    expect(get(canCreateTemplate)).toBe(false);
    expect(get(canManageUsers)).toBe(false);
    expect(get(canManageGroupInstances)).toBe(false);
    expect(get(canManageDocker)).toBe(false);
    expect(get(canManageRegistry)).toBe(false);
    expect(get(canViewMonitoring)).toBe(false);
    expect(get(effectiveMaxInstances)).toBe(0);
    expect(get(allowedTemplateIds)).toEqual([]);
  });

  it('login populates the store from the { context } envelope', async () => {
    mockApi.post.mockResolvedValue({ data: { context: context() } });

    const result = await auth.login('alice', 'pass');
    expect(result).toBe(true);
    expect(get(auth)).toEqual(context());
    expect(get(isAuthenticated)).toBe(true);
  });

  it('login returns false on failure and leaves the store null', async () => {
    mockApi.post.mockResolvedValue({ error: 'Invalid credentials' });

    const result = await auth.login('wrong', 'pass');
    expect(result).toBe(false);
    expect(get(auth)).toBeNull();
    expect(get(isAuthenticated)).toBe(false);
  });

  it('logout clears the store', async () => {
    mockApi.post.mockResolvedValue({ data: { context: context() } });
    await auth.login('alice', 'pass');
    expect(get(isAuthenticated)).toBe(true);

    mockApi.post.mockResolvedValue({});
    await auth.logout();
    expect(get(auth)).toBeNull();
    expect(get(isAuthenticated)).toBe(false);
  });

  it('check populates the store from /auth/me', async () => {
    mockApi.get.mockResolvedValue({ data: { context: context({ username: 'bob' }) } });

    await auth.check();
    expect(get(auth)).toEqual(context({ username: 'bob' }));
    expect(get(isAuthenticated)).toBe(true);
  });

  it('check clears the store when /auth/me fails', async () => {
    mockApi.get.mockResolvedValue({ error: 'Not found' });

    await auth.check();
    expect(get(auth)).toBeNull();
    expect(get(isAuthenticated)).toBe(false);
  });

  it('derives each flag helper from its own flag', async () => {
    mockApi.get.mockResolvedValue({
      data: { context: context({ can_manage_users: true, can_create_template: true }) }
    });
    await auth.check();

    expect(get(canManageUsers)).toBe(true);
    expect(get(canCreateTemplate)).toBe(true);
    expect(get(canManageGroupInstances)).toBe(false);
    expect(get(canManageDocker)).toBe(false);
    expect(get(canManageRegistry)).toBe(false);
    expect(get(canViewMonitoring)).toBe(false);
    expect(get(isAdmin)).toBe(false);
  });

  it('is_admin bypasses every flag helper', async () => {
    mockApi.get.mockResolvedValue({ data: { context: context({ is_admin: true, tier: 2 }) } });
    await auth.check();

    expect(get(isAdmin)).toBe(true);
    expect(get(canCreateTemplate)).toBe(true);
    expect(get(canManageUsers)).toBe(true);
    expect(get(canManageGroupInstances)).toBe(true);
    expect(get(canManageDocker)).toBe(true);
    expect(get(canManageRegistry)).toBe(true);
    expect(get(canViewMonitoring)).toBe(true);
  });

  it('derives effective_max_instances and the allowed template ids', async () => {
    mockApi.get.mockResolvedValue({
      data: {
        context: context({ effective_max_instances: 6, allowed_template_ids: ['t1', 't3', 't4'] })
      }
    });
    await auth.check();

    expect(get(effectiveMaxInstances)).toBe(6);
    expect(get(allowedTemplateIds)).toEqual(['t1', 't3', 't4']);
  });
});
