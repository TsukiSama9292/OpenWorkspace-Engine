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

const t = 1_700_000_000;

const snapshot: MonitorSnapshot = {
  host: {
    cpu_cores: 4,
    cpu_percent: 23.4,
    mem_used_bytes: 8_589_934_592,
    mem_total_bytes: 34_359_738_368,
    disk_used_bytes: 214_748_364_800,
    disk_total_bytes: 1_099_511_627_776,
    cpu_fine: [
      { t: t + 0, v: 10 },
      { t: t + 1, v: 20 },
      { t: t + 2, v: 30 }
    ],
    cpu_coarse: [],
    mem_fine: [
      { t: t + 0, v: 1000 },
      { t: t + 1, v: 2000 },
      { t: t + 2, v: 3000 }
    ],
    mem_coarse: [],
    disk_fine: [
      { t: t + 0, v: 5000 },
      { t: t + 1, v: 6000 },
      { t: t + 2, v: 7000 }
    ],
    disk_coarse: []
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
      cpu_limit_percent: 200,
      mem_used_bytes: 1_500_000_000,
      mem_limit_bytes: 4_096_000_000,
      cpu_fine: [
        { t: t + 0, v: 1 },
        { t: t + 1, v: 2 },
        { t: t + 2, v: 3 }
      ],
      cpu_coarse: [],
      mem_fine: [
        { t: t + 0, v: 10 },
        { t: t + 1, v: 20 },
        { t: t + 2, v: 30 }
      ],
      mem_coarse: []
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
      cpu_limit_percent: 0,
      mem_used_bytes: 0,
      mem_limit_bytes: 0,
      cpu_fine: [],
      cpu_coarse: [],
      mem_fine: [],
      mem_coarse: []
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

  it('scales points to an explicit domain so magnitude is comparable', () => {
    render(Sparkline, { props: { values: [0, 50, 100], min: 0, max: 100, width: 96, height: 28 } });
    const svg = screen.getByTestId('sparkline');
    const line = svg.querySelector('path[fill="none"]');
    // y = height - (v/domain)*26 - 1: v=0 -> 27, v=50 -> 14, v=100 -> 1.
    expect(line?.getAttribute('d')).toBe('M0.0,27.0 L48.0,14.0 L96.0,1.0');
  });

  it('clamps values outside the domain to the bounds', () => {
    render(Sparkline, { props: { values: [-5, 120], min: 0, max: 100, width: 96, height: 28 } });
    const svg = screen.getByTestId('sparkline');
    const line = svg.querySelector('path[fill="none"]');
    expect(line?.getAttribute('d')).toBe('M0.0,27.0 L96.0,1.0');
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
    expect(mockApi.get).toHaveBeenCalledWith('/monitor/snapshot');
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
    expect(screen.getByText('(37%)')).toBeTruthy();
  });

  it('renders a live interactive chart in each host card', async () => {
    await renderPanel(context({ is_admin: true, tier: 2 }));

    expect(screen.getAllByTestId('host-card').length).toBe(3);
    expect(screen.getAllByTestId('chart-live').length).toBe(3);
  });

  it('shows a used/max fraction and a percentage for a limited CPU instance', async () => {
    await renderPanel(context({ is_admin: true, tier: 2 }));

    // dev-1 runs at 17.5% of one core against a 2-core (200%) ceiling.
    expect(screen.getByText('/ 200%')).toBeTruthy();
    expect(screen.getByText('(9%)')).toBeTruthy();
  });

  it('shows a used/max fraction and a percentage for a limited instance', async () => {
    await renderPanel(context({ is_admin: true, tier: 2 }));

    expect(screen.getByText('1.4 GB')).toBeTruthy();
    expect(screen.getByText('(37%)')).toBeTruthy();
  });

  it('marks unlimited instances without a fake max for both CPU and memory', async () => {
    await renderPanel(context({ is_admin: true, tier: 2 }));

    // paused-box has cpu_limit_percent = mem_limit_bytes = 0: no "/ max" for
    // either, just the muted hint (one per CPU and per memory cell).
    expect(screen.getAllByText('(unlimited)').length).toBe(2);
    expect(screen.queryByText('(0%)')).toBeNull();
    expect(screen.queryByText('/ 0%')).toBeNull();
    expect(screen.queryByText('/ 0 B')).toBeNull();
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

describe('MonitorPanel detail modal', () => {
  it('opens the detail modal with interactive charts when Detail is clicked', async () => {
    await renderPanel(context({ is_admin: true, tier: 2 }));

    await fireEvent.click(screen.getAllByTestId('row-detail')[0]);
    await tick();

    const modal = screen.getByTestId('instance-modal');
    expect(modal).toBeTruthy();
    expect(modal.querySelectorAll('[data-testid="chart-live"]').length).toBe(2);
    expect(modal.textContent).toContain('dev-1');
    expect(modal.textContent).toContain('18%');
    expect(modal.textContent).toContain('/ 200%');
    expect(modal.textContent).toContain('1.4 GB');
    expect(modal.textContent).toContain('/ 3.8 GB');
  });

  it('shows unlimited notes in the modal for an unlimited instance', async () => {
    await renderPanel(context({ is_admin: true, tier: 2 }));

    await fireEvent.click(screen.getAllByTestId('row-detail')[1]);
    await tick();

    const modal = screen.getByTestId('instance-modal');
    expect(modal.textContent).toContain('paused-box');
    expect(modal.querySelectorAll('[data-testid="chart-empty"]').length).toBe(2);
    expect(modal.textContent).toContain('(unlimited)');
  });

  it('closes the detail modal when the overlay is clicked', async () => {
    await renderPanel(context({ is_admin: true, tier: 2 }));

    await fireEvent.click(screen.getAllByTestId('row-detail')[0]);
    await tick();
    expect(screen.getByTestId('instance-modal')).toBeTruthy();

    await fireEvent.click(screen.getByTestId('modal-overlay'));
    await tick();
    expect(screen.queryByTestId('instance-modal')).toBeNull();
  });

  it('closes the detail modal via the close button', async () => {
    await renderPanel(context({ is_admin: true, tier: 2 }));

    await fireEvent.click(screen.getAllByTestId('row-detail')[0]);
    await tick();
    expect(screen.getByTestId('instance-modal')).toBeTruthy();

    await fireEvent.click(screen.getByTestId('modal-close'));
    await tick();
    expect(screen.queryByTestId('instance-modal')).toBeNull();
  });
});

describe('MonitorPanel controls', () => {
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
