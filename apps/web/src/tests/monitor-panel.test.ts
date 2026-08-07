import { render, screen, fireEvent, waitFor } from '@testing-library/svelte';
import { tick } from 'svelte';
import { describe, it, expect, vi, afterEach } from 'vitest';
import MonitorPanel from '$lib/components/monitor/MonitorPanel.svelte';
import Sparkline from '$lib/components/monitor/Sparkline.svelte';
import type { EffectiveContext, MonitorSnapshot } from '$lib/types';

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
    effective_max_instances: 4,
    allowed_template_ids: [],
    group_ids: [],
    direct_max_instances: null,
    ...overrides
  };
}

const snapshot: MonitorSnapshot = {
  host: {
    cpu_percent: 23.4,
    mem_used_bytes: 8_589_934_592,
    mem_total_bytes: 34_359_738_368,
    disk_used_bytes: 214_748_364_800,
    disk_total_bytes: 1_099_511_627_776,
    cpu_series: [10, 20, 30],
    mem_series: [1000, 2000, 3000],
    disk_series: [5000, 6000, 7000]
  },
  instances: [
    {
      id: 'i1',
      name: 'dev-1',
      owner: 'alice',
      template: 'base-desktop',
      runtime: 'runc',
      status: 'running',
      uptime_secs: 3720,
      cpu_percent: 17.5,
      mem_used_bytes: 1_500_000_000,
      mem_limit_bytes: 4_096_000_000,
      cpu_series: [1, 2, 3],
      mem_series: [10, 20, 30]
    },
    {
      id: 'i2',
      name: 'paused-box',
      owner: 'bob',
      template: 'jupyter',
      runtime: 'runc',
      status: 'paused',
      uptime_secs: null,
      cpu_percent: 0,
      mem_used_bytes: 0,
      mem_limit_bytes: 0,
      cpu_series: [],
      mem_series: []
    }
  ]
};

describe('Sparkline', () => {
  it('renders an svg path for two or more values', () => {
    render(Sparkline, { props: { values: [1, 2, 3] } });
    expect(screen.getByTestId('sparkline')).toBeTruthy();
  });

  it('renders nothing for an empty series', () => {
    render(Sparkline, { props: { values: [] } });
    expect(screen.queryByTestId('sparkline')).toBeNull();
  });
});

async function renderPanel(ctx: EffectiveContext) {
  mockApi.get.mockResolvedValue({ data: snapshot });
  render(MonitorPanel, { props: { ctx } });
  await waitFor(() => {
    expect(screen.getByText('System Monitor')).toBeTruthy();
  });
}

afterEach(() => {
  vi.clearAllMocks();
  vi.useRealTimers();
});

async function expectPolling() {
  await vi.advanceTimersByTimeAsync(5000);
  expect(mockApi.get).toHaveBeenCalledTimes(2);
  await vi.advanceTimersByTimeAsync(5000);
  expect(mockApi.get).toHaveBeenCalledTimes(3);
}

describe('MonitorPanel access', () => {
  it('denies viewers without the can_view_monitoring flag and never calls the API', async () => {
    render(MonitorPanel, { props: { ctx: context() } });

    await waitFor(() => {
      expect(screen.getByText('You do not have permission to view monitoring.')).toBeTruthy();
    });
    expect(mockApi.get).not.toHaveBeenCalled();
  });

  it('allows a can_view_monitoring holder who is not an admin', async () => {
    mockApi.get.mockResolvedValue({ data: snapshot });
    render(MonitorPanel, { props: { ctx: context({ can_view_monitoring: true }) } });

    await waitFor(() => {
      expect(screen.getByText('System Monitor')).toBeTruthy();
      expect(screen.getByText('dev-1')).toBeTruthy();
    });
    expect(mockApi.get).toHaveBeenCalledWith('/monitor/snapshot?range=1h');
  });
});

describe('MonitorPanel table', () => {
  it('renders host cards and active instance rows', async () => {
    await renderPanel(context({ is_admin: true, tier: 2 }));

    expect(screen.getByText('23%')).toBeTruthy();
    expect(screen.getAllByText('8 GB').length).toBeGreaterThan(0);
    expect(screen.getByText('dev-1')).toBeTruthy();
    expect(screen.getByText('base-desktop')).toBeTruthy();
    expect(screen.getByText('alice')).toBeTruthy();
    expect(screen.getByText('18%')).toBeTruthy();
    expect(screen.getByText('1h 2m')).toBeTruthy();
  });

  it('shows the runtime badge and greys out paused instances', async () => {
    await renderPanel(context({ is_admin: true, tier: 2 }));

    expect(screen.getAllByText('runc').length).toBeGreaterThan(0);
    expect(screen.getByText('[paused]')).toBeTruthy();
    const pausedRow = screen.getByText('paused-box').closest('tr');
    expect(pausedRow?.classList.contains('paused')).toBe(true);
  });

  it('sorts the instance table by a clicked column', async () => {
    await renderPanel(context({ is_admin: true, tier: 2 }));

    const rowsBefore = screen.getAllByRole('row');
    expect(rowsBefore[1].textContent).toContain('dev-1');

    await fireEvent.click(screen.getByText('Instance'));
    await tick();

    const rowsAfter = screen.getAllByRole('row');
    expect(rowsAfter[1].textContent).toContain('paused-box');
  });
});

describe('MonitorPanel controls', () => {
  it('switches the series range to 24h and refetches', async () => {
    await renderPanel(context({ is_admin: true, tier: 2 }));

    await fireEvent.click(screen.getByText('24h'));
    await waitFor(() => {
      expect(mockApi.get).toHaveBeenCalledWith('/monitor/snapshot?range=24h');
    });
  });

  it('polls for a fresh snapshot every 5 seconds', async () => {
    vi.useFakeTimers();
    mockApi.get.mockResolvedValue({ data: snapshot });

    render(MonitorPanel, { props: { ctx: context({ is_admin: true, tier: 2 }) } });
    await tick();
    await Promise.resolve();

    expect(mockApi.get).toHaveBeenCalledTimes(1);

    await expectPolling();
  });
});
