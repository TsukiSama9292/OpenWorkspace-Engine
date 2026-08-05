import { render, screen } from '@testing-library/svelte';
import { describe, it, expect } from 'vitest';
import RejectionNotice from '$lib/components/RejectionNotice.svelte';

describe('RejectionNotice', () => {
  it('renders a template-not-allowed rejection (403)', () => {
    render(RejectionNotice, {
      props: {
        error: 'This template is not in your allowed templates list',
        rejection: { scope: 'template_not_allowed', current: 0, limit: 0, requested: 1 },
        onclose: () => {}
      }
    });

    expect(screen.getByTestId('rejection-notice')).toBeTruthy();
    expect(screen.getByText(/Template not allowed/)).toBeTruthy();
    expect(screen.getByText(/This template is not in your allowed templates list/)).toBeTruthy();
  });

  it('renders a user ceiling rejection (409) with the exact numbers', () => {
    render(RejectionNotice, {
      props: {
        error: 'Per-user instance limit reached (active: 2, limit: 2)',
        rejection: { scope: 'user_instance', current: 2, limit: 2, requested: 1 },
        onclose: () => {}
      }
    });

    expect(screen.getByText(/Your instance limit reached/)).toBeTruthy();
    expect(screen.getByText(/Current 2 \/ limit 2 \(requested 1\)/)).toBeTruthy();
  });

  it('renders a host ceiling rejection (409) with the exact numbers', () => {
    render(RejectionNotice, {
      props: {
        error: 'Host instance limit reached (active: 5, limit: 5)',
        rejection: { scope: 'host_instance', current: 5, limit: 5, requested: 1 },
        onclose: () => {}
      }
    });

    expect(screen.getByText(/Host instance limit reached/)).toBeTruthy();
    expect(screen.getByText(/Current 5 \/ limit 5 \(requested 1\)/)).toBeTruthy();
  });

  it('renders nothing without a rejection payload', () => {
    render(RejectionNotice, { props: { error: 'boom', rejection: null, onclose: () => {} } });
    expect(screen.queryByTestId('rejection-notice')).toBeNull();
  });
});
