import { render, screen, waitFor } from '@testing-library/svelte';
import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import Page from '../routes/+page.svelte';
import { api } from '$lib/api/client';
import { auth } from '$lib/stores/auth';
import type { EffectiveContext, Template } from '$lib/types';

vi.mock('$lib/api/client', () => ({
  api: {
    get: vi.fn(),
    post: vi.fn(),
    put: vi.fn(),
    delete: vi.fn()
  }
}));

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
    can_view_monitoring: false,
    can_view_audit_logs: false,
    effective_max_instances: 4,
    allowed_template_ids: ['t-own'],
    group_ids: [],
    direct_max_instances: null,
    ...overrides
  };
}

function template(overrides: Partial<Template> = {}): Template {
  return {
    id: 't1',
    name: 'Tpl',
    description: '',
    owner_id: 'someone-else',
    image: 'img:1',
    cores: 2,
    memory: 4294967296,
    gpu_count: 0,
    docker_registry: '',
    remote_type: 'kasmvnc',
    persistent_storage_path: '',
    container_runtime: 'runc',
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

function stubTemplates(configs: Template[]) {
  mockApi.get.mockImplementation((path: string) => {
    if (path === '/auth/me') return Promise.resolve({ data: { context: context() } });
    if (path === '/templates') return Promise.resolve({ data: { templates: configs } });
    if (path === '/instances') return Promise.resolve({ data: { instances: [] } });
    return Promise.resolve({ data: undefined, error: 'unmocked' });
  });
}

describe('quick launch grid', () => {
  beforeEach(async () => {
    await auth.logout();
    mockApi.get.mockReset();
  });

  afterEach(async () => {
    await auth.logout();
    vi.clearAllMocks();
  });

  it('hides hidden templates from the launch grid', async () => {
    stubTemplates([
      template({ id: 't-own', name: 'Private', visibility: 'private' }),
      template({ id: 't-pub', name: 'Public', visibility: 'public' }),
      template({ id: 't-hidden', name: 'HiddenOne', visibility: 'hidden' })
    ]);
    await auth.check();

    render(Page);

    await waitFor(() => {
      expect(screen.getByText('Private')).toBeTruthy();
    });
    expect(screen.getByText('Public')).toBeTruthy();
    expect(screen.queryByText('HiddenOne')).toBeNull();
  });
});
