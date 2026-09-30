import { render, screen } from '@testing-library/svelte';
import { describe, it, expect } from 'vitest';
import StatusBar from '$lib/components/vnc/StatusBar.svelte';

const noop = () => {};

describe('StatusBar', () => {
  it('states the connection in plain words', () => {
    render(StatusBar, { props: { status: 'connecting' } });
    expect(screen.getByText('Connecting...')).toBeTruthy();
  });

  it('gives the clipboard action dominant weight while connected', () => {
    render(StatusBar, {
      props: { status: 'connected', onClipboard: noop, onReconnect: noop }
    });
    const clipboard = screen.getByRole('button', { name: 'Clipboard' });
    const settings = screen.getByRole('button', { name: 'Settings' });
    const reconnect = screen.getByRole('button', { name: 'Reconnect' });
    expect(clipboard.className).toContain('bg-primary-500/15');
    expect(settings.className).not.toContain('bg-primary-500/15');
    expect(reconnect.className).not.toContain('bg-primary-500/15');
  });

  it('gives reconnect dominant weight while disconnected', () => {
    render(StatusBar, { props: { status: 'disconnected', onReconnect: noop } });
    expect(screen.queryByRole('button', { name: 'Clipboard' })).toBeNull();
    expect(screen.getByRole('button', { name: 'Reconnect' }).className).toContain(
      'bg-primary-500/15'
    );
  });
});
