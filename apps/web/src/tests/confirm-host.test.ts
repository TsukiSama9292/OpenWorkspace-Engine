import { render, screen, fireEvent, within } from '@testing-library/svelte';
import { describe, it, expect, vi } from 'vitest';
import ConfirmHost from '$lib/components/ui/ConfirmHost.svelte';

describe('ConfirmHost', () => {
  it('renders nothing without a request', () => {
    const { container } = render(ConfirmHost, { props: { request: null } });
    expect(container.querySelector('[data-testid="confirm-dialog"]')).toBeNull();
  });

  it('renders the request and runs it on confirm', async () => {
    const onConfirm = vi.fn();
    render(ConfirmHost, {
      props: {
        request: { title: 'Go?', body: 'Sure?', confirmLabel: 'Go', danger: false, onConfirm }
      }
    });
    const dialog = screen.getByRole('dialog', { name: 'Go?' });
    expect(dialog).toBeTruthy();
    await fireEvent.click(within(dialog).getByRole('button', { name: 'Go' }));
    expect(onConfirm).toHaveBeenCalledTimes(1);
    expect(screen.queryByRole('dialog')).toBeNull();
  });

  it('drops the request on cancel without running it', async () => {
    const onConfirm = vi.fn();
    render(ConfirmHost, {
      props: {
        request: { title: 'Go?', body: 'Sure?', confirmLabel: 'Go', danger: true, onConfirm }
      }
    });
    const dialog = screen.getByRole('dialog', { name: 'Go?' });
    await fireEvent.click(within(dialog).getByRole('button', { name: 'Cancel' }));
    expect(onConfirm).not.toHaveBeenCalled();
    expect(screen.queryByRole('dialog')).toBeNull();
  });
});
