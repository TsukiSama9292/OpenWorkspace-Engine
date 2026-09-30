import { render, screen, fireEvent } from '@testing-library/svelte';
import { describe, it, expect, vi } from 'vitest';
import ConfirmDialog from '$lib/components/ui/ConfirmDialog.svelte';

describe('ConfirmDialog', () => {
  it('renders nothing when closed', () => {
    const { container } = render(ConfirmDialog, {
      props: { open: false, title: 'Delete?', body: 'Gone.' }
    });
    expect(container.querySelector('[data-testid="confirm-dialog"]')).toBeNull();
  });

  it('shows the title, body, and confirm label when open', () => {
    render(ConfirmDialog, {
      props: { open: true, title: 'Delete "web"?', body: 'The container will be removed.', confirmLabel: 'Delete' }
    });
    expect(screen.getByText('Delete "web"?')).toBeTruthy();
    expect(screen.getByText('The container will be removed.')).toBeTruthy();
    expect(screen.getByRole('button', { name: 'Delete' })).toBeTruthy();
    expect(screen.getByRole('button', { name: 'Cancel' })).toBeTruthy();
  });

  it('confirms without cancelling', async () => {
    const onconfirm = vi.fn();
    const oncancel = vi.fn();
    render(ConfirmDialog, {
      props: { open: true, title: 'Go?', body: 'Sure?', confirmLabel: 'Go', onconfirm, oncancel }
    });
    await fireEvent.click(screen.getByRole('button', { name: 'Go' }));
    expect(onconfirm).toHaveBeenCalledTimes(1);
    expect(oncancel).not.toHaveBeenCalled();
  });

  it('cancels via the Cancel button and via Escape', async () => {
    const onconfirm = vi.fn();
    const oncancel = vi.fn();
    render(ConfirmDialog, {
      props: { open: true, title: 'Go?', body: 'Sure?', onconfirm, oncancel }
    });
    await fireEvent.click(screen.getByRole('button', { name: 'Cancel' }));
    expect(oncancel).toHaveBeenCalledTimes(1);
    expect(onconfirm).not.toHaveBeenCalled();

    oncancel.mockClear();
    await fireEvent.keyDown(screen.getByRole('dialog'), { key: 'Escape' });
    expect(oncancel).toHaveBeenCalledTimes(1);
    expect(onconfirm).not.toHaveBeenCalled();
  });
});
