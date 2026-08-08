//! Pure time-series chart math for the Monitor dashboard (interactive charts).
//! No DOM, no Svelte — every function is unit-testable in isolation, mirroring
//! how the API separates pure modules from route handlers.

import type { SeriesPoint } from '$lib/types';

/** The fine tier covers the last hour; older data is coarse (5 min). */
export const FINE_HOURS = 3600;
/** Drag-zoom window is clamped between roughly 5 minutes and 24 hours. */
export const MIN_ZOOM_SPAN = 5 * 60;
export const MAX_ZOOM_SPAN = 24 * 3600;
/** Pointer is snapped to a point only when within this many viewBox px. */
export const SNAP_RADIUS_PX = 24;
/** A pointer drag must move at least this many px to count as a zoom. */
export const DRAG_THRESHOLD_PX = 3;

/** Where the fine tier's data begins: one hour before its newest sample. */
export function fineRegionStart(fine: SeriesPoint[]): number {
  const newest = fine[fine.length - 1];
  return newest ? newest.t - FINE_HOURS : Number.POSITIVE_INFINITY;
}

/**
 * Merge the fine and coarse tiers for a visible window, drawn at the chosen
 * resolution: fine points inside the last hour, coarse points to its left.
 * Coarse points inside the fine region are dropped (the fine line covers it).
 * Returns points sorted ascending by `t`.
 */
export function mergedSeries(
  fine: SeriesPoint[],
  coarse: SeriesPoint[],
  start: number,
  end: number
): SeriesPoint[] {
  const fineStart = fineRegionStart(fine);
  const out: SeriesPoint[] = [];
  for (const p of fine) {
    if (p.t >= Math.max(start, fineStart) && p.t <= end) out.push(p);
  }
  for (const p of coarse) {
    if (p.t >= start && p.t < fineStart && p.t <= end) out.push(p);
  }
  return out.sort((a, b) => a.t - b.t);
}

export function timeToX(t: number, start: number, end: number, width: number): number {
  if (end <= start) return 0;
  return ((t - start) / (end - start)) * width;
}

export function xToTime(x: number, start: number, end: number, width: number): number {
  if (width <= 0) return start;
  return start + (x / width) * (end - start);
}

export function valueToY(v: number, min: number, max: number, height: number): number {
  const span = max - min || 1;
  return height - ((Math.min(Math.max(v, min), max) - min) / span) * height;
}

/** Resolve the y-domain: explicit bounds win, otherwise data min/max (floored at 0). */
export function domainFor(
  values: SeriesPoint[],
  min?: number,
  max?: number
): { min: number; max: number } {
  const dataMin = values.length ? Math.min(...values.map(p => p.v)) : 0;
  const dataMax = values.length ? Math.max(...values.map(p => p.v)) : 0;
  const lo = min ?? Math.min(dataMin, 0);
  const hi = max ?? Math.max(dataMax, 0);
  return { min: lo, max: hi === lo ? lo + 1 : hi };
}

export function buildPath(
  points: SeriesPoint[],
  start: number,
  end: number,
  width: number,
  height: number,
  min: number,
  max: number
): { line: string; area: string } {
  if (points.length < 2) return { line: '', area: '' };
  const coords = points.map(
    p => `${timeToX(p.t, start, end, width).toFixed(2)},${valueToY(p.v, min, max, height).toFixed(2)}`
  );
  const line = `M${coords.join(' L')}`;
  const area = `${line} L${width},${height} L0,${height} Z`;
  return { line, area };
}

/** Nearest displayed point to a viewBox x, snapped within a pixel radius. */
export function nearestPoint(
  points: SeriesPoint[],
  x: number,
  start: number,
  end: number,
  width: number
): SeriesPoint | null {
  if (!points.length) return null;
  const maxDt = (SNAP_RADIUS_PX * (end - start)) / Math.max(width, 1);
  let best: SeriesPoint | null = null;
  let bestDt = Number.POSITIVE_INFINITY;
  for (const p of points) {
    const px = timeToX(p.t, start, end, width);
    const dt = Math.abs(px - x);
    if (dt <= maxDt && dt < bestDt) {
      best = p;
      bestDt = dt;
    }
  }
  return best;
}

export interface SelectionStats {
  count: number;
  min: number;
  max: number;
  avg: number;
}

/** Stats over the displayed points inside a time selection. */
export function selectionStats(points: SeriesPoint[], startT: number, endT: number): SelectionStats {
  const selected = points.filter(p => p.t >= Math.min(startT, endT) && p.t <= Math.max(startT, endT));
  if (!selected.length) return { count: 0, min: 0, max: 0, avg: 0 };
  const min = Math.min(...selected.map(p => p.v));
  const max = Math.max(...selected.map(p => p.v));
  const sum = selected.reduce((acc, p) => acc + p.v, 0);
  return { count: selected.length, min, max, avg: sum / selected.length };
}

/** Clamp a zoom window's span to [MIN_ZOOM_SPAN, MAX_ZOOM_SPAN] around its midpoint. */
export function clampWindow(start: number, end: number): { start: number; end: number } {
  let span = end - start;
  if (span < MIN_ZOOM_SPAN) span = MIN_ZOOM_SPAN;
  if (span > MAX_ZOOM_SPAN) span = MAX_ZOOM_SPAN;
  const mid = (start + end) / 2;
  return { start: mid - span / 2, end: mid + span / 2 };
}

/** The default 24 h view with the right edge pinned to the newest data. */
export function defaultWindow(dataEnd: number): { start: number; end: number } {
  return { start: dataEnd - MAX_ZOOM_SPAN, end: dataEnd };
}

/** Slide the window so its end tracks a newer `dataEnd`, keeping its span. */
export function followWindow(
  window: { start: number; end: number },
  dataEnd: number
): { start: number; end: number } {
  const span = window.end - window.start;
  return { start: dataEnd - span, end: dataEnd };
}

/** A handful of evenly spaced axis ticks across the visible window. */
export function ticks(start: number, end: number, count = 5): number[] {
  const step = (end - start) / count;
  return Array.from({ length: count + 1 }, (_, i) => start + step * i);
}

function pad2(n: number): string {
  return String(n).padStart(2, '0');
}

/** Format an epoch-seconds timestamp as HH:MM:SS. */
export function formatChartTime(t: number): string {
  const d = new Date(t * 1000);
  return `${pad2(d.getHours())}:${pad2(d.getMinutes())}:${pad2(d.getSeconds())}`;
}

/** Format an epoch-seconds timestamp as HH:MM for axis labels. */
export function formatAxisTime(t: number): string {
  const d = new Date(t * 1000);
  return `${pad2(d.getHours())}:${pad2(d.getMinutes())}`;
}
