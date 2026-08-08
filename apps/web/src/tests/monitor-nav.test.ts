import { render, screen, fireEvent, waitFor } from '@testing-library/svelte';
import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import Page from '../routes/+page.svelte';
import { api } from '$lib/api/client';
import { auth } from '$lib/stores/auth';
import type { EffectiveContext } from '$lib/types';

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
    allowed_template_ids: [],
    group_ids: [],
    direct_max_instances: null,
    ...overrides
  };
}

function stubContext(ctx: EffectiveContext) {
  mockApi.get.mockImplementation((path: string) => {
    if (path === '/auth/me') return Promise.resolve({ data: { context: ctx } });
    if (path === '/templates') return Promise.resolve({ data: { templates: [] } });
    if (path === '/instances') return Promise.resolve({ data: { instances: [] } });
    return Promise.resolve({ data: undefined, error: 'unmocked' });
  });
}

async function expandSidebar() {
  const sidebar = document.querySelector('.sidebar') as HTMLElement;
  await fireEvent.mouseEnter(sidebar);
}

async function renderExpanded(ctx: EffectiveContext) {
  stubContext(ctx);
  await auth.check();
  render(Page);
  await expandSidebar();
}

describe('monitor nav entry', () => {
  beforeEach(async () => {
    await auth.logout();
    mockApi.get.mockReset();
    mockApi.post.mockReset();
  });

  afterEach(async () => {
    await auth.logout();
    vi.clearAllMocks();
  });

  it('shows no Monitor entry for a plain user', async () => {
    renderExpanded(context());
    await waitFor(() => {
      expect(screen.queryByText('Monitor')).toBeNull();
    });
  });

  it('shows no Monitor entry for a permission holder without the monitoring flag', async () => {
    renderExpanded(context({ can_manage_users: true }));
    await waitFor(() => {
      expect(screen.queryByText('Monitor')).toBeNull();
    });
  });

  it('shows the Monitor entry for a can_view_monitoring holder who is not an admin', async () => {
    renderExpanded(context({ can_view_monitoring: true }));
    await waitFor(() => {
      expect(screen.getByText('Monitor')).toBeTruthy();
    });
  });

  it('shows the Monitor entry for a system admin', async () => {
    renderExpanded(context({ is_admin: true, tier: 2 }));
    await waitFor(() => {
      expect(screen.getByText('Monitor')).toBeTruthy();
    });
  });
});
