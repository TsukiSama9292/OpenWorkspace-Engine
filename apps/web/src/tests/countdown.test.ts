import { render, screen } from '@testing-library/svelte';
import { describe, it, expect, vi, afterEach } from 'vitest';
import {
  remainingMs,
  formatRemaining,
  severity,
  wrapperUrl,
  iframeSrc,
  WARNING_THRESHOLD_MS,
  CRITICAL_THRESHOLD_MS
} from '$lib/countdown/countdown';
import CountdownOverlay from '$lib/countdown/CountdownOverlay.svelte';

describe('remainingMs', () => {
  it('returns null when the deadline is absent', () => {
    expect(remainingMs(null, 1000)).toBeNull();
    expect(remainingMs(undefined, 1000)).toBeNull();
  });

  it('returns null for an unparseable deadline', () => {
    expect(remainingMs('not-a-date', 1000)).toBeNull();
  });

  it('computes the difference in ms', () => {
    const deadline = new Date(10_000).toISOString();
    expect(remainingMs(deadline, 4_000)).toBe(6_000);
  });

  it('clamps at zero for past deadlines', () => {
    const deadline = new Date(1_000).toISOString();
    expect(remainingMs(deadline, 4_000)).toBe(0);
  });
});

describe('formatRemaining', () => {
  it('formats minutes and seconds', () => {
    expect(formatRemaining(0)).toBe('00:00');
    expect(formatRemaining(90_000)).toBe('01:30');
    expect(formatRemaining(23 * 60_000 + 45_000)).toBe('23:45');
  });

  it('rounds sub-second remainders up', () => {
    expect(formatRemaining(500)).toBe('00:01');
  });

  it('switches to hours past one hour', () => {
    expect(formatRemaining(3_600_000)).toBe('1:00:00');
    expect(formatRemaining((2 * 3600 + 3 * 60 + 7) * 1000)).toBe('2:03:07');
  });
});

describe('severity', () => {
  it('marks long remainders as normal', () => {
    expect(severity(WARNING_THRESHOLD_MS)).toBe('normal');
    expect(severity(WARNING_THRESHOLD_MS + 1)).toBe('normal');
  });

  it('marks under ten minutes as warning', () => {
    expect(severity(WARNING_THRESHOLD_MS - 1)).toBe('warning');
    expect(severity(CRITICAL_THRESHOLD_MS)).toBe('warning');
  });

  it('marks under a minute as critical', () => {
    expect(severity(CRITICAL_THRESHOLD_MS - 1)).toBe('critical');
    expect(severity(0)).toBe('critical');
  });
});

describe('wrapperUrl', () => {
  it('keeps kasmvnc on its own page', () => {
    expect(wrapperUrl('kasmvnc', 'abc')).toBe('/kasmvnc/abc/');
  });

  it('routes ttyd and jupyter to the wrapper page', () => {
    expect(wrapperUrl('ttyd', 'abc')).toBe('/open/abc/');
    expect(wrapperUrl('jupyter', 'abc')).toBe('/open/abc/');
  });
});

describe('iframeSrc', () => {
  it('embeds ttyd directly', () => {
    expect(iframeSrc('ttyd', 'abc', 'pw')).toBe('/ttyd/abc/');
  });

  it('passes the password token to jupyter lab', () => {
    expect(iframeSrc('jupyter', 'abc', 'a b')).toBe('/jupyter/abc/lab?token=a%20b');
  });
});

describe('CountdownOverlay', () => {
  afterEach(() => {
    vi.restoreAllMocks();
  });

  it('renders nothing without a deadline', () => {
    const { container } = render(CountdownOverlay, { props: {} });
    expect(container.textContent).toBe('');
  });

  it('shows the formatted remaining time and action', () => {
    const deadline = new Date(Date.now() + 23 * 60_000 + 45_000).toISOString();
    render(CountdownOverlay, { props: { deadline, action: 'pause' } });
    expect(screen.getByText('23:45')).toBeTruthy();
    expect(screen.getByText('到期將暫停')).toBeTruthy();
  });

  it('does not paint over the pointer', () => {
    const deadline = new Date(Date.now() + 60_000).toISOString();
    const { container } = render(CountdownOverlay, { props: { deadline, action: 'remove' } });
    expect(container.firstElementChild?.className).toContain('pointer-events-none');
  });

  it('shows the expired state at zero', () => {
    const deadline = new Date(Date.now() - 1_000).toISOString();
    render(CountdownOverlay, { props: { deadline } });
    expect(screen.getByText('已到期')).toBeTruthy();
  });

  it('re-syncs through the page-provided callback', async () => {
    vi.useFakeTimers();
    try {
      const next = { deadline: null, action: null as null };
      const onResync = vi.fn(async () => next);
      const deadline = new Date(Date.now() + 60_000).toISOString();
      const { container, unmount } = render(CountdownOverlay, {
        props: { deadline, onResync }
      });
      expect(container.textContent).toContain('01:00');

      vi.advanceTimersByTime(31_000);
      await vi.runOnlyPendingTimersAsync();
      expect(onResync).toHaveBeenCalled();
      expect(container.textContent).toBe('');
      unmount();
    } finally {
      vi.useRealTimers();
    }
  });

  it('re-syncs immediately when the countdown hits zero', async () => {
    vi.useFakeTimers();
    try {
      const onResync = vi.fn(async () => null);
      const deadline = new Date(Date.now() + 1_000).toISOString();
      const { unmount } = render(CountdownOverlay, { props: { deadline, onResync } });
      expect(onResync).not.toHaveBeenCalled();

      vi.advanceTimersByTime(1_000);
      await vi.runOnlyPendingTimersAsync();
      expect(onResync).toHaveBeenCalled();
      unmount();
    } finally {
      vi.useRealTimers();
    }
  });
});
