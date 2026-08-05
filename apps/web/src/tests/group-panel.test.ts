import { render, screen, fireEvent, waitFor } from '@testing-library/svelte';
import { describe, it, expect, vi, afterEach } from 'vitest';
import GroupPanel from '$lib/components/groups/GroupPanel.svelte';
import type { EffectiveContext, Group, Template } from '$lib/types';

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
    is_admin: false, tier: 0,
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

function template(overrides: Partial<Template> = {}): Template {
  return {
    id: 't1',
    name: 'Dev VM',
    description: '',
    owner_id: 'u1',
    image: 'img:1',
    cores: 2,
    memory: 4294967296,
    gpu_count: 0,
    docker_registry: '',
    remote_type: 'kasmvnc',
    persistent_storage_path: '',
    container_runtime: 'docker',
    max_run_seconds: null,
    timeout_action: 'remove',
    keep_time_seconds: null,
    keep_time_action: 'pause',
    network_bandwidth_up_mbps: 0,
    network_bandwidth_down_mbps: 0,
    docker_in_instance: false,
    visibility: 'private',
    run_config: {},
    exec_config: {},
    volume_mappings: {},
    created_at: '2026-01-01T00:00:00Z',
    updated_at: '2026-01-01T00:00:00Z',
    ...overrides
  };
}

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
  max_instances: 2,
  template_ids: ['t1']
};

describe('GroupPanel', () => {
  afterEach(() => {
    vi.clearAllMocks();
  });

  it('hides group policy entirely from a non-admin', async () => {
    render(GroupPanel, { props: { ctx: context(), templates: [template()] } });

    await waitFor(() => {
      expect(screen.queryByText('Group Management')).toBeNull();
      expect(screen.queryByText('+ New Group')).toBeNull();
      expect(screen.queryByText('Managers')).toBeNull();
    });
    expect(mockApi.get).not.toHaveBeenCalled();
  });

  it('renders a hidden group panel for a can_manage_users holder who is not an admin', async () => {
    render(GroupPanel, {
      props: { ctx: context({ can_manage_users: true }), templates: [template()] }
    });

    await waitFor(() => {
      expect(screen.queryByText('Group Management')).toBeNull();
      expect(screen.queryByText('+ New Group')).toBeNull();
    });
  });

  it('lists groups for an admin', async () => {
    mockApi.get.mockResolvedValue({ data: { groups: [group] } });

    render(GroupPanel, { props: { ctx: context({ is_admin: true, tier: 2 }), templates: [template()] } });

    await waitFor(() => {
      expect(screen.getByText('Group Management')).toBeTruthy();
      expect(screen.getByText('Managers')).toBeTruthy();
      expect(screen.getByText('+ New Group')).toBeTruthy();
    });
    expect(mockApi.get).toHaveBeenCalledWith('/groups');
  });

  it('round-trips a created group with the flags and whitelist as a GroupInput', async () => {
    mockApi.get.mockResolvedValue({ data: { groups: [] } });
    mockApi.post.mockResolvedValue({ data: { group } });

    render(GroupPanel, {
      props: { ctx: context({ is_admin: true, tier: 2 }), templates: [template(), template({ id: 't2', name: 'Jupyter' })] }
    });

    await waitFor(() => {
      expect(screen.getByText('+ New Group')).toBeTruthy();
    });

    await fireEvent.click(screen.getByText('+ New Group'));
    await waitFor(() => {
      expect(screen.getByText('New Group')).toBeTruthy();
    });

    const nameInput = screen.getByLabelText('Name') as HTMLInputElement;
    await fireEvent.input(nameInput, { target: { value: 'Lab' } });

    await fireEvent.click(screen.getByTestId('group-flag-can_create_template'));
    await fireEvent.click(screen.getByTestId('group-flag-can_manage_registry'));
    await fireEvent.click(screen.getByTestId('group-template-t1'));
    await fireEvent.click(screen.getByTestId('group-template-t2'));

    const maxInput = screen.getByLabelText(/Max Instances/) as HTMLInputElement;
    await fireEvent.input(maxInput, { target: { value: '5' } });

    await fireEvent.click(screen.getByText('Create Group'));

    await waitFor(() => {
      const postCall = mockApi.post.mock.calls.find(([path]) => path === '/groups');
      expect(postCall).toBeTruthy();
      const [, body] = postCall as [string, unknown];
      expect(body).toEqual({
        name: 'Lab',
        description: null,
        can_create_template: true,
        can_manage_users: false,
        can_manage_group_instances: false,
        can_manage_docker: false,
        can_manage_registry: true,
        max_instances: 5,
        template_ids: ['t1', 't2']
      });
    });
  });

  it('refreshes the effective context after saving a group', async () => {
    mockApi.get.mockResolvedValue({ data: { groups: [] } });
    mockApi.post.mockResolvedValue({ data: { group } });

    render(GroupPanel, {
      props: { ctx: context({ is_admin: true, tier: 2 }), templates: [template()] }
    });

    await waitFor(() => {
      expect(screen.getByText('+ New Group')).toBeTruthy();
    });

    await fireEvent.click(screen.getByText('+ New Group'));
    await waitFor(() => {
      expect(screen.getByText('New Group')).toBeTruthy();
    });

    await fireEvent.input(screen.getByLabelText('Name'), { target: { value: 'Lab' } });
    await fireEvent.click(screen.getByText('Create Group'));

    await waitFor(() => {
      expect(mockApi.get).toHaveBeenCalledWith('/auth/me');
    });
  });

  it('deletes a group after confirmation', async () => {
    mockApi.get.mockResolvedValue({ data: { groups: [group] } });
    mockApi.delete.mockResolvedValue({ data: null });
    vi.spyOn(window, 'confirm').mockReturnValue(true);

    render(GroupPanel, { props: { ctx: context({ is_admin: true, tier: 2 }), templates: [template()] } });

    await waitFor(() => {
      expect(screen.getByText('Managers')).toBeTruthy();
    });

    await fireEvent.click(screen.getByText('Delete'));

    await waitFor(() => {
      expect(mockApi.delete).toHaveBeenCalledWith('/groups/g1');
    });
    await waitFor(() => {
      expect(screen.queryByText('Managers')).toBeNull();
    });
  });

  it('filters groups by name search', async () => {
    const devs: Group = {
      ...group,
      id: 'g2',
      name: 'Devs',
      description: null,
      can_manage_docker: false
    };
    mockApi.get.mockResolvedValue({ data: { groups: [group, devs] } });

    render(GroupPanel, { props: { ctx: context({ is_admin: true, tier: 2 }), templates: [template()] } });

    await waitFor(() => {
      expect(screen.getByText('Managers')).toBeTruthy();
      expect(screen.getByText('Devs')).toBeTruthy();
    });

    const input = screen.getByPlaceholderText('Search groups...') as HTMLInputElement;
    await fireEvent.input(input, { target: { value: 'dev' } });

    await waitFor(() => {
      expect(screen.queryByText('Managers')).toBeNull();
      expect(screen.getByText('Devs')).toBeTruthy();
    });
  });

  it('filters groups by permission flag', async () => {
    const devs: Group = {
      ...group,
      id: 'g2',
      name: 'Devs',
      description: null,
      can_manage_docker: false
    };
    mockApi.get.mockResolvedValue({ data: { groups: [group, devs] } });

    render(GroupPanel, { props: { ctx: context({ is_admin: true, tier: 2 }), templates: [template()] } });

    await waitFor(() => {
      expect(screen.getByText('Managers')).toBeTruthy();
      expect(screen.getByText('Devs')).toBeTruthy();
    });

    const select = screen.getByLabelText('Filter by permission') as HTMLSelectElement;
    await fireEvent.change(select, { target: { value: 'can_manage_docker' } });

    await waitFor(() => {
      expect(screen.getByText('Managers')).toBeTruthy();
      expect(screen.queryByText('Devs')).toBeNull();
    });
  });
});
