export type TriStateMode = 'unlimited' | 'disabled' | 'custom';

/**
 * A resource cap with tri-state semantics: `-1` = unlimited, `0` = disabled
 * (a zero cap that blocks any request), positive = an exact cap.
 */
export interface TriState {
  mode: TriStateMode;
  /** The custom cap; only meaningful when `mode === 'custom'`. */
  value: number;
}

export const UNLIMITED: TriState = { mode: 'unlimited', value: 1 };
export const DISABLED: TriState = { mode: 'disabled', value: 0 };

export function triStateFromValue(value: number | null | undefined, fallback = -1): TriState {
  const n = value ?? fallback;
  if (n < 0) return UNLIMITED;
  if (n === 0) return DISABLED;
  return { mode: 'custom', value: n };
}

export function valueFromTriState(t: TriState): number {
  if (t.mode === 'unlimited') return -1;
  if (t.mode === 'disabled') return 0;
  return t.value;
}

export function parsePositiveInt(raw: string): number | null {
  const n = Number.parseInt(raw, 10);
  if (!Number.isFinite(n) || n <= 0) return null;
  return n;
}

export function isTriStateValid(t: TriState): boolean {
  return t.mode !== 'custom' || t.value > 0;
}
