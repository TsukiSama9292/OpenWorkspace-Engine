import { describe, it, expect } from 'vitest';
import { filterDashboard, countSessionsByStatus } from '$lib/dashboard/filter';
import type { Instance, Template } from '$lib/types';

function session(overrides: Partial<Instance> = {}): Instance {
  return {
    id: 's1',
    name: 'Ubuntu dev',
    template_id: 't1',
    template_name: 'Ubuntu',
    remote_type: 'kasmvnc',
    owner_id: 'u1',
    owner_username: 'alice',
    owner_group_ids: [],
    owner_tier: 0,
    status: 'running',
    instance_number: 1,
    container_id: 'c1',
    mount_persistent: false,
    created_at: '2026-01-01T00:00:00Z',
    updated_at: '2026-01-01T00:00:00Z',
    ...overrides
  };
}

function template(overrides: Partial<Template> = {}): Template {
  return {
    id: 't1',
    name: 'Ubuntu',
    description: 'General purpose desktop',
    owner_id: 'admin',
    image: 'ow-ubuntu:jammy',
    cores: 2,
    memory: 4294967296,
    gpu_count: 0,
    docker_registry: '',
    remote_type: 'kasmvnc',
    persistent_storage_path: '',
    container_runtime: 'runc',
    max_run_seconds: -1,
    timeout_action: 'remove',
    keep_time_seconds: -1,
    keep_time_action: 'pause',
    network_bandwidth_up_mbps: 0,
    network_bandwidth_down_mbps: 0,
    docker_in_instance: false,
    visibility: 'public',
    run_config: {},
    exec_config: {},
    volume_mappings: {},
    created_at: '2026-01-01T00:00:00Z',
    updated_at: '2026-01-01T00:00:00Z',
    ...overrides
  };
}

describe('filterDashboard', () => {
  it('returns everything when the query is blank', () => {
    const sessions = [session({ id: 's1' }), session({ id: 's2', status: 'stopped' })];
    const templates = [template({ id: 't1' }), template({ id: 't2', name: 'Python' })];
    const out = filterDashboard(sessions, templates, { query: '   ', status: '' });
    expect(out.sessions.map((s) => s.id)).toEqual(['s1', 's2']);
    expect(out.templates.map((t) => t.id)).toEqual(['t1', 't2']);
  });

  it('matches sessions by name case-insensitively', () => {
    const sessions = [session({ id: 's1', name: 'Ubuntu dev' }), session({ id: 's2', name: 'Rust box', template_name: 'Rust' })];
    const out = filterDashboard(sessions, [], { query: 'UBUNTU', status: '' });
    expect(out.sessions.map((s) => s.id)).toEqual(['s1']);
  });

  it('filters sessions by the running chip', () => {
    const sessions = [
      session({ id: 's1', status: 'running' }),
      session({ id: 's2', status: 'stopped' }),
      session({ id: 's3', status: 'paused' })
    ];
    const out = filterDashboard(sessions, [], { query: '', status: 'running' });
    expect(out.sessions.map((s) => s.id)).toEqual(['s1']);
  });

  it('keeps paused sessions visible under the stopped chip', () => {
    const sessions = [
      session({ id: 's1', status: 'running' }),
      session({ id: 's2', status: 'stopped' }),
      session({ id: 's3', status: 'paused' })
    ];
    const out = filterDashboard(sessions, [], { query: '', status: 'stopped' });
    expect(out.sessions.map((s) => s.id)).toEqual(['s2', 's3']);
  });

  it('counts running versus the rest', () => {
    const sessions = [
      session({ id: 's1', status: 'running' }),
      session({ id: 's2', status: 'stopped' }),
      session({ id: 's3', status: 'paused' })
    ];
    expect(countSessionsByStatus(sessions)).toEqual({ running: 1, stopped: 2 });
    expect(countSessionsByStatus([])).toEqual({ running: 0, stopped: 0 });
  });

  it('hides hidden templates from the catalog', () => {
    const templates = [
      template({ id: 't1', name: 'Public', visibility: 'public' }),
      template({ id: 't2', name: 'HiddenOne', visibility: 'hidden' })
    ];
    const out = filterDashboard([], templates, { query: '', status: '' });
    expect(out.templates.map((t) => t.id)).toEqual(['t1']);
  });

  it('matches templates by description', () => {
    const templates = [
      template({ id: 't1', name: 'Ubuntu', description: 'General purpose desktop' }),
      template({ id: 't2', name: 'Python', description: 'ML workhorse' })
    ];
    const out = filterDashboard([], templates, { query: 'ml work', status: '' });
    expect(out.templates.map((t) => t.id)).toEqual(['t2']);
  });
});
