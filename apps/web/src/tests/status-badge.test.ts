import { describe, it, expect } from 'vitest';
import { statusBadgeClass } from '$lib/status-badge';

describe('statusBadgeClass', () => {
  it.each([
    ['running', 'dot-active'],
    ['paused', 'dot-paused'],
    ['stopped', 'dot-stopped'],
    ['error', 'dot-error'],
    ['starting', 'dot-starting'],
    ['orphaned', 'dot-stopped'],
  ])('maps %s to %s', (status, cls) => {
    expect(statusBadgeClass(status)).toBe(cls);
  });

  it('renders unknown or missing states unstyled rather than guessing', () => {
    expect(statusBadgeClass('migrating')).toBe('');
    expect(statusBadgeClass(null)).toBe('');
    expect(statusBadgeClass(undefined)).toBe('');
  });
});
