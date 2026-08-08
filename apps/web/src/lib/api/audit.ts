import { api } from '$lib/api/client';
import type { AuditEntry, AuditPage, AuditQuery } from '$lib/types';

/** Serialize the optional audit filters into a URL query string (pure). */
export function buildAuditQueryString(query: AuditQuery): string {
  const params = new URLSearchParams();
  if (query.action) params.set('action', query.action);
  if (query.actor) params.set('actor', query.actor);
  if (query.target) params.set('target', query.target);
  if (query.outcome) params.set('outcome', query.outcome);
  if (query.after) params.set('after', query.after);
  if (query.before) params.set('before', query.before);
  if (query.cursor) params.set('cursor', query.cursor);
  if (query.limit) params.set('limit', String(query.limit));
  const qs = params.toString();
  return qs ? `?${qs}` : '';
}

export async function fetchAudit(query: AuditQuery): Promise<{ page?: AuditPage; error?: string }> {
  const res = await api.get<{ entries: AuditEntry[]; next_cursor: string | null }>(
    `/audit${buildAuditQueryString(query)}`
  );
  if (res.error || !res.data) return { error: res.error ?? 'Failed to load audit trail' };
  return {
    page: { entries: res.data.entries, next_cursor: res.data.next_cursor }
  };
}
