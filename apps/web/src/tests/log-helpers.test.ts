import { describe, it, expect, afterEach } from 'vitest';
import {
  formatAuditTime,
  fullAuditTime,
  shouldAutoscroll,
  LOG_FONT_KEY,
  LOG_FONT_MIN,
  LOG_FONT_MAX,
  LOG_FONT_DEFAULT,
  clampFontSize,
  loadLogFontSize,
  saveLogFontSize
} from '$lib/logs/log-helpers';

describe('formatAuditTime', () => {
  it('formats an ISO timestamp as YYYY-MM-DD HH:MM', () => {
    const iso = new Date(2026, 7, 8, 15, 14, 30).toISOString();
    expect(formatAuditTime(iso)).toBe('2026-08-08 15:14');
  });

  it('zero-pads month, day, hour and minute', () => {
    const iso = new Date(2026, 0, 5, 9, 7, 0).toISOString();
    expect(formatAuditTime(iso)).toBe('2026-01-05 09:07');
  });

  it('falls back to the raw input for invalid dates', () => {
    expect(formatAuditTime('not-a-date')).toBe('not-a-date');
    expect(formatAuditTime('')).toBe('');
  });
});

describe('fullAuditTime', () => {
  it('returns the full locale string for the title attribute', () => {
    const iso = new Date(2026, 7, 8, 15, 14, 30).toISOString();
    expect(fullAuditTime(iso)).toBe(new Date(iso).toLocaleString());
  });

  it('falls back to the raw input for invalid dates', () => {
    expect(fullAuditTime('garbage')).toBe('garbage');
  });
});

describe('shouldAutoscroll', () => {
  it('returns true when the viewport is pinned to the bottom', () => {
    expect(shouldAutoscroll(0, 100, 100)).toBe(true);
    expect(shouldAutoscroll(500, 1000, 500)).toBe(true);
  });

  it('returns true when within the threshold of the bottom', () => {
    expect(shouldAutoscroll(476, 1000, 500)).toBe(true);
  });

  it('returns false when scrolled above the threshold', () => {
    expect(shouldAutoscroll(475, 1000, 500)).toBe(false);
    expect(shouldAutoscroll(200, 1000, 500)).toBe(false);
  });

  it('respects a custom threshold', () => {
    expect(shouldAutoscroll(490, 1000, 500, 10)).toBe(true);
    expect(shouldAutoscroll(489, 1000, 500, 10)).toBe(false);
  });
});

describe('log font-size settings', () => {
  afterEach(() => {
    localStorage.clear();
  });

  it('exposes the range and default constants', () => {
    expect(LOG_FONT_MIN).toBe(12);
    expect(LOG_FONT_MAX).toBe(16);
    expect(LOG_FONT_DEFAULT).toBe(13);
    expect(LOG_FONT_KEY).toBe('ow-log-font-size');
  });

  it('clamps font sizes into the 12-16px range', () => {
    expect(clampFontSize(10)).toBe(12);
    expect(clampFontSize(20)).toBe(16);
    expect(clampFontSize(14)).toBe(14);
    expect(clampFontSize(NaN)).toBe(13);
  });

  it('returns the default when nothing is stored', () => {
    expect(loadLogFontSize()).toBe(13);
  });

  it('round-trips a saved size through localStorage', () => {
    saveLogFontSize(15);
    expect(localStorage.getItem(LOG_FONT_KEY)).toBe('15');
    expect(loadLogFontSize()).toBe(15);
  });

  it('clamps an out-of-range stored value on load', () => {
    localStorage.setItem(LOG_FONT_KEY, '99');
    expect(loadLogFontSize()).toBe(16);
  });

  it('falls back to the default for a non-numeric stored value', () => {
    localStorage.setItem(LOG_FONT_KEY, 'abc');
    expect(loadLogFontSize()).toBe(13);
  });
});
