import { describe, it, expect } from 'vitest';
import { preflightMessage, preflightNumbers, preflightTitle } from '$lib/preflight';
import { isPreflightRejection, type PreflightRejection } from '$lib/types';

const NOT_ALLOWED: PreflightRejection = { scope: 'template_not_allowed', current: 0, limit: 0, requested: 1 };
const USER_CEILING: PreflightRejection = { scope: 'user_instance', current: 2, limit: 2, requested: 1 };
const HOST_CEILING: PreflightRejection = { scope: 'host_instance', current: 5, limit: 5, requested: 1 };

describe('preflight rejection copy', () => {
  it('renders a template-not-allowed rejection (403) without ceiling numbers', () => {
    expect(preflightTitle(NOT_ALLOWED)).toBe('Template not allowed');
    expect(preflightNumbers(NOT_ALLOWED)).toBeNull();

    const msg = preflightMessage(NOT_ALLOWED, 'This template is not in your allowed templates list');
    expect(msg).toContain('Template not allowed');
    expect(msg).toContain('This template is not in your allowed templates list');
  });

  it('renders a user ceiling rejection (409) with the exact numbers', () => {
    expect(preflightTitle(USER_CEILING)).toBe('Your instance limit reached');
    expect(preflightNumbers(USER_CEILING)).toBe('Current 2 / limit 2 (requested 1)');

    const msg = preflightMessage(USER_CEILING, 'Per-user instance limit reached (active: 2, limit: 2)');
    expect(msg).toContain('Your instance limit reached');
    expect(msg).toContain('Current 2 / limit 2 (requested 1)');
  });

  it('renders a host ceiling rejection (409) with the exact numbers', () => {
    expect(preflightTitle(HOST_CEILING)).toBe('Host instance limit reached');
    expect(preflightNumbers(HOST_CEILING)).toBe('Current 5 / limit 5 (requested 1)');

    const msg = preflightMessage(HOST_CEILING, 'Host instance limit reached (active: 5, limit: 5)');
    expect(msg).toContain('Host instance limit reached');
    expect(msg).toContain('Current 5 / limit 5 (requested 1)');
  });

  it('works without an API error string', () => {
    expect(preflightMessage(USER_CEILING, '')).toBe('Your instance limit reached: Current 2 / limit 2 (requested 1)');
  });
});

describe('isPreflightRejection', () => {
  it('accepts a valid body for every scope', () => {
    for (const scope of ['template_not_allowed', 'user_instance', 'host_instance']) {
      expect(isPreflightRejection({ scope, current: 2, limit: 2, requested: 1 })).toBe(true);
    }
  });

  it('rejects null, non-objects, and malformed bodies', () => {
    expect(isPreflightRejection(null)).toBe(false);
    expect(isPreflightRejection(undefined)).toBe(false);
    expect(isPreflightRejection('user_instance')).toBe(false);
    expect(isPreflightRejection({ scope: 'user_instance', current: 2 })).toBe(false);
    expect(isPreflightRejection({ scope: 'bogus', current: 2, limit: 2, requested: 1 })).toBe(false);
    expect(isPreflightRejection({ scope: 'user_instance', current: '2', limit: 2, requested: 1 })).toBe(false);
  });
});
