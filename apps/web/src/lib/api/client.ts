import type { ApiResult } from '$lib/types';

const BASE = '/api';

async function request<T>(method: string, path: string, body?: unknown): Promise<ApiResult<T>> {
  try {
    const opts: RequestInit = {
      method,
      credentials: 'include',
      headers: { 'Content-Type': 'application/json' }
    };
    if (body) opts.body = JSON.stringify(body);

    const res = await fetch(`${BASE}${path}`, opts);

    const text = await res.text();
    let data: unknown;
    try {
      data = text ? JSON.parse(text) : null;
    } catch {
      return { error: `Server error (${res.status})` };
    }

    if (!res.ok) {
      const msg = (data && typeof data === 'object' && 'error' in data)
        ? (data as { error: string }).error
        : `Request failed (${res.status})`;
      return { error: msg };
    }

    return { data: data as T };
  } catch {
    return { error: 'Network error' };
  }
}

export const api = {
  get: <T>(path: string) => request<T>('GET', path),
  post: <T>(path: string, body?: unknown) => request<T>('POST', path, body),
  put: <T>(path: string, body?: unknown) => request<T>('PUT', path, body),
  delete: <T>(path: string) => request<T>('DELETE', path)
};
