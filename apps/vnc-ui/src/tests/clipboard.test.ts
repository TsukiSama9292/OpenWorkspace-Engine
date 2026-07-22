import { describe, it, expect, beforeEach } from 'vitest';
import { render } from '@testing-library/svelte';
import Clipboard from '../lib/components/Clipboard.svelte';

describe('Clipboard.svelte', () => {
  beforeEach(() => {
    document.body.innerHTML = '';
  });

  it('does not render when closed', () => {
    const { container } = render(Clipboard, { props: { open: false } });
    const overlay = container.querySelector('.clipboard-overlay');
    expect(overlay).toBeNull();
  });

  it('renders when open', () => {
    const { container } = render(Clipboard, { props: { open: true } });
    const overlay = container.querySelector('.clipboard-overlay');
    expect(overlay).toBeTruthy();
  });

  it('renders textarea', () => {
    const { container } = render(Clipboard, { props: { open: true } });
    const textarea = container.querySelector('textarea');
    expect(textarea).toBeTruthy();
  });

  it('has send button', () => {
    const { container } = render(Clipboard, { props: { open: true } });
    const sendBtn = container.querySelector('.btn.primary');
    expect(sendBtn).toBeTruthy();
  });
});
