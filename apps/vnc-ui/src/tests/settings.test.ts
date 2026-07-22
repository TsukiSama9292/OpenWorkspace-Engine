import { describe, it, expect, beforeEach } from 'vitest';
import { render } from '@testing-library/svelte';
import Settings from '../lib/components/Settings.svelte';

describe('Settings.svelte', () => {
  beforeEach(() => {
    document.body.innerHTML = '';
  });

  it('does not render when closed', () => {
    const { container } = render(Settings, { props: { open: false } });
    const overlay = container.querySelector('.settings-overlay');
    expect(overlay).toBeNull();
  });

  it('renders when open', () => {
    const { container } = render(Settings, { props: { open: true } });
    const overlay = container.querySelector('.settings-overlay');
    expect(overlay).toBeTruthy();
  });

  it('renders quality slider', () => {
    const { container } = render(Settings, { props: { open: true } });
    const slider = container.querySelector('input[type="range"]');
    expect(slider).toBeTruthy();
  });

  it('renders checkboxes', () => {
    const { container } = render(Settings, { props: { open: true } });
    const checkboxes = container.querySelectorAll('input[type="checkbox"]');
    expect(checkboxes.length).toBe(2);
  });
});
