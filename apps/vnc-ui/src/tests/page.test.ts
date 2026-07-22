import { describe, it, expect, beforeEach } from 'vitest';
import { render } from '@testing-library/svelte';
import Page from '../routes/[...path]/+page.svelte';

describe('+page.svelte', () => {
  beforeEach(() => {
    document.body.innerHTML = '';
  });

  it('renders status bar', () => {
    const { container } = render(Page);
    const statusBar = container.querySelector('.status-bar');
    expect(statusBar).toBeTruthy();
  });

  it('renders VNC viewport', () => {
    const { container } = render(Page);
    const viewport = container.querySelector('.vnc-viewport');
    expect(viewport).toBeTruthy();
  });

  it('renders VncViewer component', () => {
    const { container } = render(Page);
    const viewer = container.querySelector('.vnc-viewer');
    expect(viewer).toBeTruthy();
  });

  it('has correct layout structure', () => {
    const { container } = render(Page);
    expect(container.querySelector('.vnc-container')).toBeTruthy();
    expect(container.querySelector('.status-bar')).toBeTruthy();
    expect(container.querySelector('.vnc-viewport')).toBeTruthy();
  });
});
