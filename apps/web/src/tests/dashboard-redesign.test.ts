import { render, screen, waitFor, fireEvent, within } from '@testing-library/svelte';
import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import Page from '../routes/+page.svelte';
import { api } from '$lib/api/client';
import { auth } from '$lib/stores/auth';
import type { EffectiveContext, Instance, Template } from '$lib/types';

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
    allowed_template_ids: ['t1'],
    group_ids: [],
    direct_max_instances: null,
    ...overrides
  };
}

function template(overrides: Partial<Template> = {}): Template {
  return {
    id: 't1',
    name: 'Ubuntu dev',
    description: 'General purpose desktop',
    owner_id: 'admin',
    image: 'ow-ubuntu:jammy',
    cores: 2,
    memory: 4294967296,
    gpu_count: 1,
    docker_registry: '',
    remote_type: 'kasmvnc',
    persistent_storage_path: '',
    container_runtime: 'runc',
    max_run_seconds: -1,
    timeout_action: 'remove',
    keep_time_seconds: -1,
    keep_time_action: 'pause',
    network_bandwidth_up_mbps: 0,
    network_bandwidth_down_mbps: 0,
    docker_in_instance: false,
    visibility: 'public',
    run_config: {},
    exec_config: {},
    volume_mappings: {},
    created_at: '2026-01-01T00:00:00Z',
    updated_at: '2026-01-01T00:00:00Z',
    ...overrides
  };
}

function session(overrides: Partial<Instance> = {}): Instance {
  return {
    id: 's1',
    name: 'alice-box',
    template_id: 't1',
    template_name: 'Ubuntu dev',
    remote_type: 'kasmvnc',
    owner_id: 'me',
    owner_username: 'alice',
    owner_group_ids: [],
    owner_tier: 0,
    status: 'running',
    instance_number: 1,
    container_id: 'c1',
    mount_persistent: false,
    access_token: 'tok',
    created_at: '2026-01-01T00:00:00Z',
    updated_at: '2026-01-01T00:00:00Z',
    ...overrides
  };
}

function stubDashboard(configs: Template[], instances: Instance[], ctx: EffectiveContext) {
  mockApi.get.mockImplementation((path: string) => {
    if (path === '/auth/me') return Promise.resolve({ data: { context: ctx } });
    if (path === '/templates') return Promise.resolve({ data: { templates: configs } });
    if (path === '/instances') return Promise.resolve({ data: { instances } });
    return Promise.resolve({ data: undefined });
  });
}

describe('dashboard redesign', () => {
  beforeEach(async () => {
    await auth.logout();
    mockApi.get.mockReset();
    localStorage.clear();
  });

  afterEach(async () => {
    await auth.logout();
    vi.clearAllMocks();
  });

  it('shows a truthful identity and a running count in the rail', async () => {
    stubDashboard([template()], [session()], context());
    await auth.check();
    render(Page);
    await waitFor(() => expect(screen.getByText('alice')).toBeTruthy());
    expect(screen.getByText('Tier 0')).toBeTruthy();
    expect(screen.getByLabelText('Instances, 1 running')).toBeTruthy();
  });

  it('serves catalog marks as local vectors with no emoji', async () => {
    stubDashboard([template()], [], context());
    await auth.check();
    const { container } = render(Page);
    await waitFor(() => expect(screen.getByText('Template catalog')).toBeTruthy());
    const marks = container.querySelectorAll('img.catalog-mark');
    expect(marks.length).toBe(1);
    expect(marks[0].getAttribute('src')).toBe('/icons/ubuntu.svg');
    expect(document.body.textContent).not.toMatch(/🐧|🐍|🧠|⚙|📦/);
  });

  it('filters sessions and templates through the hero search', async () => {
    stubDashboard(
      [template({ id: 't1', name: 'Ubuntu dev' }), template({ id: 't2', name: 'Rust box', image: 'ow-rust:jammy' })],
      [session({ id: 's1', name: 'alice-box' })],
      context({ allowed_template_ids: ['t1', 't2'] })
    );
    await auth.check();
    render(Page);
    await waitFor(() => expect(screen.getByText('Rust box')).toBeTruthy());
    const search = screen.getByLabelText('Search sessions and templates');
    await fireEvent.input(search, { target: { value: 'rust' } });
    await waitFor(() => expect(screen.queryByText('Ubuntu dev')).toBeNull());
    expect(screen.getByText('Rust box')).toBeTruthy();
  });

  it('emphasizes Open and tucks secondary actions into a More menu', async () => {
    stubDashboard([template()], [session({ status: 'running', access_token: 'tok' })], context());
    await auth.check();
    const { container } = render(Page);
    await waitFor(() => expect(screen.getByText('alice-box')).toBeTruthy());
    expect(screen.getByText('Open')).toBeTruthy();
    const more = screen.getByRole('button', { name: /More/ });
    expect(more.getAttribute('aria-haspopup')).toBe('menu');
    expect(container.querySelector('[role="menu"]')).toBeNull();
    await fireEvent.click(more);
    await waitFor(() => expect(container.querySelector('[role="menu"]')).toBeTruthy());
    expect(screen.getByRole('menuitem', { name: 'Remove' })).toBeTruthy();
  });

  it('shows the session state as legible text with no fake progress', async () => {
    stubDashboard([template()], [session({ status: 'running', access_token: 'tok' })], context());
    await auth.check();
    const { container } = render(Page);
    await waitFor(() => expect(screen.getByText('alice-box')).toBeTruthy());
    expect(screen.getByText('running')).toBeTruthy();
    expect(container.querySelector('[role="progressbar"]')).toBeNull();
  });

  it('shows the budget story with an honest meter when the window is known', async () => {
    const tpl = template({ max_run_seconds: 3600, timeout_action: 'stop' });
    const inst = session({
      status: 'running',
      access_token: 'tok',
      auto_sleeps_at: new Date(Date.now() + 1_800_000).toISOString(),
      timeout_action: 'stop'
    });
    stubDashboard([tpl], [inst], context());
    await auth.check();
    const { container } = render(Page);
    await waitFor(() => expect(screen.getByText('alice-box')).toBeTruthy());
    expect(screen.getByText('Auto-sleep in 30:00')).toBeTruthy();
    const meter = container.querySelector('[role="progressbar"]');
    expect(meter?.getAttribute('aria-label')).toBe('Auto-sleep in 30:00');
    expect(meter?.getAttribute('aria-valuenow')).toBe('50');
  });

  it('shows the story with no meter when the window is unlimited', async () => {
    const tpl = template({ max_run_seconds: -1, timeout_action: 'stop' });
    const inst = session({
      status: 'running',
      access_token: 'tok',
      auto_sleeps_at: new Date(Date.now() + 1_800_000).toISOString(),
      timeout_action: 'stop'
    });
    stubDashboard([tpl], [inst], context());
    await auth.check();
    const { container } = render(Page);
    await waitFor(() => expect(screen.getByText('alice-box')).toBeTruthy());
    expect(screen.getByText('Auto-sleep in 30:00')).toBeTruthy();
    expect(container.querySelector('[role="progressbar"]')).toBeNull();
  });

  it('asks for confirmation before removing a session', async () => {
    stubDashboard([template()], [session({ status: 'running', access_token: 'tok' })], context());
    mockApi.delete.mockResolvedValue({});
    await auth.check();
    render(Page);
    await waitFor(() => expect(screen.getByText('alice-box')).toBeTruthy());
    await fireEvent.click(screen.getByRole('button', { name: /More/ }));
    await fireEvent.click(await screen.findByRole('menuitem', { name: 'Remove' }));
    expect(screen.getByText('Delete "alice-box"?')).toBeTruthy();
    expect(mockApi.delete).not.toHaveBeenCalled();
    await fireEvent.click(screen.getByRole('button', { name: 'Delete' }));
    await waitFor(() => expect(mockApi.delete).toHaveBeenCalledWith('/instances/s1'));
    await waitFor(() => expect(screen.queryByText('alice-box')).toBeNull());
  });

  it('offers a clear-search action inside the empty state', async () => {
    stubDashboard([template()], [session({ status: 'running', access_token: 'tok' })], context());
    await auth.check();
    render(Page);
    await waitFor(() => expect(screen.getByText('alice-box')).toBeTruthy());
    const search = screen.getByLabelText('Search sessions and templates');
    await fireEvent.input(search, { target: { value: 'zzz-no-match' } });
    await waitFor(() => expect(screen.getByText(/No sessions match the current search/)).toBeTruthy());
    await fireEvent.click(screen.getByRole('button', { name: 'Clear search' }));
    await waitFor(() => expect(screen.getByText('alice-box')).toBeTruthy());
  });

  it('moves focus into the More menu on open', async () => {
    stubDashboard([template()], [session({ status: 'running', access_token: 'tok' })], context());
    await auth.check();
    render(Page);
    await waitFor(() => expect(screen.getByText('alice-box')).toBeTruthy());
    await fireEvent.click(screen.getByRole('button', { name: /More/ }));
    await waitFor(() => expect(screen.getByRole('menuitem', { name: 'Pause' })).toBeTruthy());
    expect(document.activeElement).toBe(screen.getByRole('menuitem', { name: 'Pause' }));
  });

  it('closes the More menu on Escape and returns focus to its button', async () => {
    stubDashboard([template()], [session({ status: 'running', access_token: 'tok' })], context());
    await auth.check();
    const { container } = render(Page);
    await waitFor(() => expect(screen.getByText('alice-box')).toBeTruthy());
    const more = screen.getByRole('button', { name: /More/ });
    await fireEvent.click(more);
    await waitFor(() => expect(container.querySelector('[role="menu"]')).toBeTruthy());
    await fireEvent.keyDown(container.querySelector('[role="menu"]')!, { key: 'Escape' });
    await waitFor(() => expect(container.querySelector('[role="menu"]')).toBeNull());
    expect(document.activeElement).toBe(more);
  });

  it('shows quota usage in the hero', async () => {
    stubDashboard([template()], [session()], context({ effective_max_instances: 4 }));
    await auth.check();
    render(Page);
    await waitFor(() => expect(screen.getByText(/Quota: 1\/4 sessions used/)).toBeTruthy());
  });

  it('explains locked templates with their actual cause', async () => {
    stubDashboard(
      [template({ id: 't2', name: 'Secret', visibility: 'private' })],
      [],
      context({ allowed_template_ids: [] })
    );
    await auth.check();
    render(Page);
    await waitFor(() => expect(screen.getByText('Secret')).toBeTruthy());
    expect(screen.getByText(/outside your whitelist/)).toBeTruthy();
  });

  it('leads the sessions table with an Open action for running instances', async () => {
    window.location.hash = '#sessions';
    try {
      stubDashboard(
        [template()],
        [session({ status: 'running', access_token: 'tok', owner_username: 'alice' })],
        context({ is_admin: true, tier: 2 })
      );
      await auth.check();
      render(Page);
      const open = await screen.findByRole('link', { name: 'Open' });
      expect(open.getAttribute('href')).toBe('/kasmvnc/tok/');
      const table = screen.getByRole('table');
      expect(within(table).getByText('alice')).toBeTruthy();
      expect(within(table).getByText('Budget')).toBeTruthy();
    } finally {
      window.location.hash = '';
    }
  });

  it('asks for confirmation before deleting a template', async () => {
    window.location.hash = '#templates';
    try {
      stubDashboard(
        [template({ id: 't1', name: 'Old box', owner_id: 'me' })],
        [],
        context({ user_id: 'me', can_create_template: true, allowed_template_ids: ['t1'] })
      );
      mockApi.delete.mockResolvedValue({});
      await auth.check();
      render(Page);
      await waitFor(() => expect(screen.getByText('Old box')).toBeTruthy());
      await fireEvent.click(screen.getByRole('button', { name: 'Delete' }));
      expect(screen.getByText('Delete template "Old box"?')).toBeTruthy();
      expect(mockApi.delete).not.toHaveBeenCalled();
      const templateDialog = screen.getByRole('dialog');
      await fireEvent.click(within(templateDialog).getByRole('button', { name: 'Delete' }));
      await waitFor(() => expect(mockApi.delete).toHaveBeenCalledWith('/templates/t1'));
      await waitFor(() => expect(screen.queryByText('Old box')).toBeNull());
    } finally {
      window.location.hash = '';
    }
  });
});
