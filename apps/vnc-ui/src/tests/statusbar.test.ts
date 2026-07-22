import { describe, it, expect, beforeEach } from 'vitest';
import { render } from '@testing-library/svelte';
import StatusBar from '../lib/components/StatusBar.svelte';

describe('StatusBar.svelte', () => {
  beforeEach(() => {
    document.body.innerHTML = '';
  });

  it('renders status text', () => {
    const { container } = render(StatusBar, { props: { status: 'idle' } });
    const text = container.querySelector('.status-text');
    expect(text).toBeTruthy();
    expect(text?.textContent).toBe('Ready');
  });

  it('shows connected status', () => {
    const { container } = render(StatusBar, { props: { status: 'connected' } });
    const text = container.querySelector('.status-text');
    expect(text?.textContent).toBe('Connected');
  });

  it('shows action buttons when connected', () => {
    const { container } = render(StatusBar, { props: { status: 'connected' } });
    const actions = container.querySelector('.status-actions');
    expect(actions).toBeTruthy();
  });

  it('hides action buttons when not connected', () => {
    const { container } = render(StatusBar, { props: { status: 'idle' } });
    const actions = container.querySelector('.status-actions');
    expect(actions).toBeNull();
  });

  it('has correct CSS classes', () => {
    const { container } = render(StatusBar, { props: { status: 'idle' } });
    expect(container.querySelector('.status-bar')).toBeTruthy();
    expect(container.querySelector('.status-dot')).toBeTruthy();
    expect(container.querySelector('.status-left')).toBeTruthy();
  });
});
