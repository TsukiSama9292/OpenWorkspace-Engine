import { describe, it, expect, vi, beforeEach } from 'vitest';
import {
  GROUP_FLAGS,
  buildGroupInput,
  createInitialGroupForm,
  groupFormFromGroup,
  submitGroup,
  submitGroupUpdate
} from '$lib/groups/group-form';
import { createGroup, updateGroup } from '$lib/api/rbac-actions';
import type { Group } from '$lib/types';

vi.mock('$lib/api/rbac-actions', async (importOriginal) => {
  const actual = await importOriginal<typeof import('$lib/api/rbac-actions')>();
  return {
    ...actual,
    createGroup: vi.fn(),
    updateGroup: vi.fn()
  };
});

const mockCreateGroup = vi.mocked(createGroup);
const mockUpdateGroup = vi.mocked(updateGroup);

const group: Group = {
  id: 'g1',
  name: 'Managers',
  description: 'All flags',
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

describe('group form', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  describe('createInitialGroupForm', () => {
    it('returns empty defaults with the five flags off', () => {
      const state = createInitialGroupForm();
      expect(state.name).toBe('');
      expect(state.description).toBe('');
      expect(state.max_instances).toBe('2');
      expect(state.template_ids).toEqual([]);
      for (const flag of GROUP_FLAGS) expect(state[flag]).toBe(false);
    });
  });

  describe('groupFormFromGroup', () => {
    it('maps a Group into editable form state', () => {
      const state = groupFormFromGroup(group);
      expect(state.name).toBe('Managers');
      expect(state.description).toBe('All flags');
      expect(state.max_instances).toBe('2');
      expect(state.template_ids).toEqual(['t1']);
      for (const flag of GROUP_FLAGS) expect(state[flag]).toBe(true);
    });

    it('maps a null max_instances (unlimited) to a blank ceiling', () => {
      const state = groupFormFromGroup({ ...group, max_instances: null });
      expect(state.max_instances).toBe('');
    });
  });

  describe('buildGroupInput', () => {
    it('builds a GroupInput matching the pinned contract shape', () => {
      const state = groupFormFromGroup({ ...group, description: null });
      expect(buildGroupInput(state)).toEqual({
        name: 'Managers',
        description: null,
        can_create_template: true,
        can_manage_users: true,
        can_manage_group_instances: true,
        can_manage_docker: true,
        can_manage_registry: true,
        can_view_monitoring: true,
        max_instances: 2,
        template_ids: ['t1']
      });
    });

    it('reflects flag toggles and the whitelist multi-select in the payload', () => {
      const state = createInitialGroupForm();
      state.can_create_template = true;
      state.can_manage_registry = true;
      state.max_instances = '5';
      state.template_ids = ['t1', 't3'];
      const input = buildGroupInput(state);
      expect(input.can_create_template).toBe(true);
      expect(input.can_manage_users).toBe(false);
      expect(input.can_manage_group_instances).toBe(false);
      expect(input.can_manage_docker).toBe(false);
      expect(input.can_manage_registry).toBe(true);
      expect(input.max_instances).toBe(5);
      expect(input.template_ids).toEqual(['t1', 't3']);
    });

    it('trims the name and normalizes an empty description to null', () => {
      const input = buildGroupInput({ ...createInitialGroupForm(), name: '  Dev Team  ', description: '   ' });
      expect(input.name).toBe('Dev Team');
      expect(input.description).toBeNull();
    });

    it('forces the Admin system groups flags on and the User groups flags off', () => {
      const adminInput = buildGroupInput({
        ...groupFormFromGroup({ ...group, kind: 'admin' }),
        can_manage_docker: false,
        can_manage_registry: false
      });
      expect(adminInput.can_manage_docker).toBe(true);
      expect(adminInput.can_manage_registry).toBe(true);
      expect(adminInput.can_view_monitoring).toBe(true);

      const userInput = buildGroupInput({
        ...groupFormFromGroup({ ...group, kind: 'user' }),
        can_create_template: true,
        can_manage_users: true
      });
      expect(userInput.can_create_template).toBe(false);
      expect(userInput.can_manage_users).toBe(false);
      expect(userInput.can_view_monitoring).toBe(false);
    });

    it('does not force Manager or custom group flags', () => {
      const managerInput = buildGroupInput({
        ...groupFormFromGroup({ ...group, kind: 'manager' }),
        can_manage_docker: false
      });
      expect(managerInput.can_manage_docker).toBe(false);
    });
  });

  describe('submitGroup', () => {
    it('creates a group through rbac-actions with a GroupInput payload', async () => {
      mockCreateGroup.mockResolvedValue({ group: { ...group } });

      const result = await submitGroup(groupFormFromGroup(group));

      expect(result.id).toBe('g1');
      expect(mockCreateGroup).toHaveBeenCalledTimes(1);
      expect(mockCreateGroup).toHaveBeenCalledWith(expect.objectContaining({
        name: 'Managers',
        description: 'All flags',
        can_create_template: true,
        can_manage_docker: true,
        max_instances: 2,
        template_ids: ['t1']
      }));
    });

    it('rejects an empty name without calling the API', async () => {
      const result = await submitGroup(createInitialGroupForm());
      expect(result).toEqual({ error: 'Name is required' });
      expect(mockCreateGroup).not.toHaveBeenCalled();
    });

    it('rejects a negative max_instances without calling the API', async () => {
      const result = await submitGroup({ ...createInitialGroupForm(), name: 'X', max_instances: '-1' });
      expect(result.error).toBeTruthy();
      expect(mockCreateGroup).not.toHaveBeenCalled();
    });

    it('surfaces createGroup errors', async () => {
      mockCreateGroup.mockResolvedValue({ error: 'Forbidden' });
      const result = await submitGroup(groupFormFromGroup(group));
      expect(result).toEqual({ error: 'Forbidden' });
    });
  });

  describe('submitGroupUpdate', () => {
    it('updates a group through rbac-actions with the id and a GroupInput payload', async () => {
      mockUpdateGroup.mockResolvedValue({ group: { ...group } });

      const result = await submitGroupUpdate('g1', groupFormFromGroup({ ...group, max_instances: 8 }));

      expect(result.error).toBeUndefined();
      expect(mockUpdateGroup).toHaveBeenCalledWith('g1', expect.objectContaining({ max_instances: 8 }));
    });

    it('rejects an empty name without calling the API', async () => {
      const result = await submitGroupUpdate('g1', createInitialGroupForm());
      expect(result).toEqual({ error: 'Name is required' });
      expect(mockUpdateGroup).not.toHaveBeenCalled();
    });
  });
});
