import { render, screen, fireEvent, waitFor } from '@testing-library/svelte';
import { tick } from 'svelte';
import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import LogsPanel from '$lib/components/logs/LogsPanel.svelte';
import { formatAuditTime, fullAuditTime } from '$lib/logs/log-helpers';
import type { AuditEntry, EffectiveContext } from '$lib/types';

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
    can_view_monitoring: false,
    can_view_audit_logs: true,
    effective_max_instances: 4,
    allowed_template_ids: [],
    group_ids: [],
    direct_max_instances: null,
    ...overrides
  };
}

function entry(overrides: Partial<AuditEntry> = {}): AuditEntry {
  return {
    id: 'e1',
    created_at: new Date(2026, 7, 8, 15, 14, 30).toISOString(),
    actor_user_id: 'u1',
    actor_name: 'alice',
    action: 'auth.login',
    target_type: 'user',
    target_id: null,
    target_name: null,
    outcome: 'success',
    client_ip: '10.0.0.1',
    detail: null,
    ...overrides
  };
}

function mockMatchMedia(initial: boolean) {
  let listener: ((e: { matches: boolean }) => void) | null = null;
  const mq = {
    matches: initial,
    media: '(max-width: 899px)',
    onchange: null,
    addEventListener: vi.fn(
      (_event: string, cb: (e: { matches: boolean }) => void) => {
        listener = cb;
      }
    ),
    removeEventListener: vi.fn(),
    addListener: vi.fn(),
    removeListener: vi.fn(),
    dispatchEvent: vi.fn()
  };
  vi.stubGlobal('matchMedia', vi.fn().mockReturnValue(mq));
  return {
    emit(matches: boolean) {
      mq.matches = matches;
      listener?.({ matches } as MediaQueryListEvent);
    }
  };
}

describe('LogsPanel — access, filter bar & timestamps', () => {
  beforeEach(() => {
    mockMatchMedia(false);
  });

  afterEach(() => {
    vi.unstubAllGlobals();
    vi.clearAllMocks();
  });

  it('hides the audit surface from a viewer without the audit permission', async () => {
    render(LogsPanel, { props: { ctx: context({ can_view_audit_logs: false }) } });

    await waitFor(() => {
      expect(screen.getByText(/do not have permission/)).toBeTruthy();
    });
    expect(mockApi.get).not.toHaveBeenCalled();
  });

  it('renders the filter fields in a grid plus a separate right-aligned action row', async () => {
    mockApi.get.mockResolvedValue({ data: { entries: [entry()], next_cursor: null } });
    const { container } = render(LogsPanel, { props: { ctx: context() } });

    await waitFor(() => {
      expect(screen.getByText('alice')).toBeTruthy();
    });

    expect(container.querySelector('.filter-grid')).toBeTruthy();
    expect(container.querySelector('.filter-grid select#log-filter-action')).toBeTruthy();
    expect(container.querySelector('.filter-pair')).toBeTruthy();
    expect(container.querySelector('.filter-actions-row .filter-apply')).toBeTruthy();
    expect(container.querySelector('.filter-actions-row .filter-count')?.textContent).toBe('1 entry');
    expect(container.querySelector('.filter-grid .filter-actions-row')).toBeNull();
  });

  it('shows compact timestamps with the full locale string in the title', async () => {
    const e = entry();
    mockApi.get.mockResolvedValue({ data: { entries: [e], next_cursor: null } });
    const { container } = render(LogsPanel, { props: { ctx: context() } });

    await waitFor(() => {
      expect(screen.getByText('alice')).toBeTruthy();
    });

    const timeCell = container.querySelector('tbody tr:first-child td:first-child') as HTMLTableCellElement;
    expect(timeCell.textContent).toBe(formatAuditTime(e.created_at));
    expect(timeCell.getAttribute('title')).toBe(fullAuditTime(e.created_at));
  });
});

describe('LogsPanel — diff rows', () => {
  beforeEach(() => {
    mockMatchMedia(false);
  });

  afterEach(() => {
    vi.unstubAllGlobals();
    vi.clearAllMocks();
  });

  it('toggles the diff with the chevron button and flips aria-expanded', async () => {
    const e = entry({
      id: 'e2',
      action: 'group.update',
      detail: { max_instances: { before: 1, after: 3 } }
    });
    mockApi.get.mockResolvedValue({ data: { entries: [e], next_cursor: null } });
    const { container } = render(LogsPanel, { props: { ctx: context() } });

    await waitFor(() => {
      expect(screen.getByText('Group updated')).toBeTruthy();
    });

    const button = container.querySelector('.diff-toggle') as HTMLButtonElement;
    expect(button).toBeTruthy();
    expect(button.getAttribute('aria-expanded')).toBe('false');

    await fireEvent.click(button);
    await waitFor(() => {
      expect(button.getAttribute('aria-expanded')).toBe('true');
      expect(screen.getByText('max_instances')).toBeTruthy();
      expect(screen.getByText('3')).toBeTruthy();
    });

    await fireEvent.click(button);
    await waitFor(() => {
      expect(button.getAttribute('aria-expanded')).toBe('false');
      expect(screen.queryByText('max_instances')).toBeNull();
    });
  });
});

describe('LogsPanel — responsive layout', () => {
  beforeEach(() => {
    mockMatchMedia(false);
  });

  afterEach(() => {
    vi.unstubAllGlobals();
    vi.clearAllMocks();
  });

  it('applies the ip-hidden class under the narrow viewport rule and lifts it when wide', async () => {
    const mq = mockMatchMedia(true);
    mockApi.get.mockResolvedValue({ data: { entries: [entry()], next_cursor: null } });
    const { container } = render(LogsPanel, { props: { ctx: context() } });

    await waitFor(() => {
      expect(screen.getByText('alice')).toBeTruthy();
    });

    const table = container.querySelector('table.audit-table');
    expect(table?.classList.contains('ip-hidden')).toBe(true);

    mq.emit(false);
    await tick();
    expect(table?.classList.contains('ip-hidden')).toBe(false);

    mq.emit(true);
    await tick();
    expect(table?.classList.contains('ip-hidden')).toBe(true);
  });
});
