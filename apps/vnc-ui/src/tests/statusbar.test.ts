import { describe, it, expect, beforeEach } from 'vitest';
import { render } from '@testing-library/svelte';
import StatusBar from '../lib/components/StatusBar.svelte';

describe('StatusBar.svelte', () => {
  beforeEach(() => {
    document.body.innerHTML = '';
  });

  it('renders status text', () => {
    const { container } = render(StatusBar, { props: { status: 'idle' } });
    const text = container.querySelector('.status-label');
    expect(text).toBeTruthy();
    expect(text?.textContent).toBe('Ready');
  });

  it('shows connected status', () => {
    const { container } = render(StatusBar, { props: { status: 'connected' } });
    const text = container.querySelector('.status-label');
    expect(text?.textContent).toBe('Connected');
  });

  it('shows action buttons when connected', () => {
    const { container } = render(StatusBar, { props: { status: 'connected' } });
    const actions = container.querySelector('.sidebar-actions');
    expect(actions).toBeTruthy();
  });

  it('hides connected-only buttons when not connected', () => {
    const { container } = render(StatusBar, { props: { status: 'idle' } });
    const clipboardBtn = container.querySelector('[title="Clipboard"]');
    const ctrlAltDelBtn = container.querySelector('[title="Ctrl+Alt+Del"]');
    const fullscreenBtn = container.querySelector('[title="Fullscreen"]');
    const settingsBtn = container.querySelector('[title="Settings"]');
    expect(clipboardBtn).toBeNull();
    expect(ctrlAltDelBtn).toBeNull();
    expect(fullscreenBtn).toBeNull();
    expect(settingsBtn).toBeNull();
  });

  it('has correct CSS classes', () => {
    const { container } = render(StatusBar, { props: { status: 'idle' } });
    expect(container.querySelector('.sidebar')).toBeTruthy();
    expect(container.querySelector('.status-dot')).toBeTruthy();
    expect(container.querySelector('.status-indicator')).toBeTruthy();
  });

  it('renders toggle button', () => {
    const { container } = render(StatusBar, { props: { status: 'idle' } });
    const toggle = container.querySelector('.toggle-btn');
    expect(toggle).toBeTruthy();
  });

  it('starts expanded by default', () => {
    const { container } = render(StatusBar, { props: { status: 'idle' } });
    const sidebar = container.querySelector('.sidebar');
    expect(sidebar?.classList.contains('collapsed')).toBe(false);
  });
});
