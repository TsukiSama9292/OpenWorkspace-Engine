import { render, screen, fireEvent, waitFor } from '@testing-library/svelte';
import { describe, it, expect, vi } from 'vitest';
import Clipboard from '$lib/components/vnc/Clipboard.svelte';

describe('Clipboard', () => {
  it('opens with a labelled dialog, an editor, and three weighted actions', () => {
    render(Clipboard, { props: { open: true } });
    expect(screen.getByRole('dialog', { name: 'Clipboard' })).toBeTruthy();
    expect(screen.getByPlaceholderText(/Paste or type text/)).toBeTruthy();
    expect(screen.getByRole('button', { name: 'Send to remote' })).toBeTruthy();
    expect(screen.getByRole('button', { name: 'Read from clipboard' })).toBeTruthy();
    expect(screen.getByRole('button', { name: 'Copy to clipboard' })).toBeTruthy();
  });

  it('sends trimmed text and clears the editor', async () => {
    const onSend = vi.fn();
    render(Clipboard, { props: { open: true, onSend } });
    const editor = screen.getByPlaceholderText(/Paste or type text/);
    await fireEvent.input(editor, { target: { value: 'hello remote' } });
    await fireEvent.click(screen.getByRole('button', { name: 'Send to remote' }));
    expect(onSend).toHaveBeenCalledWith('hello remote');
    await waitFor(() => {
      expect((editor as HTMLTextAreaElement).value).toBe('');
    });
  });

  it('sends nothing for blank text', async () => {
    const onSend = vi.fn();
    render(Clipboard, { props: { open: true, onSend } });
    await fireEvent.click(screen.getByRole('button', { name: 'Send to remote' }));
    expect(onSend).not.toHaveBeenCalled();
  });
});
