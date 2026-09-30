import { render, screen } from '@testing-library/svelte';
import { describe, it, expect } from 'vitest';
import Settings from '$lib/components/vnc/Settings.svelte';

describe('VNC Settings', () => {
  const settings = { quality: 5, compression: 5, viewOnly: false, clipboardSync: true, scaleViewport: true };

  it('opens with a labelled dialog and associated range controls', () => {
    render(Settings, { props: { open: true, settings: { ...settings } } });
    expect(screen.getByRole('dialog', { name: 'Settings' })).toBeTruthy();
    expect(screen.getByLabelText('Quality')).toBeTruthy();
    expect(screen.getByLabelText('Compression')).toBeTruthy();
    expect(screen.getByLabelText(/View only/)).toBeTruthy();
    expect(screen.getByLabelText(/Sync clipboard/)).toBeTruthy();
  });
});
