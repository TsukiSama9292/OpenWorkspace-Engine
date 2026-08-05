import { describe, it, expect } from 'vitest';
import {
  mayControlInstance,
  mayLaunchTemplate,
  mayManageUsers,
  mayCreateTemplate,
  mayEditTemplate
} from '$lib/permissions';
import type { EffectiveContext, Instance, Template } from '$lib/types';

function context(overrides: Partial<EffectiveContext> = {}): EffectiveContext {
  return {
    user_id: 'me',
    username: 'me',
    is_admin: false,
    tier: 0,
    can_create_template: false,
    can_manage_users: false,
    can_manage_group_instances: false,
    can_manage_docker: false,
    can_manage_registry: false,
    effective_max_instances: 4,
    allowed_template_ids: ['t1', 't2'],
    group_ids: ['g1'],
    direct_max_instances: null,
    ...overrides
  };
}

function instance(overrides: Partial<Instance> = {}): Instance {
  return {
    id: 'i1',
    name: 'inst',
    template_id: 't1',
    template_name: 'Tpl',
    remote_type: 'kasmvnc',
    owner_id: 'u-other',
    owner_username: 'other',
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
    name: 'Tpl',
    description: '',
    owner_id: 'u-owner',
    image: 'img:1',
    cores: 2,
    memory: 4294967296,
    gpu_count: 0,
    docker_registry: '',
    remote_type: 'kasmvnc',
    persistent_storage_path: '',
    container_runtime: 'docker',
    max_run_seconds: null,
    timeout_action: 'remove',
    keep_time_seconds: null,
    keep_time_action: 'pause',
    network_bandwidth_up_mbps: 0,
    network_bandwidth_down_mbps: 0,
    docker_in_instance: false,
    visibility: 'private',
    run_config: {},
    exec_config: {},
    volume_mappings: {},
    created_at: '2026-01-01T00:00:00Z',
    updated_at: '2026-01-01T00:00:00Z',
    ...overrides
  };
}

describe('mayControlInstance', () => {
  it('lets the owner always control their own instance', () => {
    const ctx = context();
    const own = instance({ owner_id: 'me' });
    expect(mayControlInstance(ctx, own)).toBe(true);
  });

  it('lets the system admin control every instance regardless of owner tier', () => {
    const ctx = context({ is_admin: true, tier: 2, group_ids: [] });
    expect(mayControlInstance(ctx, instance())).toBe(true);
    expect(mayControlInstance(ctx, instance({ owner_group_ids: ['g9'] }))).toBe(true);
    expect(mayControlInstance(ctx, instance({ owner_group_ids: ['g9'], owner_tier: 2 }))).toBe(true);
  });

  it('does not let a group instance manager control a same-tier or higher owner', () => {
    const manager = context({ can_manage_group_instances: true, tier: 1, group_ids: ['g1'] });
    expect(mayControlInstance(manager, instance({ owner_group_ids: ['g1'], owner_tier: 1 }))).toBe(false);
    expect(mayControlInstance(manager, instance({ owner_group_ids: ['g1'], owner_tier: 2 }))).toBe(false);
  });

  it('lets a group instance manager control a strictly lower-tier owner in a shared group', () => {
    const manager = context({ can_manage_group_instances: true, tier: 1, group_ids: ['g1'] });
    expect(mayControlInstance(manager, instance({ owner_group_ids: ['g1'], owner_tier: 0 }))).toBe(true);
    const admin = context({ can_manage_group_instances: true, tier: 2, group_ids: ['g1'] });
    expect(mayControlInstance(admin, instance({ owner_group_ids: ['g1'], owner_tier: 1 }))).toBe(true);
  });

  it('lets a group instance manager control same-group lower-tier owners only', () => {
    const ctx = context({ can_manage_group_instances: true, tier: 1 });
    expect(mayControlInstance(ctx, instance({ owner_group_ids: ['g1'] }))).toBe(true);
    expect(mayControlInstance(ctx, instance({ owner_group_ids: ['g1', 'g2'] }))).toBe(true);
    expect(mayControlInstance(ctx, instance({ owner_group_ids: ['g2'] }))).toBe(false);
    expect(mayControlInstance(ctx, instance({ owner_group_ids: [] }))).toBe(false);
  });

  it('does not let a plain user control anyone elses instance', () => {
    const ctx = context({ group_ids: ['g1'] });
    expect(mayControlInstance(ctx, instance({ owner_id: 'me' }))).toBe(true);
    expect(mayControlInstance(ctx, instance({ owner_id: 'u-other', owner_group_ids: ['g1'] }))).toBe(false);
  });

  it('treats a missing owner_group_ids field as having no groups', () => {
    const legacy = instance({ owner_group_ids: ['g1'] });
    delete (legacy as { owner_group_ids?: string[] }).owner_group_ids;
    const manager = context({ can_manage_group_instances: true, tier: 1 });
    const owner = context();
    expect(mayControlInstance(manager, legacy)).toBe(false);
    expect(mayControlInstance(owner, legacy)).toBe(false);
  });

  it('returns false without an authenticated context', () => {
    expect(mayControlInstance(null, instance({ owner_id: 'me' }))).toBe(false);
  });
});

describe('mayLaunchTemplate', () => {
  it('honors the effective allowed template whitelist', () => {
    const ctx = context({ allowed_template_ids: ['t1'] });
    expect(mayLaunchTemplate(ctx, template({ id: 't1' }))).toBe(true);
    expect(mayLaunchTemplate(ctx, template({ id: 't2' }))).toBe(false);
  });

  it('does not let the system admin launch a template outside the whitelist', () => {
    const ctx = context({ is_admin: true, tier: 2, allowed_template_ids: ['t1'] });
    expect(mayLaunchTemplate(ctx, template({ id: 't1' }))).toBe(true);
    expect(mayLaunchTemplate(ctx, template({ id: 'unlisted' }))).toBe(false);
  });

  it('lets anyone launch a public template outside the whitelist', () => {
    const ctx = context({ allowed_template_ids: ['t1'] });
    expect(mayLaunchTemplate(ctx, template({ id: 't9', visibility: 'public' }))).toBe(true);
    const admin = context({ is_admin: true, tier: 2, allowed_template_ids: ['t1'] });
    expect(mayLaunchTemplate(admin, template({ id: 't9', visibility: 'public' }))).toBe(true);
  });

  it('does not launch a hidden template the API excluded from the whitelist', () => {
    // The API strips hidden templates from allowed_template_ids, so the
    // whitelist check alone rejects them (no client-side visibility branch).
    const ctx = context({ allowed_template_ids: ['t2'] });
    expect(mayLaunchTemplate(ctx, template({ id: 't1', visibility: 'hidden' }))).toBe(false);
  });

  it('does not let the system admin launch a hidden template the API excluded', () => {
    const ctx = context({ is_admin: true, tier: 2, allowed_template_ids: ['t2'] });
    expect(mayLaunchTemplate(ctx, template({ id: 't1', visibility: 'hidden' }))).toBe(false);
  });

  it('treats a missing visibility as private', () => {
    const legacy = template({ id: 't1' });
    delete (legacy as { visibility?: string }).visibility;
    const ctx = context({ allowed_template_ids: ['t1'] });
    expect(mayLaunchTemplate(ctx, legacy)).toBe(true);
    expect(mayLaunchTemplate(ctx, template({ id: 't9' }))).toBe(false);
  });

  it('returns false without an authenticated context', () => {
    expect(mayLaunchTemplate(null, template())).toBe(false);
  });
});

describe('mayManageUsers', () => {
  it('is true for a can_manage_users holder and the system admin', () => {
    expect(mayManageUsers(context({ can_manage_users: true }))).toBe(true);
    expect(mayManageUsers(context({ is_admin: true, tier: 2 }))).toBe(true);
    expect(mayManageUsers(context())).toBe(false);
    expect(mayManageUsers(null)).toBe(false);
  });
});

describe('mayCreateTemplate', () => {
  it('is true for a can_create_template holder and the system admin', () => {
    expect(mayCreateTemplate(context({ can_create_template: true }))).toBe(true);
    expect(mayCreateTemplate(context({ is_admin: true, tier: 2 }))).toBe(true);
    expect(mayCreateTemplate(context())).toBe(false);
    expect(mayCreateTemplate(null)).toBe(false);
  });
});

describe('mayEditTemplate', () => {
  it('lets a template creator edit only their own templates', () => {
    const creator = context({ can_create_template: true, user_id: 'me' });
    expect(mayEditTemplate(creator, template({ owner_id: 'me' }))).toBe(true);
    expect(mayEditTemplate(creator, template({ owner_id: 'someone-else' }))).toBe(false);
  });

  it('does not let a non-creator edit even their own template', () => {
    const ctx = context({ can_create_template: false, user_id: 'me' });
    expect(mayEditTemplate(ctx, template({ owner_id: 'me' }))).toBe(false);
  });

  it('lets the system admin edit any template', () => {
    const admin = context({ is_admin: true, tier: 2 });
    expect(mayEditTemplate(admin, template({ owner_id: 'someone-else' }))).toBe(true);
  });

  it('returns false without an authenticated context', () => {
    expect(mayEditTemplate(null, template({ owner_id: 'me' }))).toBe(false);
  });
});
