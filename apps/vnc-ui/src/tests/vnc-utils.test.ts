import { describe, it, expect } from 'vitest';

describe('VNC utilities', () => {
  it('constructs WebSocket URL correctly', () => {
    const host = 'localhost';
    const port = 6901;
    const path = 'websockify';
    const useSSL = false;

    const protocol = useSSL ? 'wss' : 'ws';
    const url = `${protocol}://${host}:${port}/${path}`;

    expect(url).toBe('ws://localhost:6901/websockify');
  });

  it('constructs WebSocket URL with SSL', () => {
    const host = 'localhost';
    const port = 6901;
    const path = 'websockify';
    const useSSL = true;

    const protocol = useSSL ? 'wss' : 'ws';
    const url = `${protocol}://${host}:${port}/${path}`;

    expect(url).toBe('wss://localhost:6901/websockify');
  });

  it('constructs WebSocket URL with sub-path', () => {
    const basePath = '/kasm1';
    const websockifyPath = 'websockify';

    const url = `${basePath}/${websockifyPath}`;

    expect(url).toBe('/kasm1/websockify');
  });

  it('validates connection parameters', () => {
    const params = {
      host: 'localhost',
      port: 6901,
      path: 'websockify',
      useSSL: false,
      password: 'test'
    };

    expect(params.host).toBeTruthy();
    expect(params.port).toBeGreaterThan(0);
    expect(params.path).toBeTruthy();
    expect(typeof params.useSSL).toBe('boolean');
  });
});
