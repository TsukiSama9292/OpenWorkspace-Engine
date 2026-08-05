import { describe, it, expect, vi, beforeEach } from 'vitest';
import { api } from '$lib/api/client';

describe('api client', () => {
  beforeEach(() => {
    vi.restoreAllMocks();
  });

  it('sends GET request with credentials', async () => {
    const mockFetch = vi.fn().mockResolvedValue({
      ok: true,
      text: () => Promise.resolve(JSON.stringify({ users: [] }))
    });
    vi.stubGlobal('fetch', mockFetch);

    await api.get('/users');

    expect(mockFetch).toHaveBeenCalledWith('/api/users', expect.objectContaining({
      method: 'GET',
      credentials: 'include'
    }));
  });

  it('sends POST request with JSON body', async () => {
    const mockFetch = vi.fn().mockResolvedValue({
      ok: true,
      text: () => Promise.resolve(JSON.stringify({ id: '1' }))
    });
    vi.stubGlobal('fetch', mockFetch);

    await api.post('/templates', { name: 'test' });

    expect(mockFetch).toHaveBeenCalledWith('/api/templates', expect.objectContaining({
      method: 'POST',
      body: JSON.stringify({ name: 'test' })
    }));
  });

  it('returns data on success', async () => {
    vi.stubGlobal('fetch', vi.fn().mockResolvedValue({
      ok: true,
      text: () => Promise.resolve(JSON.stringify({ user: { id: '1', username: 'admin' } }))
    }));

    const result = await api.get<{ user: { id: string; username: string } }>('/auth/me');
    expect(result.data).toEqual({ user: { id: '1', username: 'admin' } });
    expect(result.error).toBeUndefined();
  });

  it('returns error on non-ok response', async () => {
    vi.stubGlobal('fetch', vi.fn().mockResolvedValue({
      ok: false,
      status: 401,
      text: () => Promise.resolve(JSON.stringify({ error: 'Unauthorized' }))
    }));

    const result = await api.get('/protected');
    expect(result.error).toBe('Unauthorized');
    expect(result.data).toBeUndefined();
  });

  it('surfaces a template-not-allowed rejection on a 403', async () => {
    vi.stubGlobal('fetch', vi.fn().mockResolvedValue({
      ok: false,
      status: 403,
      text: () => Promise.resolve(JSON.stringify({
        error: 'This template is not in your allowed templates list',
        rejection: { scope: 'template_not_allowed', current: 0, limit: 0, requested: 1 }
      }))
    }));

    const result = await api.post('/instances');
    expect(result.error).toBe('This template is not in your allowed templates list');
    expect(result.status).toBe(403);
    expect(result.rejection).toEqual({ scope: 'template_not_allowed', current: 0, limit: 0, requested: 1 });
  });

  it('surfaces a ceiling rejection with its numbers on a 409', async () => {
    vi.stubGlobal('fetch', vi.fn().mockResolvedValue({
      ok: false,
      status: 409,
      text: () => Promise.resolve(JSON.stringify({
        error: 'Per-user instance limit reached (active: 2, limit: 2)',
        rejection: { scope: 'user_instance', current: 2, limit: 2, requested: 1 }
      }))
    }));

    const result = await api.post('/instances');
    expect(result.error).toBe('Per-user instance limit reached (active: 2, limit: 2)');
    expect(result.status).toBe(409);
    expect(result.rejection).toEqual({ scope: 'user_instance', current: 2, limit: 2, requested: 1 });
  });

  it('falls back to a plain error for a 409 without a rejection body', async () => {
    vi.stubGlobal('fetch', vi.fn().mockResolvedValue({
      ok: false,
      status: 409,
      text: () => Promise.resolve(JSON.stringify({ error: 'Instance is already running' }))
    }));

    const result = await api.post('/instances/x/start');
    expect(result.error).toBe('Instance is already running');
    expect(result.status).toBe(409);
    expect(result.rejection).toBeUndefined();
  });

  it('falls back to a plain error for a 403 without a rejection body', async () => {
    vi.stubGlobal('fetch', vi.fn().mockResolvedValue({
      ok: false,
      status: 403,
      text: () => Promise.resolve(JSON.stringify({ error: 'Forbidden' }))
    }));

    const result = await api.put('/templates/x');
    expect(result.error).toBe('Forbidden');
    expect(result.rejection).toBeUndefined();
  });

  it('ignores an invalid rejection body', async () => {
    vi.stubGlobal('fetch', vi.fn().mockResolvedValue({
      ok: false,
      status: 409,
      text: () => Promise.resolve(JSON.stringify({
        error: 'bad',
        rejection: { scope: 'bogus', current: 2, limit: 2, requested: 1 }
      }))
    }));

    const result = await api.post('/instances');
    expect(result.error).toBe('bad');
    expect(result.rejection).toBeUndefined();
  });

  it('returns error on network failure', async () => {
    vi.stubGlobal('fetch', vi.fn().mockRejectedValue(new Error('Network error')));

    const result = await api.get('/anything');
    expect(result.error).toBe('Network error');
  });

  it('returns error when response is not valid JSON', async () => {
    vi.stubGlobal('fetch', vi.fn().mockResolvedValue({
      ok: true,
      status: 500,
      text: () => Promise.resolve('Internal Server Error')
    }));

    const result = await api.get('/broken');
    expect(result.error).toBe('Server error (500)');
  });

  it('sends DELETE request', async () => {
    const mockFetch = vi.fn().mockResolvedValue({
      ok: true,
      text: () => Promise.resolve('')
    });
    vi.stubGlobal('fetch', mockFetch);

    await api.delete('/templates/123');

    expect(mockFetch).toHaveBeenCalledWith('/api/templates/123', expect.objectContaining({
      method: 'DELETE'
    }));
  });

  it('sends PUT request with body', async () => {
    const mockFetch = vi.fn().mockResolvedValue({
      ok: true,
      text: () => Promise.resolve(JSON.stringify({ updated: true }))
    });
    vi.stubGlobal('fetch', mockFetch);

    await api.put('/templates/123', { name: 'updated' });

    expect(mockFetch).toHaveBeenCalledWith('/api/templates/123', expect.objectContaining({
      method: 'PUT',
      body: JSON.stringify({ name: 'updated' })
    }));
  });
});
