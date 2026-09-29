import type { Instance, Template } from '$lib/types';

export type DashboardStatusFilter = '' | 'running' | 'stopped';

export interface DashboardFilter {
  query: string;
  status: DashboardStatusFilter;
}

export interface FilteredDashboard {
  sessions: Instance[];
  templates: Template[];
}

export interface SessionCounts {
  running: number;
  stopped: number;
}

export function countSessionsByStatus(sessions: Instance[]): SessionCounts {
  let running = 0;
  for (const s of sessions) {
    if (s.status === 'running') running += 1;
  }
  return { running, stopped: sessions.length - running };
}

function contains_case_insensitive(haystack: string, needle: string): boolean {
  return haystack.toLowerCase().includes(needle);
}

export function filterDashboard(
  sessions: Instance[],
  templates: Template[],
  filter: DashboardFilter
): FilteredDashboard {
  const needle = filter.query.trim().toLowerCase();
  const status = filter.status;

  const outSessions = sessions.filter((s) => {
    if (status === 'running' && s.status !== 'running') return false;
    if (status === 'stopped' && s.status === 'running') return false;
    if (!needle) return true;
    return (
      contains_case_insensitive(s.name, needle) ||
      contains_case_insensitive(s.template_name, needle) ||
      contains_case_insensitive(s.owner_username, needle) ||
      contains_case_insensitive(s.id, needle)
    );
  });

  const outTemplates = templates.filter((t) => {
    if (t.visibility === 'hidden') return false;
    if (!needle) return true;
    return contains_case_insensitive(t.name, needle) || contains_case_insensitive(t.description, needle) || contains_case_insensitive(t.image, needle);
  });

  return { sessions: outSessions, templates: outTemplates };
}
