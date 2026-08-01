import { render, screen } from '@testing-library/svelte';
import { describe, it, expect, vi, afterEach } from 'vitest';
import { tick } from 'svelte';
import {
  remainingMs,
  deadlineRemaining,
  selectDeadline,
  formatRemaining,
  severity,
  wrapperUrl,
  iframeSrc,
  WARNING_THRESHOLD_MS,
  CRITICAL_THRESHOLD_MS
} from '$lib/countdown/countdown';
import CountdownOverlay from '$lib/countdown/CountdownOverlay.svelte';
import type { TimeoutAction } from '$lib/types';

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

describe('deadlineRemaining (keep_time_deadline)', () => {
  it('computes the remaining time from a valid keep_time_deadline', () => {
    const deadline = new Date(10_000).toISOString();
    expect(deadlineRemaining(deadline, 4_000)).toBe(6_000);
  });

  it('returns null when keep_time_deadline is absent or null', () => {
    expect(deadlineRemaining(null, 4_000)).toBeNull();
    expect(deadlineRemaining(undefined, 4_000)).toBeNull();
  });

  it('returns null for a malformed keep_time_deadline', () => {
    expect(deadlineRemaining('not-a-date', 4_000)).toBeNull();
  });
});

describe('selectDeadline', () => {
  const iso = (ms: number) => new Date(ms).toISOString();

  it('returns the auto-sleep deadline when only auto-sleep exists', () => {
    const d = iso(50_000);
    expect(selectDeadline(d, 'remove', null, null)).toEqual({ deadline: d, action: 'remove' });
    expect(selectDeadline(d, 'remove', undefined, undefined)).toEqual({
      deadline: d,
      action: 'remove'
    });
  });

  it('returns the keep-time deadline when only keep-time exists', () => {
    const d = iso(50_000);
    expect(selectDeadline(null, null, d, 'pause')).toEqual({ deadline: d, action: 'pause' });
    expect(selectDeadline(undefined, undefined, d, undefined)).toEqual({
      deadline: d,
      action: null
    });
  });

  it('prefers the keep-time deadline when it is earlier', () => {
    const auto = iso(100_000);
    const keep = iso(50_000);
    expect(selectDeadline(auto, 'remove', keep, 'pause')).toEqual({
      deadline: keep,
      action: 'pause'
    });
  });

  it('prefers the auto-sleep deadline when it is earlier', () => {
    const auto = iso(50_000);
    const keep = iso(100_000);
    expect(selectDeadline(auto, 'remove', keep, 'pause')).toEqual({
      deadline: auto,
      action: 'remove'
    });
  });

  it('prefers the auto-sleep deadline on a tie', () => {
    const d = iso(50_000);
    expect(selectDeadline(d, 'remove', d, 'pause')).toEqual({ deadline: d, action: 'remove' });
  });

  it('returns null when neither deadline exists', () => {
    expect(selectDeadline(null, null, null, null)).toBeNull();
    expect(selectDeadline(undefined, undefined, undefined, undefined)).toBeNull();
  });

  it('ignores a malformed keep_time_deadline', () => {
    const auto = iso(50_000);
    expect(selectDeadline(auto, 'remove', 'not-a-date', 'pause')).toEqual({
      deadline: auto,
      action: 'remove'
    });
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
  function setVisibility(state: DocumentVisibilityState) {
    Object.defineProperty(document, 'visibilityState', {
      value: state,
      configurable: true
    });
  }

  function setHasFocus(value: boolean) {
    vi.spyOn(document, 'hasFocus').mockReturnValue(value);
  }

  function setIframeFocused(focused: boolean) {
    const innerHasFocus = vi.fn(() => focused);
    const fakeIframe = {
      contentWindow: {
        document: { hasFocus: innerHasFocus }
      }
    };
    vi.spyOn(document, 'querySelectorAll').mockReturnValue(
      [fakeIframe] as unknown as NodeListOf<HTMLIFrameElement>
    );
    return innerHasFocus;
  }

  afterEach(() => {
    vi.restoreAllMocks();
    delete (document as { visibilityState?: unknown }).visibilityState;
  });

  it('renders nothing without a deadline', () => {
    const { container } = render(CountdownOverlay, { props: {} });
    expect(container.textContent).toBe('');
  });

  it('shows the formatted remaining time and action', () => {
    const deadline = new Date(Date.now() + 23 * 60_000 + 45_000).toISOString();
    render(CountdownOverlay, {
      props: { auto_sleeps_at: deadline, timeout_action: 'pause' }
    });
    expect(screen.getByText('23:45')).toBeTruthy();
    expect(screen.getByText('Pause on expiry')).toBeTruthy();
  });

  it('does not paint over the pointer', () => {
    const deadline = new Date(Date.now() + 60_000).toISOString();
    const { container } = render(CountdownOverlay, {
      props: { auto_sleeps_at: deadline, timeout_action: 'remove' }
    });
    expect(container.firstElementChild?.className).toContain('pointer-events-none');
  });

  it('shows the expired state at zero', () => {
    const deadline = new Date(Date.now() - 1_000).toISOString();
    render(CountdownOverlay, { props: { auto_sleeps_at: deadline } });
    expect(screen.getByText('Expired')).toBeTruthy();
  });

  it('hides the badge while the tab is focused and shows it once blurred', async () => {
    setVisibility('visible');
    setHasFocus(false);
    const deadline = new Date(Date.now() + 60_000).toISOString();
    const { container, unmount } = render(CountdownOverlay, {
      props: { auto_sleeps_at: deadline, timeout_action: 'stop' }
    });
    expect(container.textContent).toContain('01:00');
    expect(screen.getByText('Stop on expiry')).toBeTruthy();

    setHasFocus(true);
    window.dispatchEvent(new Event('focus'));
    await tick();
    expect(container.textContent).toBe('');

    setHasFocus(false);
    window.dispatchEvent(new Event('blur'));
    await tick();
    expect(container.textContent).toContain('01:00');
    unmount();
  });

  it('shows the keep-time badge only while visible and not focused', () => {
    setVisibility('visible');
    setHasFocus(false);
    const deadline = new Date(Date.now() + 60_000).toISOString();
    const { container, unmount } = render(CountdownOverlay, {
      props: { keep_time_deadline: deadline, keep_time_action: 'pause' }
    });
    expect(container.textContent).toContain('01:00');
    expect(screen.getByText('Pause on expiry')).toBeTruthy();
    unmount();
  });

  it('hides the keep-time badge while the tab is focused', async () => {
    setVisibility('visible');
    setHasFocus(false);
    const deadline = new Date(Date.now() + 60_000).toISOString();
    const { container, unmount } = render(CountdownOverlay, {
      props: { keep_time_deadline: deadline, keep_time_action: 'pause' }
    });
    expect(container.textContent).toContain('01:00');

    setHasFocus(true);
    window.dispatchEvent(new Event('focus'));
    await tick();
    expect(container.textContent).toBe('');

    setHasFocus(false);
    window.dispatchEvent(new Event('blur'));
    await tick();
    expect(container.textContent).toContain('01:00');
    unmount();
  });

  it('hides the keep-time badge while the embedded iframe document has focus', () => {
    setVisibility('visible');
    setHasFocus(false);
    setIframeFocused(true);
    const deadline = new Date(Date.now() + 60_000).toISOString();
    const { container, unmount } = render(CountdownOverlay, {
      props: { keep_time_deadline: deadline, keep_time_action: 'pause' }
    });
    expect(container.textContent).toBe('');
    unmount();
  });

  it('shows the keep-time badge once focus leaves the iframe', async () => {
    setVisibility('visible');
    setHasFocus(false);
    const innerHasFocus = setIframeFocused(true);
    const deadline = new Date(Date.now() + 60_000).toISOString();
    const { container, unmount } = render(CountdownOverlay, {
      props: { keep_time_deadline: deadline, keep_time_action: 'pause' }
    });
    expect(container.textContent).toBe('');

    innerHasFocus.mockReturnValue(false);
    document.dispatchEvent(new Event('focusout'));
    await tick();
    expect(container.textContent).toContain('01:00');
    unmount();
  });

  it('does not render the keep-time badge while the tab is hidden', () => {
    setVisibility('hidden');
    setHasFocus(false);
    const deadline = new Date(Date.now() + 60_000).toISOString();
    const { container, unmount } = render(CountdownOverlay, {
      props: { keep_time_deadline: deadline, keep_time_action: 'pause' }
    });
    expect(container.textContent).toBe('');
    unmount();
  });

  it('counts down from the last heartbeat when the server deadline is stale', () => {
    setVisibility('visible');
    setHasFocus(false);
    const now = Date.now();
    const { container, unmount } = render(CountdownOverlay, {
      props: {
        keep_time_deadline: new Date(now - 1_000).toISOString(),
        keep_time_action: 'pause',
        keep_time_seconds: 60,
        last_heartbeat_at: now
      }
    });
    expect(container.textContent).toContain('01:00');
    expect(screen.queryByText('Expired')).toBeNull();
    unmount();
  });

  it('uses the later of the server and heartbeat deadlines', () => {
    setVisibility('visible');
    setHasFocus(false);
    const now = Date.now();
    const { container, unmount } = render(CountdownOverlay, {
      props: {
        keep_time_deadline: new Date(now + 2 * 60_000).toISOString(),
        keep_time_action: 'pause',
        keep_time_seconds: 60,
        last_heartbeat_at: now
      }
    });
    expect(container.textContent).toContain('02:00');
    unmount();
  });

  it('falls back to the server deadline without a heartbeat timestamp', () => {
    setVisibility('visible');
    setHasFocus(false);
    const deadline = new Date(Date.now() + 60_000).toISOString();
    const { container, unmount } = render(CountdownOverlay, {
      props: { keep_time_deadline: deadline, keep_time_action: 'pause', keep_time_seconds: 60 }
    });
    expect(container.textContent).toContain('01:00');
    unmount();
  });

  it('re-syncs the deadline when focus is lost', async () => {
    setVisibility('visible');
    setHasFocus(true);
    const onResync = vi.fn(async () => ({
      auto_sleeps_at: null,
      timeout_action: null,
      keep_time_deadline: new Date(Date.now() + 60_000).toISOString(),
      keep_time_action: 'pause' as TimeoutAction
    }));
    const { container, unmount } = render(CountdownOverlay, {
      props: { onResync }
    });
    expect(onResync).not.toHaveBeenCalled();

    setHasFocus(false);
    window.dispatchEvent(new Event('blur'));
    await new Promise(r => setTimeout(r, 0));
    await tick();
    expect(onResync).toHaveBeenCalledTimes(1);
    expect(container.textContent).toContain('01:00');
    unmount();
  });

  it('re-syncs the deadline again when focus is regained', async () => {
    setVisibility('visible');
    setHasFocus(true);
    const onResync = vi.fn(async () => ({
      auto_sleeps_at: null,
      timeout_action: null,
      keep_time_deadline: null,
      keep_time_action: null
    }));
    const { unmount } = render(CountdownOverlay, {
      props: { onResync }
    });

    setHasFocus(false);
    window.dispatchEvent(new Event('blur'));
    await new Promise(r => setTimeout(r, 0));
    await tick();
    expect(onResync).toHaveBeenCalledTimes(1);

    setHasFocus(true);
    window.dispatchEvent(new Event('focus'));
    await new Promise(r => setTimeout(r, 0));
    await tick();
    expect(onResync).toHaveBeenCalledTimes(2);
    unmount();
  });

  it('shows the earlier deadline when both exist', () => {
    const auto = new Date(Date.now() + 10 * 60_000).toISOString();
    const keep = new Date(Date.now() + 60_000).toISOString();
    const { container, unmount } = render(CountdownOverlay, {
      props: {
        auto_sleeps_at: auto,
        timeout_action: 'remove',
        keep_time_deadline: keep,
        keep_time_action: 'pause'
      }
    });
    expect(container.textContent).toContain('01:00');
    expect(screen.getByText('Pause on expiry')).toBeTruthy();
    unmount();
  });

  it('re-syncs through the page-provided callback', async () => {
    vi.useFakeTimers();
    try {
      const next = {
        auto_sleeps_at: null,
        timeout_action: null,
        keep_time_deadline: null,
        keep_time_action: null
      };
      const onResync = vi.fn(async () => next);
      const deadline = new Date(Date.now() + 60_000).toISOString();
      const { container, unmount } = render(CountdownOverlay, {
        props: { auto_sleeps_at: deadline, onResync }
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
      const { unmount } = render(CountdownOverlay, {
        props: { auto_sleeps_at: deadline, onResync }
      });
      expect(onResync).not.toHaveBeenCalled();

      vi.advanceTimersByTime(1_000);
      await vi.runOnlyPendingTimersAsync();
      expect(onResync).toHaveBeenCalled();
      unmount();
    } finally {
      vi.useRealTimers();
    }
  });

  it('re-syncs at zero for a keep-time-only deadline', async () => {
    vi.useFakeTimers();
    try {
      setVisibility('visible');
      setHasFocus(false);
      const onResync = vi.fn(async () => null);
      const deadline = new Date(Date.now() + 1_000).toISOString();
      const { unmount } = render(CountdownOverlay, {
        props: { keep_time_deadline: deadline, keep_time_action: 'stop', onResync }
      });
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
