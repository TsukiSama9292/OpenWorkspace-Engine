import { describe, it, expect, vi, beforeEach } from 'vitest';
import {
  buildUserPolicyUpdate,
  createInitialUserPolicyForm,
  submitUserPolicy,
  userPolicyFormFromRow
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
      expect(state.direct_max_instances).toBe('');
    });
  });

  describe('userPolicyFormFromRow', () => {
    it('prefills memberships and ceiling from the user row', () => {
      const state = userPolicyFormFromRow({
        group_ids: ['g1', 'g2'],
        direct_max_instances: 6
      });
      expect(state.group_ids).toEqual(['g1', 'g2']);
      expect(state.direct_max_instances).toBe('6');
    });

    it('defaults missing policy fields to empty / inherit', () => {
      const state = userPolicyFormFromRow({});
      expect(state.group_ids).toEqual([]);
      expect(state.direct_max_instances).toBe('');
    });
  });

  describe('buildUserPolicyUpdate', () => {
    it('builds a UserPolicyUpdate with memberships and ceiling', () => {
      const update = buildUserPolicyUpdate({
        group_ids: ['g1'],
        direct_max_instances: '4',
        loading: false,
        error: ''
      });
      expect(update).toEqual({ group_ids: ['g1'], direct_max_instances: 4 });
    });

    it('clears the personal ceiling with null when the field is emptied', () => {
      const update = buildUserPolicyUpdate({
        group_ids: ['g1'],
        direct_max_instances: '',
        loading: false,
        error: ''
      });
      expect(update.direct_max_instances).toBeNull();
    });
  });

  describe('submitUserPolicy', () => {
    it('sends the built UserPolicyUpdate through rbac-actions', async () => {
      mockUpdateUserPolicy.mockResolvedValue({});

      const result = await submitUserPolicy('u1', {
        group_ids: ['g1', 'g2'],
        direct_max_instances: '6',
        loading: false,
        error: ''
      });

      expect(result.error).toBeUndefined();
      expect(mockUpdateUserPolicy).toHaveBeenCalledWith('u1', {
        group_ids: ['g1', 'g2'],
        direct_max_instances: 6
      });
    });

    it('rejects a non-numeric ceiling without calling the API', async () => {
      const result = await submitUserPolicy('u1', {
        group_ids: [],
        direct_max_instances: 'abc',
        loading: false,
        error: ''
      });
      expect(result.error).toBeTruthy();
      expect(mockUpdateUserPolicy).not.toHaveBeenCalled();
    });

    it('rejects a negative ceiling without calling the API', async () => {
      const result = await submitUserPolicy('u1', {
        group_ids: [],
        direct_max_instances: '-2',
        loading: false,
        error: ''
      });
      expect(result.error).toBeTruthy();
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
          direct_max_instances: '6',
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
