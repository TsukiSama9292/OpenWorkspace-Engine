import { describe, it, expect } from 'vitest';
import { loadRailCollapsed, saveRailCollapsed } from '$lib/dashboard/navigation';

function memoryStore(initial: Record<string, string> = {}): Storage {
  const data = new Map(Object.entries(initial));
  return {
    get length() {
      return data.size;
    },
    clear: () => data.clear(),
    getItem: (k: string) => (data.has(k) ? data.get(k)! : null),
    key: (i: number) => [...data.keys()][i] ?? null,
    removeItem: (k: string) => void data.delete(k),
    setItem: (k: string, v: string) => void data.set(k, v)
  };
}

describe('rail collapsed preference', () => {
  it('defaults to expanded when nothing is stored', () => {
    expect(loadRailCollapsed(memoryStore())).toBe(false);
  });

  it('round-trips a collapsed preference', () => {
    const store = memoryStore();
    saveRailCollapsed(store, true);
    expect(loadRailCollapsed(store)).toBe(true);
    saveRailCollapsed(store, false);
    expect(loadRailCollapsed(store)).toBe(false);
  });

  it('treats corrupt values as expanded', () => {
    expect(loadRailCollapsed(memoryStore({ 'ow-rail-collapsed': 'maybe' }))).toBe(false);
  });
});
