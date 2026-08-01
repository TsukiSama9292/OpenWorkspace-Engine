import { describe, it, expect, vi, beforeEach } from 'vitest';
import { launchInstance } from '$lib/api/template-actions';

describe('launchInstance', () => {
  beforeEach(() => {
    vi.restoreAllMocks();
  });

  function stubSuccess() {
    const mockFetch = vi.fn().mockResolvedValue({
      ok: true,
      text: () => Promise.resolve(JSON.stringify({ instance: { id: 'i1', mount_persistent: false } }))
    });
    vi.stubGlobal('fetch', mockFetch);
    return mockFetch;
  }

  it('defaults to no persistence without a client host path', async () => {
    const mockFetch = stubSuccess();

    await launchInstance('tpl-1');

    const body = JSON.parse(mockFetch.mock.calls[0][1].body as string);
    expect(body).toEqual({
      template_id: 'tpl-1',
      persistence: 'no_persistent',
      mount_persistent: false
    });
    expect(body.resolved_volume_host_path).toBeUndefined();
  });

  it('sends use_persistent with mount_persistent true', async () => {
    const mockFetch = stubSuccess();

    await launchInstance('tpl-1', 'use_persistent');

    const body = JSON.parse(mockFetch.mock.calls[0][1].body as string);
    expect(body).toEqual({
      template_id: 'tpl-1',
      persistence: 'use_persistent',
      mount_persistent: true
    });
  });

  it('sends reset_persistent with mount_persistent true', async () => {
    const mockFetch = stubSuccess();

    await launchInstance('tpl-1', 'reset_persistent');

    const body = JSON.parse(mockFetch.mock.calls[0][1].body as string);
    expect(body).toEqual({
      template_id: 'tpl-1',
      persistence: 'reset_persistent',
      mount_persistent: true
    });
  });

  it('surfaces API errors', async () => {
    vi.stubGlobal('fetch', vi.fn().mockResolvedValue({
      ok: false,
      status: 409,
      text: () => Promise.resolve(JSON.stringify({ error: 'persistent storage already exists' }))
    }));

    const result = await launchInstance('tpl-1', 'use_persistent');
    expect(result.error).toBe('persistent storage already exists');
  });
});
