import { render, screen, fireEvent, waitFor } from '@testing-library/svelte';
import { describe, it, expect, vi, afterEach } from 'vitest';
import UserManagementPanel from '$lib/components/users/UserManagementPanel.svelte';
import { TIER_MANAGER, type EffectiveContext, type Group } from '$lib/types';

vi.mock('$lib/api/client', () => ({
  api: {
    get: vi.fn(),
    post: vi.fn(),
    put: vi.fn(),
    delete: vi.fn()
  }
}));

import { api } from '$lib/api/client';
const mockApi = vi.mocked(api);

function context(overrides: Partial<EffectiveContext> = {}): EffectiveContext {
  return {
    user_id: 'me',
    username: 'me',
    is_admin: false,
    tier: 0,
    can_create_template: false,
    can_manage_users: false,
    can_manage_group_instances: false,
    can_manage_docker: false,
    can_manage_registry: false,
    effective_max_instances: 4,
    allowed_template_ids: [],
    group_ids: [],
    direct_max_instances: null,
    ...overrides
  };
}

const adminGroup: Group = {
  id: 'g-admin',
  name: 'Admin',
  description: null,
  kind: 'admin',
  can_create_template: true,
  can_manage_users: true,
  can_manage_group_instances: true,
  can_manage_docker: true,
  can_manage_registry: true,
  max_instances: null,
  template_ids: []
};

const managerGroup: Group = {
  id: 'g-manager',
  name: 'Manager',
  description: null,
  kind: 'manager',
  can_create_template: true,
  can_manage_users: true,
  can_manage_group_instances: true,
  can_manage_docker: true,
  can_manage_registry: true,
  max_instances: 2,
  template_ids: []
};

const userGroup: Group = {
  id: 'g-user',
  name: 'User',
  description: null,
  kind: 'user',
  can_create_template: false,
  can_manage_users: false,
  can_manage_group_instances: false,
  can_manage_docker: false,
  can_manage_registry: false,
  max_instances: 1,
  template_ids: []
};

const customGroup: Group = {
  id: 'g1',
  name: 'Devs',
  description: null,
  kind: null,
  can_create_template: true,
  can_manage_users: false,
  can_manage_group_instances: false,
  can_manage_docker: false,
  can_manage_registry: false,
  max_instances: 2,
  template_ids: []
};

const allGroups = [adminGroup, managerGroup, userGroup, customGroup];

const userRow = {
  id: 'u1',
  username: 'alice',
  created_at: '2026-01-01T00:00:00Z',
  group_ids: ['g1'],
  direct_max_instances: 6,
  tier: 0,
  is_admin: false
};

const managerRow = {
  id: 'u2',
  username: 'mallory',
  created_at: '2026-01-01T00:00:00Z',
  group_ids: ['g-manager'],
  direct_max_instances: null,
  tier: 1,
  is_admin: false
};

const adminRow = {
  id: 'u3',
  username: 'root',
  created_at: '2026-01-01T00:00:00Z',
  group_ids: ['g-admin'],
  direct_max_instances: null,
  tier: 2,
  is_admin: true
};

const managerCtx = () => context({ can_manage_users: true, tier: TIER_MANAGER });

describe('UserManagementPanel', () => {
  afterEach(() => {
    vi.clearAllMocks();
  });

  function stubListings(users: unknown[] = [userRow]) {
    mockApi.get.mockImplementation((path: string) =>
      Promise.resolve({
        data: path === '/groups' ? { groups: allGroups } : { users }
      })
    );
  }

  it('shows no membership controls for a non can_manage_users holder', async () => {
    mockApi.get.mockResolvedValue({ data: { users: [userRow] } });

    render(UserManagementPanel, { props: { ctx: context() } });

    await waitFor(() => {
      expect(screen.getByText('alice')).toBeTruthy();
    });
    expect(screen.queryByText('+ New User')).toBeNull();
    expect(screen.queryByText('Edit')).toBeNull();
    expect(screen.queryByTestId('user-policy-groups')).toBeNull();
    expect(screen.queryByLabelText(/Personal Max Instances/)).toBeNull();
  });

  it('hides Edit/Delete on equal-or-higher-tier users for a manager', async () => {
    stubListings([userRow, managerRow, adminRow]);

    render(UserManagementPanel, { props: { ctx: managerCtx() } });

    await waitFor(() => {
      expect(screen.getByText('alice')).toBeTruthy();
      expect(screen.getByText('mallory')).toBeTruthy();
      expect(screen.getByText('root')).toBeTruthy();
    });

    expect(screen.getAllByText('Edit')).toHaveLength(1);
    expect(screen.getAllByText('Delete')).toHaveLength(1);
  });

  it('keeps Edit/Delete on the actors own row (owner-self exception)', async () => {
    stubListings([{ ...managerRow, id: 'me', username: 'manager-self' }]);

    render(UserManagementPanel, { props: { ctx: managerCtx() } });

    await waitFor(() => {
      expect(screen.getByText('manager-self')).toBeTruthy();
    });

    expect(screen.getAllByText('Edit')).toHaveLength(1);
    expect(screen.getAllByText('Delete')).toHaveLength(1);
  });

  it('shows memberships and a tier-filtered policy editor with no personal whitelist', async () => {
    stubListings();

    render(UserManagementPanel, { props: { ctx: managerCtx() } });

    await waitFor(() => {
      expect(screen.getByText('alice')).toBeTruthy();
    });

    expect(screen.getByText('+ New User')).toBeTruthy();

    await fireEvent.click(screen.getByText('Edit'));
    await waitFor(() => {
      expect(screen.getByTestId('user-policy-groups')).toBeTruthy();
      expect(screen.getByTestId('user-policy-group-g-user')).toBeTruthy();
      expect(screen.getByTestId('user-policy-group-g1')).toBeTruthy();
      expect(screen.getByLabelText(/Personal Max Instances/) as HTMLInputElement).toBeTruthy();
    });
    expect(screen.queryByTestId('user-policy-group-g-manager')).toBeNull();
    expect(screen.queryByTestId('user-policy-group-g-admin')).toBeNull();
    expect(screen.queryByText('Personal Template Whitelist')).toBeNull();
  });

  it('saves memberships and ceiling (no whitelist) as a UserPolicyUpdate', async () => {
    stubListings();
    mockApi.put.mockResolvedValue({ data: null });

    render(UserManagementPanel, { props: { ctx: managerCtx() } });

    await waitFor(() => {
      expect(screen.getByText('alice')).toBeTruthy();
    });

    await fireEvent.click(screen.getByText('Edit'));
    await waitFor(() => {
      expect(screen.getByTestId('user-policy-groups')).toBeTruthy();
    });

    const ceiling = screen.getByLabelText(/Personal Max Instances/) as HTMLInputElement;
    await fireEvent.input(ceiling, { target: { value: '' } });

    await fireEvent.click(screen.getByTestId('user-policy-group-g-user'));
    await fireEvent.click(screen.getByText('Save Policy'));

    await waitFor(() => {
      const putCall = mockApi.put.mock.calls.find(([path]) => path === '/users/u1');
      expect(putCall).toBeTruthy();
      const [, body] = putCall as [string, unknown];
      expect(body).toEqual({
        group_ids: ['g1', 'g-user'],
        direct_max_instances: null
      });
    });
  });

  it('creates a new user with a tier-filtered group picker defaulting to the User group', async () => {
    stubListings([]);
    mockApi.post.mockResolvedValue({
      data: { user: { id: 'u4', username: 'bob', created_at: '2026-01-01T00:00:00Z', tier: 0, is_admin: false } }
    });

    render(UserManagementPanel, { props: { ctx: managerCtx() } });

    await waitFor(() => {
      expect(screen.getByText('+ New User')).toBeTruthy();
    });

    await fireEvent.click(screen.getByText('+ New User'));
    await waitFor(() => {
      expect(screen.getByText('Create User')).toBeTruthy();
    });

    expect(screen.getByTestId('create-user-group-g-user')).toBeTruthy();
    expect(screen.getByTestId('create-user-group-g1')).toBeTruthy();
    expect(screen.queryByTestId('create-user-group-g-manager')).toBeNull();
    expect(screen.queryByTestId('create-user-group-g-admin')).toBeNull();

    const userGroupBox = screen.getByTestId('create-user-group-g-user') as HTMLInputElement;
    expect(userGroupBox.checked).toBe(true);
    expect(userGroupBox.disabled).toBe(true);

    await fireEvent.click(screen.getByTestId('create-user-group-g1'));

    const username = screen.getByLabelText('Username') as HTMLInputElement;
    await fireEvent.input(username, { target: { value: 'bob' } });
    const password = screen.getByLabelText('Password') as HTMLInputElement;
    await fireEvent.input(password, { target: { value: 'secret' } });

    await fireEvent.click(screen.getByText('Create'));

    await waitFor(() => {
      const postCall = mockApi.post.mock.calls.find(([path]) => path === '/users');
      expect(postCall).toBeTruthy();
      const [, body] = postCall as [string, unknown];
      expect(body).toEqual({ username: 'bob', password: 'secret', group_ids: ['g-user', 'g1'] });
    });
  });

  it('searches users by username', async () => {
    stubListings([userRow, { ...userRow, id: 'u2', username: 'bob' }]);

    render(UserManagementPanel, { props: { ctx: managerCtx() } });

    await waitFor(() => {
      expect(screen.getByText('alice')).toBeTruthy();
      expect(screen.getByText('bob')).toBeTruthy();
    });

    const input = screen.getByPlaceholderText('Search users...') as HTMLInputElement;
    await fireEvent.input(input, { target: { value: 'ali' } });

    await waitFor(() => {
      expect(screen.getByText('alice')).toBeTruthy();
      expect(screen.queryByText('bob')).toBeNull();
    });
  });

  it('filters users by group membership', async () => {
    stubListings([userRow, { ...userRow, id: 'u2', username: 'bob', group_ids: [] }]);

    render(UserManagementPanel, { props: { ctx: managerCtx() } });

    await waitFor(() => {
      expect(screen.getByText('alice')).toBeTruthy();
      expect(screen.getByText('bob')).toBeTruthy();
    });

    const select = screen.getByLabelText('Filter by group') as HTMLSelectElement;
    await fireEvent.change(select, { target: { value: 'g1' } });

    await waitFor(() => {
      expect(screen.getByText('alice')).toBeTruthy();
      expect(screen.queryByText('bob')).toBeNull();
    });
  });

  it('filters users by ceiling type', async () => {
    stubListings([userRow, { ...userRow, id: 'u2', username: 'bob', direct_max_instances: null }]);

    render(UserManagementPanel, { props: { ctx: managerCtx() } });

    await waitFor(() => {
      expect(screen.getByText('alice')).toBeTruthy();
      expect(screen.getByText('bob')).toBeTruthy();
    });

    const select = screen.getByLabelText('Filter by ceiling') as HTMLSelectElement;
    await fireEvent.change(select, { target: { value: 'inherit' } });

    await waitFor(() => {
      expect(screen.queryByText('alice')).toBeNull();
      expect(screen.getByText('bob')).toBeTruthy();
    });
  });
});
