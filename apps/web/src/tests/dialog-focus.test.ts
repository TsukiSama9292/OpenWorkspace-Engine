import { render, screen, fireEvent } from '@testing-library/svelte';
import { describe, it, expect } from 'vitest';
import { tick } from 'svelte';
import ConfirmDialog from '$lib/components/ui/ConfirmDialog.svelte';

describe('dialog focus management', () => {
  it('names its close button for assistive technology', () => {
    render(ConfirmDialog, {
      props: { open: true, title: 'Go?', body: 'Sure?' }
    });
    expect(screen.getByRole('button', { name: 'Close dialog' })).toBeTruthy();
  });

  it('moves focus into the dialog on open and traps Tab inside', async () => {
    render(ConfirmDialog, {
      props: { open: true, title: 'Go?', body: 'Sure?', confirmLabel: 'Go' }
    });
    await tick();
    expect(document.activeElement).toBe(screen.getByRole('dialog'));

    const go = screen.getByRole('button', { name: 'Go' });
    go.focus();
    await fireEvent.keyDown(go, { key: 'Tab' });
    expect(document.activeElement).toBe(screen.getByRole('button', { name: 'Close dialog' }));
  });

  it('returns focus to the previous element on Escape', async () => {
    const before = document.createElement('button');
    before.textContent = 'before';
    document.body.appendChild(before);
    before.focus();
    try {
      render(ConfirmDialog, {
        props: { open: true, title: 'Go?', body: 'Sure?', oncancel: () => {} }
      });
      await tick();
      expect(document.activeElement).toBe(screen.getByRole('dialog'));
      await fireEvent.keyDown(screen.getByRole('dialog'), { key: 'Escape' });
      expect(document.activeElement).toBe(before);
    } finally {
      before.remove();
    }
  });
});
