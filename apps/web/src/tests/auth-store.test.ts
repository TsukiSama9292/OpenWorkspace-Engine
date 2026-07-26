import { describe, it, expect, vi, beforeEach } from 'vitest';
import { get } from 'svelte/store';
import { auth, isAuthenticated, isAdmin } from '$lib/stores/auth';

vi.mock('$lib/api/client', () => ({
  api: {
    get: vi.fn(),
    post: vi.fn()
  }
}));

import { api } from '$lib/api/client';
const mockApi = vi.mocked(api);

describe('auth store', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    auth.logout();
  });

  it('starts unauthenticated', () => {
    expect(get(auth)).toBeNull();
    expect(get(isAuthenticated)).toBe(false);
    expect(get(isAdmin)).toBe(false);
  });

  it('login sets user on success', async () => {
    mockApi.post.mockResolvedValue({
      data: { user: { id: '1', username: 'admin', role: 'admin' } }
    });

    const result = await auth.login('admin', 'pass');
    expect(result).toBe(true);
    expect(get(auth)).toEqual({ id: '1', username: 'admin', role: 'admin' });
    expect(get(isAuthenticated)).toBe(true);
    expect(get(isAdmin)).toBe(true);
  });

  it('login returns false on failure', async () => {
    mockApi.post.mockResolvedValue({ error: 'Invalid credentials' });

    const result = await auth.login('wrong', 'pass');
    expect(result).toBe(false);
    expect(get(auth)).toBeNull();
    expect(get(isAuthenticated)).toBe(false);
  });

  it('logout clears user', async () => {
    mockApi.post.mockResolvedValue({ data: { user: { id: '1', username: 'admin', role: 'admin' } } });
    await auth.login('admin', 'pass');
    expect(get(isAuthenticated)).toBe(true);

    mockApi.post.mockResolvedValue({});
    await auth.logout();
    expect(get(auth)).toBeNull();
    expect(get(isAuthenticated)).toBe(false);
  });

  it('check sets user when authenticated', async () => {
    mockApi.get.mockResolvedValue({
      data: { user: { id: '2', username: 'user', role: 'user' } }
    });

    await auth.check();
    expect(get(auth)).toEqual({ id: '2', username: 'user', role: 'user' });
    expect(get(isAdmin)).toBe(false);
  });

  it('check clears user when not authenticated', async () => {
    mockApi.get.mockResolvedValue({ error: 'Not found' });

    await auth.check();
    expect(get(auth)).toBeNull();
    expect(get(isAuthenticated)).toBe(false);
  });
});
