import { render, screen } from '@testing-library/svelte';
import { describe, it, expect } from 'vitest';
import QuotaModal from '$lib/components/quota/QuotaModal.svelte';
import { quotaScopeInfo } from '$lib/quota';
import type { QuotaPayload, QuotaScope } from '$lib/types';

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

function props(quota: QuotaPayload, error = 'Per-user instance limit reached (active: 2, limit: 2)') {
  return { error, quota, onclose: () => {} };
}

function samplePayload(scope: QuotaScope): QuotaPayload {
  const isMemory = scope.endsWith('ram');
  return {
    scope,
    current: isMemory ? 2 * 1024 ** 3 : 2,
    limit: isMemory ? 2 * 1024 ** 3 : 2,
    requested: isMemory ? 1 * 1024 ** 3 : 1,
  };
}

describe('QuotaModal', () => {
  it.each(SCOPES)('renders the scope label and numbers for %s', (scope) => {
    const info = quotaScopeInfo(scope);
    const quota = samplePayload(scope);
    render(QuotaModal, { props: props(quota) });

    expect(screen.getByText(new RegExp(info!.label))).toBeTruthy();
    expect(screen.getByText(/目前 2/)).toBeTruthy();
    expect(screen.getByText(/上限 2/)).toBeTruthy();
    expect(screen.getByText(/本次請求 1/)).toBeTruthy();
    expect(screen.getByText(new RegExp(info!.guidance))).toBeTruthy();
  });

  it('shows the API error string alongside the structured message', () => {
    render(QuotaModal, { props: props({ scope: 'user_instance', current: 2, limit: 2, requested: 1 }) });
    expect(screen.getByText(/Per-user instance limit reached/)).toBeTruthy();
  });

  it('renders nothing without a quota payload', () => {
    render(QuotaModal, { props: { error: 'boom', quota: null, onclose: () => {} } });
    expect(screen.queryByTestId('quota-notice')).toBeNull();
  });
});
