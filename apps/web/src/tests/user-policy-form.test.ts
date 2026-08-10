import { describe, it, expect, vi, beforeEach } from 'vitest';
import {
  BLOCKED_CEILING,
  INHERIT_CEILING,
  UNLIMITED_CEILING,
  buildUserPolicyUpdate,
  ceilingFromValue,
  createInitialUserPolicyForm,
  describeDirectMax,
  submitUserPolicy,
  userPolicyFormFromRow,
  valueFromCeiling
} from '$lib/users/user-policy-form';
import { updateUserPolicy } from '$lib/api/rbac-actions';

vi.mock('$lib/api/rbac-actions', async (importOriginal) => {
  const actual = await importOriginal<typeof import('$lib/api/rbac-actions')>();
  return {
    ...actual,
    updateUserPolicy: vi.fn()
  };
});

const mockUpdateUserPolicy = vi.mocked(updateUserPolicy);

describe('user policy form', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  describe('createInitialUserPolicyForm', () => {
    it('starts with no memberships and an inherit ceiling', () => {
      const state = createInitialUserPolicyForm();
      expect(state.group_ids).toEqual([]);
      expect(state.ceiling).toEqual({ mode: 'inherit', value: 1 });
    });
  });

  describe('ceilingFromValue', () => {
    it('maps NULL to inherit', () => {
      expect(ceilingFromValue(null)).toEqual(INHERIT_CEILING);
      expect(ceilingFromValue(undefined)).toEqual(INHERIT_CEILING);
    });

    it('maps -1 to unlimited', () => {
      expect(ceilingFromValue(-1)).toEqual(UNLIMITED_CEILING);
    });

    it('maps 0 to blocked', () => {
      expect(ceilingFromValue(0)).toEqual(BLOCKED_CEILING);
    });

    it('maps a positive number to custom', () => {
      expect(ceilingFromValue(6)).toEqual({ mode: 'custom', value: 6 });
    });
  });

  describe('valueFromCeiling', () => {
    it('round-trips all four modes back to wire values', () => {
      expect(valueFromCeiling(INHERIT_CEILING)).toBeNull();
      expect(valueFromCeiling(UNLIMITED_CEILING)).toBe(-1);
      expect(valueFromCeiling(BLOCKED_CEILING)).toBe(0);
      expect(valueFromCeiling({ mode: 'custom', value: 6 })).toBe(6);
    });
  });

  describe('describeDirectMax', () => {
    it('labels raw wire values for the table row', () => {
      expect(describeDirectMax(null)).toBe('inherit');
      expect(describeDirectMax(-1)).toBe('unlimited');
      expect(describeDirectMax(0)).toBe('blocked');
      expect(describeDirectMax(6)).toBe('6');
    });
  });

  describe('userPolicyFormFromRow', () => {
    it('prefills memberships and ceiling from the user row', () => {
      const state = userPolicyFormFromRow({
        group_ids: ['g1', 'g2'],
        direct_max_instances: 6
      });
      expect(state.group_ids).toEqual(['g1', 'g2']);
      expect(state.ceiling).toEqual({ mode: 'custom', value: 6 });
    });

    it('defaults missing policy fields to empty / inherit', () => {
      const state = userPolicyFormFromRow({});
      expect(state.group_ids).toEqual([]);
      expect(state.ceiling).toEqual(INHERIT_CEILING);
    });
  });

  describe('buildUserPolicyUpdate', () => {
    it('builds a UserPolicyUpdate with memberships and a custom ceiling', () => {
      const update = buildUserPolicyUpdate({
        group_ids: ['g1'],
        ceiling: { mode: 'custom', value: 4 },
        loading: false,
        error: ''
      });
      expect(update).toEqual({ group_ids: ['g1'], direct_max_instances: 4 });
    });

    it('keeps NULL inherit when the mode is inherit', () => {
      const update = buildUserPolicyUpdate({
        group_ids: ['g1'],
        ceiling: { mode: 'inherit', value: 1 },
        loading: false,
        error: ''
      });
      expect(update.direct_max_instances).toBeNull();
    });

    it('emits -1 for unlimited and 0 for blocked', () => {
      const unlimited = buildUserPolicyUpdate({
        group_ids: [],
        ceiling: { mode: 'unlimited', value: 1 },
        loading: false,
        error: ''
      });
      expect(unlimited.direct_max_instances).toBe(-1);

      const blocked = buildUserPolicyUpdate({
        group_ids: [],
        ceiling: { mode: 'disabled', value: 0 },
        loading: false,
        error: ''
      });
      expect(blocked.direct_max_instances).toBe(0);
    });
  });

  describe('submitUserPolicy', () => {
    it('sends the built UserPolicyUpdate through rbac-actions', async () => {
      mockUpdateUserPolicy.mockResolvedValue({});

      const result = await submitUserPolicy('u1', {
        group_ids: ['g1', 'g2'],
        ceiling: { mode: 'custom', value: 6 },
        loading: false,
        error: ''
      });

      expect(result.error).toBeUndefined();
      expect(mockUpdateUserPolicy).toHaveBeenCalledWith('u1', {
        group_ids: ['g1', 'g2'],
        direct_max_instances: 6
      });
    });

    it('rejects a non-positive custom ceiling without calling the API', async () => {
      const result = await submitUserPolicy('u1', {
        group_ids: [],
        ceiling: { mode: 'custom', value: 0 },
        loading: false,
        error: ''
      });
      expect(result.error).toContain('-1');
      expect(mockUpdateUserPolicy).not.toHaveBeenCalled();
    });

    it('rejects a negative custom ceiling without calling the API', async () => {
      const result = await submitUserPolicy('u1', {
        group_ids: [],
        ceiling: { mode: 'custom', value: -2 },
        loading: false,
        error: ''
      });
      expect(result.error).toContain('-1');
      expect(mockUpdateUserPolicy).not.toHaveBeenCalled();
    });

    it('surfaces updateUserPolicy errors', async () => {
      mockUpdateUserPolicy.mockResolvedValue({ error: 'Forbidden' });
      const result = await submitUserPolicy('u1', createInitialUserPolicyForm());
      expect(result).toEqual({ error: 'Forbidden' });
    });

    it('omits group_ids when omitGroupIds is set (admin member)', async () => {
      mockUpdateUserPolicy.mockResolvedValue({});

      const result = await submitUserPolicy(
        'u1',
        {
          group_ids: ['admin-group'],
          ceiling: { mode: 'custom', value: 6 },
          loading: false,
          error: ''
        },
        { omitGroupIds: true }
      );

      expect(result.error).toBeUndefined();
      expect(mockUpdateUserPolicy).toHaveBeenCalledWith('u1', { direct_max_instances: 6 });
    });
  });
});
