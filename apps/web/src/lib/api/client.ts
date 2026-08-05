import type { ApiResult, PreflightRejection } from '$lib/types';
import { isPreflightRejection } from '$lib/types';

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
      return { error: `Server error (${res.status})`, status: res.status };
    }

    if (!res.ok) {
      const payload = (data && typeof data === 'object') ? (data as Record<string, unknown>) : null;
      const msg = payload && typeof payload.error === 'string'
        ? payload.error
        : `Request failed (${res.status})`;
      const rejection: PreflightRejection | undefined =
        payload && isPreflightRejection(payload.rejection) ? payload.rejection : undefined;
      return { error: msg, status: res.status, rejection };
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
