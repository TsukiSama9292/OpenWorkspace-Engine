import { describe, it, expect } from 'vitest';
import {
  FINE_HOURS,
  MIN_ZOOM_SPAN,
  MAX_ZOOM_SPAN,
  fineRegionStart,
  mergedSeries,
  timeToX,
  xToTime,
  valueToY,
  domainFor,
  buildPath,
  nearestPoint,
  selectionStats,
  clampWindow,
  defaultWindow,
  followWindow,
  ticks
} from '$lib/chart/timeSeries';
import type { SeriesPoint } from '$lib/types';

const fine: SeriesPoint[] = Array.from({ length: 10 }, (_, i) => ({
  t: 10_000 + i * 15,
  v: i * 10
}));
// Newest fine sample at t=10_135, so the fine region starts at 10_135 - 1h.
const now = 10_000 + 9 * 15;
const fineStart = now - FINE_HOURS;

const coarse: SeriesPoint[] = [
  { t: 1_000, v: 5 },
  { t: 2_000, v: 8 },
  { t: 3_000, v: 2 }
];

describe('fineRegionStart', () => {
  it('is one hour before the newest fine sample', () => {
    expect(fineRegionStart(fine)).toBe(now - FINE_HOURS);
  });

  it('is Infinity when there is no fine data', () => {
    expect(fineRegionStart([])).toBe(Number.POSITIVE_INFINITY);
  });
});

describe('mergedSeries', () => {
  it('draws coarse points left of the fine region and fine points inside it', () => {
    const merged = mergedSeries(fine, coarse, 0, 50_000);
    expect(merged).toContainEqual({ t: 1_000, v: 5 });
    expect(merged).toContainEqual({ t: 2_000, v: 8 });
    expect(merged).toContainEqual({ t: 3_000, v: 2 });
    for (const p of fine) expect(merged).toContainEqual(p);
    expect(merged.every((p, i) => i === 0 || merged[i - 1].t <= p.t)).toBe(true);
  });

  it('drops coarse points inside the fine region (no double-draw)', () => {
    const coarseNearNow: SeriesPoint[] = [{ t: now - 100, v: 9 }];
    const merged = mergedSeries(fine, coarseNearNow, 0, 50_000);
    expect(merged).not.toContainEqual(coarseNearNow[0]);
  });

  it('window entirely older than one hour draws coarse only', () => {
    const merged = mergedSeries(fine, coarse, 0, 4_000);
    expect(merged).toEqual([{ t: 1_000, v: 5 }, { t: 2_000, v: 8 }, { t: 3_000, v: 2 }]);
  });

  it('window inside the last hour draws fine only', () => {
    const merged = mergedSeries(fine, coarse, fineStart, now);
    expect(merged).toEqual(fine);
  });
});

describe('time mapping', () => {
  it('maps time to x and back within a window', () => {
    const x = timeToX(2_000, 1_000, 3_000, 600);
    expect(x).toBe(300);
    expect(xToTime(x, 1_000, 3_000, 600)).toBe(2_000);
  });

  it('maps value to y within a domain, clamping outside it', () => {
    expect(valueToY(50, 0, 100, 200)).toBe(100);
    expect(valueToY(200, 0, 100, 200)).toBe(0);
    expect(valueToY(-5, 0, 100, 200)).toBe(200);
  });
});

describe('domainFor', () => {
  it('uses explicit bounds when given', () => {
    expect(domainFor([{ t: 1, v: 5 }], 0, 100)).toEqual({ min: 0, max: 100 });
  });

  it('falls back to data bounds floored at zero', () => {
    expect(domainFor([{ t: 1, v: 5 }, { t: 2, v: 30 }])).toEqual({ min: 0, max: 30 });
  });

  it('keeps a nonzero span for flat data', () => {
    const d = domainFor([{ t: 1, v: 0 }]);
    expect(d.max).toBeGreaterThan(d.min);
  });
});

describe('buildPath', () => {
  it('returns empty strings for fewer than two points', () => {
    expect(buildPath([], 0, 100, 100, 100, 0, 100)).toEqual({ line: '', area: '' });
    expect(buildPath([{ t: 1, v: 1 }], 0, 100, 100, 100, 0, 100)).toEqual({ line: '', area: '' });
  });

  it('produces a polyline across the window', () => {
    const { line, area } = buildPath(
      [
        { t: 0, v: 0 },
        { t: 100, v: 100 }
      ],
      0,
      100,
      100,
      100,
      0,
      100
    );
    expect(line).toBe('M0.00,100.00 L100.00,0.00');
    expect(area.endsWith('L100,100 L0,100 Z')).toBe(true);
  });
});

describe('nearestPoint', () => {
  const pts: SeriesPoint[] = [
    { t: 0, v: 1 },
    { t: 100, v: 2 },
    { t: 200, v: 3 }
  ];

  it('snaps to a point within the radius', () => {
    expect(nearestPoint(pts, 98, 0, 200, 200)).toEqual({ t: 100, v: 2 });
  });

  it('returns null outside the radius', () => {
    expect(nearestPoint(pts, 140, 0, 200, 200)).toBeNull();
  });
});

describe('selectionStats', () => {
  it('computes count/min/max/avg over the selected displayed points', () => {
    const stats = selectionStats(fine, fineStart - 20, now + 20);
    expect(stats.count).toBe(10);
    expect(stats.min).toBe(0);
    expect(stats.max).toBe(90);
    expect(stats.avg).toBe(45);
  });

  it('returns zeros for an empty selection', () => {
    expect(selectionStats(fine, 9_000_000, 9_900_000)).toEqual({ count: 0, min: 0, max: 0, avg: 0 });
  });
});

describe('clampWindow', () => {
  it('expands a too-narrow window to the minimum span', () => {
    const w = clampWindow(1_000, 1_010);
    expect(w.end - w.start).toBe(MIN_ZOOM_SPAN);
  });

  it('shrinks a too-wide window to the maximum span', () => {
    const w = clampWindow(0, 100 * 3600);
    expect(w.end - w.start).toBe(MAX_ZOOM_SPAN);
  });

  it('leaves an in-range window untouched', () => {
    const w = clampWindow(0, 3_600);
    expect(w).toEqual({ start: 0, end: 3_600 });
  });
});

describe('window helpers', () => {
  it('defaults to a full 24h window ending at the newest data', () => {
    const w = defaultWindow(now);
    expect(w.end).toBe(now);
    expect(w.end - w.start).toBe(MAX_ZOOM_SPAN);
  });

  it('follows newer data while preserving the span', () => {
    const w = followWindow({ start: 0, end: 3_600 }, 100_000);
    expect(w.end).toBe(100_000);
    expect(w.end - w.start).toBe(3_600);
  });

  it('produces evenly spaced ticks', () => {
    expect(ticks(0, 100, 4)).toEqual([0, 25, 50, 75, 100]);
  });
});
