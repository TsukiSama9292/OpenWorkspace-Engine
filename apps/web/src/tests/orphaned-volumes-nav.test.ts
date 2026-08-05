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

describe('orphaned-volumes nav entry', () => {
  beforeEach(async () => {
    await auth.logout();
    mockApi.get.mockReset();
    mockApi.post.mockReset();
  });

  afterEach(async () => {
    await auth.logout();
    vi.clearAllMocks();
  });

  it('shows no Volumes entry for a plain user', async () => {
    stubContext(context());
    await auth.check();

    render(Page);
    await expandSidebar();

    await waitFor(() => {
      expect(screen.queryByText('Volumes')).toBeNull();
    });
    expect(screen.getAllByText('Instances').length).toBeGreaterThan(0);
  });

  it('shows no Volumes entry for a group-instance manager who lacks the volumes permission', async () => {
    stubContext(context({ can_manage_group_instances: true }));
    await auth.check();

    render(Page);
    await expandSidebar();

    await waitFor(() => {
      expect(screen.queryByText('Volumes')).toBeNull();
    });
  });

  it('shows the Volumes entry for a can_manage_users holder', async () => {
    stubContext(context({ can_manage_users: true }));
    await auth.check();

    render(Page);
    await expandSidebar();

    await waitFor(() => {
      expect(screen.getByText('Volumes')).toBeTruthy();
    });
  });

  it('shows the Volumes entry for a system admin', async () => {
    stubContext(context({ is_admin: true, tier: 2 }));
    await auth.check();

    render(Page);
    await expandSidebar();

    await waitFor(() => {
      expect(screen.getByText('Volumes')).toBeTruthy();
    });
  });
});
