import { describe, it, expect, vi, beforeEach } from 'vitest';
import {
  cleanupOrphanedVolume,
  createGroup,
  deleteGroup,
  fetchEffectiveContext,
  listGroups,
  listOrphanedVolumes,
  updateGroup,
  updateUserPolicy
} from '$lib/api/rbac-actions';
import type { EffectiveContext, Group, PersistentVolume } from '$lib/types';

describe('rbac contract client', () => {
  beforeEach(() => {
    vi.restoreAllMocks();
  });

  function stubJson(payload: unknown) {
    const mockFetch = vi.fn().mockResolvedValue({
      ok: true,
      text: () => Promise.resolve(JSON.stringify(payload))
    });
    vi.stubGlobal('fetch', mockFetch);
    return mockFetch;
  }

  function stubEmpty() {
    const mockFetch = vi.fn().mockResolvedValue({
      ok: true,
      text: () => Promise.resolve('')
    });
    vi.stubGlobal('fetch', mockFetch);
    return mockFetch;
  }

  function stubError(status: number, message: string) {
    const mockFetch = vi.fn().mockResolvedValue({
      ok: false,
      status,
      text: () => Promise.resolve(JSON.stringify({ error: message }))
    });
    vi.stubGlobal('fetch', mockFetch);
    return mockFetch;
  }

  const context: EffectiveContext = {
    user_id: 'u1',
    username: 'alice',
    is_admin: false,
    tier: 0,
    can_create_template: true,
    can_manage_users: true,
    can_manage_group_instances: false,
    can_manage_docker: false,
    can_manage_registry: false,
    can_view_monitoring: false,
    effective_max_instances: 4,
    allowed_template_ids: ['t1', 't2'],
    group_ids: ['g1'],
    direct_max_instances: null
  };

  const group: Group = {
    id: 'g1',
    name: 'Managers',
    description: null,
    kind: null,
    can_create_template: true,
    can_manage_users: true,
    can_manage_group_instances: true,
    can_manage_docker: true,
    can_manage_registry: true,
    can_view_monitoring: true,
    max_instances: 2,
    template_ids: ['t1']
  };

  const volume: PersistentVolume = {
    id: 'v1',
    host_path: '/data/openworkspace/tpl-1/u1',
    owner_id: null,
    owner_username: null,
    status: 'orphaned',
    created_at: '2026-08-01T00:00:00Z'
  };

  describe('fetchEffectiveContext', () => {
    it('parses the effective-context payload from /auth/me', async () => {
      const mockFetch = stubJson({ context });

      const result = await fetchEffectiveContext();

      expect(mockFetch).toHaveBeenCalledWith('/api/auth/me', expect.objectContaining({
        method: 'GET'
      }));
      expect(result.context).toEqual(context);
      expect(result.error).toBeUndefined();
    });

    it('surfaces the error when /auth/me fails', async () => {
      stubError(401, 'Unauthorized');

      const result = await fetchEffectiveContext();

      expect(result.error).toBe('Unauthorized');
      expect(result.context).toBeUndefined();
    });
  });

  describe('group CRUD', () => {
    it('lists flat groups from GET /groups', async () => {
      const mockFetch = stubJson({ groups: [group] });

      const result = await listGroups();

      expect(mockFetch).toHaveBeenCalledWith('/api/groups', expect.objectContaining({
        method: 'GET'
      }));
      expect(result.groups).toEqual([group]);
      expect(result.error).toBeUndefined();
    });

    it('creates a group by posting the flat-group input to /groups', async () => {
      const mockFetch = stubJson({ group });
      const input = { ...group, id: undefined } as Omit<Group, 'id' | 'kind'>;

      const result = await createGroup(input);

      expect(mockFetch).toHaveBeenCalledWith('/api/groups', expect.objectContaining({
        method: 'POST',
        body: JSON.stringify(input)
      }));
      expect(result.group).toEqual(group);
      expect(result.error).toBeUndefined();
    });

    it('updates a group with PUT /groups/:id', async () => {
      const mockFetch = stubJson({ group });
      const input = { ...group, max_instances: 5 } as Omit<Group, 'id' | 'kind'>;

      const result = await updateGroup('g1', input);

      expect(mockFetch).toHaveBeenCalledWith('/api/groups/g1', expect.objectContaining({
        method: 'PUT',
        body: JSON.stringify(input)
      }));
      expect(result.group).toEqual(group);
    });

    it('deletes a group with DELETE /groups/:id', async () => {
      const mockFetch = stubEmpty();

      const result = await deleteGroup('g1');

      expect(mockFetch).toHaveBeenCalledWith('/api/groups/g1', expect.objectContaining({
        method: 'DELETE'
      }));
      expect(result.error).toBeUndefined();
    });

    it('surfaces the error when a group create is rejected', async () => {
      stubError(403, 'Forbidden');

      const result = await createGroup({ ...group, id: undefined } as Omit<Group, 'id' | 'kind'>);

      expect(result.error).toBe('Forbidden');
      expect(result.group).toBeUndefined();
    });
  });

  describe('user policy update', () => {
    it('sends memberships and the personal ceiling to PUT /users/:id', async () => {
      const mockFetch = stubEmpty();

      const result = await updateUserPolicy('u1', {
        group_ids: ['g1', 'g2'],
        direct_max_instances: 6
      });

      expect(mockFetch).toHaveBeenCalledWith('/api/users/u1', expect.objectContaining({
        method: 'PUT',
        body: JSON.stringify({
          group_ids: ['g1', 'g2'],
          direct_max_instances: 6
        })
      }));
      expect(result.error).toBeUndefined();
    });

    it('clears the personal ceiling with a null direct_max_instances', async () => {
      const mockFetch = stubEmpty();

      await updateUserPolicy('u1', { direct_max_instances: null });

      const body = JSON.parse(mockFetch.mock.calls[0][1].body as string);
      expect(body).toEqual({ direct_max_instances: null });
    });

    it('surfaces the error when the update is rejected', async () => {
      stubError(403, 'Forbidden');

      const result = await updateUserPolicy('u1', { group_ids: ['g1'] });

      expect(result.error).toBe('Forbidden');
    });
  });

  describe('orphaned volumes', () => {
    it('lists the persistent-volume registry from GET /persistent-volumes', async () => {
      const mockFetch = stubJson({ volumes: [volume] });

      const result = await listOrphanedVolumes();

      expect(mockFetch).toHaveBeenCalledWith('/api/persistent-volumes', expect.objectContaining({
        method: 'GET'
      }));
      expect(result.volumes).toEqual([volume]);
      expect(result.error).toBeUndefined();
    });

    it('triggers thorough cleanup with POST /persistent-volumes/:id/cleanup', async () => {
      const mockFetch = stubEmpty();

      const result = await cleanupOrphanedVolume('v1');

      expect(mockFetch).toHaveBeenCalledWith('/api/persistent-volumes/v1/cleanup', expect.objectContaining({
        method: 'POST'
      }));
      expect(result.error).toBeUndefined();
    });

    it('surfaces the error when cleanup is denied', async () => {
      stubError(403, 'Forbidden');

      const result = await cleanupOrphanedVolume('v1');

      expect(result.error).toBe('Forbidden');
    });
  });
});
