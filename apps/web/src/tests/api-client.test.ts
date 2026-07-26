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

    await api.post('/configs', { name: 'test' });

    expect(mockFetch).toHaveBeenCalledWith('/api/configs', expect.objectContaining({
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

    await api.delete('/configs/123');

    expect(mockFetch).toHaveBeenCalledWith('/api/configs/123', expect.objectContaining({
      method: 'DELETE'
    }));
  });

  it('sends PUT request with body', async () => {
    const mockFetch = vi.fn().mockResolvedValue({
      ok: true,
      text: () => Promise.resolve(JSON.stringify({ updated: true }))
    });
    vi.stubGlobal('fetch', mockFetch);

    await api.put('/configs/123', { name: 'updated' });

    expect(mockFetch).toHaveBeenCalledWith('/api/configs/123', expect.objectContaining({
      method: 'PUT',
      body: JSON.stringify({ name: 'updated' })
    }));
  });
});
