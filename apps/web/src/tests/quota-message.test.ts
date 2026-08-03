import { describe, it, expect } from 'vitest';
import type { QuotaPayload, QuotaScope } from '$lib/types';
import {
  quotaMessage,
  quotaScopeInfo,
  formatQuotaNumbers,
  isQuotaPayload,
} from '$lib/quota';

const SCOPES: QuotaScope[] = [
  'user_instance',
  'user_cpu',
  'user_ram',
  'host_instance',
  'host_dedicated_cpu',
  'host_dedicated_ram',
  'host_shared_cpu',
  'host_shared_ram',
];

function payload(scope: QuotaScope, current = 2, limit = 2, requested = 1): QuotaPayload {
  return { scope, current, limit, requested };
}

function samplePayload(scope: QuotaScope): QuotaPayload {
  const isMemory = scope.endsWith('ram');
  return isMemory
    ? payload(scope, 2 * 1024 ** 3, 2 * 1024 ** 3, 1 * 1024 ** 3)
    : payload(scope);
}

describe('quota message', () => {
  it.each(SCOPES)('renders a label and the numbers for scope %s', (scope) => {
    const info = quotaScopeInfo(scope);
    expect(info).toBeDefined();
    const message = quotaMessage(samplePayload(scope));
    expect(message).toContain(info!.label);
    expect(message).toContain('目前 2');
    expect(message).toContain('上限 2');
    expect(message).toContain('本次請求 1');
    expect(message).toContain(info!.guidance);
  });

  it('matches the example wording for user_instance', () => {
    expect(quotaMessage(payload('user_instance'))).toBe(
      '實例數量已達上限：目前 2 / 上限 2（本次請求 1）請先停止或刪除一個實例。'
    );
  });

  it('formats RAM scopes as human-readable sizes', () => {
    const message = quotaMessage({
      scope: 'user_ram',
      current: 6 * 1024 ** 3,
      limit: 8 * 1024 ** 3,
      requested: 4 * 1024 ** 3,
    });
    expect(message).toContain('目前 6 GB');
    expect(message).toContain('上限 8 GB');
    expect(message).toContain('本次請求 4 GB');
  });

  it('leaves CPU and count scopes as plain numbers', () => {
    expect(formatQuotaNumbers(payload('user_cpu', 3, 4, 2))).toBe(
      '目前 3 / 上限 4（本次請求 2）'
    );
    expect(formatQuotaNumbers(payload('host_instance', 5, 5, 1))).toBe(
      '目前 5 / 上限 5（本次請求 1）'
    );
  });
});

describe('isQuotaPayload', () => {
  it('accepts a valid quota body for every scope', () => {
    for (const scope of SCOPES) {
      expect(isQuotaPayload({ scope, current: 2, limit: 2, requested: 1 })).toBe(true);
    }
  });

  it('rejects non-objects and missing fields', () => {
    expect(isQuotaPayload(null)).toBe(false);
    expect(isQuotaPayload(undefined)).toBe(false);
    expect(isQuotaPayload('user_instance')).toBe(false);
    expect(isQuotaPayload({ scope: 'user_instance', current: 2 })).toBe(false);
  });

  it('rejects an unknown scope', () => {
    expect(isQuotaPayload({ scope: 'bogus', current: 2, limit: 2, requested: 1 })).toBe(false);
  });

  it('rejects non-numeric counters', () => {
    expect(isQuotaPayload({ scope: 'user_instance', current: '2', limit: 2, requested: 1 })).toBe(false);
  });
});
