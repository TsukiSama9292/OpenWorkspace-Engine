export type RemoteType = 'kasmvnc' | 'ttyd' | 'jupyter';

export type TimeoutAction = 'remove' | 'stop' | 'pause';

export type TemplateVisibility = 'public' | 'private' | 'hidden';

export interface Template {
  id: string;
  name: string;
  description: string;
  owner_id: string;
  image: string;
  cores: number;
  memory: number;
  gpu_count: number;
  docker_registry: string;
  remote_type: RemoteType;
  persistent_storage_path: string;
  container_runtime: string;
  max_run_seconds: number | null;
  timeout_action: TimeoutAction;
  keep_time_seconds: number | null;
  keep_time_action: TimeoutAction;
  network_bandwidth_up_mbps: number;
  network_bandwidth_down_mbps: number;
  docker_in_instance: boolean;
  visibility: TemplateVisibility;
  run_config: Record<string, unknown>;
  exec_config: Record<string, unknown>;
  volume_mappings: Record<string, string>;
  instance_count?: number;
  created_at: string;
  updated_at: string;
}

export interface Instance {
  id: string;
  name: string;
  template_id: string;
  template_name: string;
  remote_type: 'kasmvnc' | 'ttyd' | 'jupyter';
  owner_id: string;
  owner_username: string;
  owner_group_ids: string[];
  owner_tier: number;
  status: 'running' | 'stopped' | 'paused' | 'error' | 'starting';
  instance_number: number;
  container_id: string;
  mount_persistent: boolean;
  resolved_volume_host_path?: string;
  access_token?: string;
  access_password?: string;
  auto_sleeps_at?: string | null;
  timeout_action?: TimeoutAction | null;
  keep_time_deadline?: string | null;
  keep_time_seconds?: number | null;
  keep_time_action?: TimeoutAction | null;
  created_at: string;
  updated_at: string;
}

export interface VncSettings {
  quality: number;
  compression: number;
  viewOnly: boolean;
  clipboardSync: boolean;
  scaleViewport: boolean;
}

export type PreflightRejectionScope = 'template_not_allowed' | 'user_instance' | 'host_instance';

export interface PreflightRejection {
  scope: PreflightRejectionScope;
  current: number;
  limit: number;
  requested: number;
}

const PREFLIGHT_SCOPES: PreflightRejectionScope[] = ['template_not_allowed', 'user_instance', 'host_instance'];

export function isPreflightRejection(value: unknown): value is PreflightRejection {
  if (!value || typeof value !== 'object') return false;
  const v = value as Record<string, unknown>;
  return (
    typeof v.scope === 'string' &&
    PREFLIGHT_SCOPES.includes(v.scope as PreflightRejectionScope) &&
    typeof v.current === 'number' &&
    typeof v.limit === 'number' &&
    typeof v.requested === 'number'
  );
}

export interface EffectiveContext {
  user_id: string;
  username: string;
  is_admin: boolean;
  tier: number;
  can_create_template: boolean;
  can_manage_users: boolean;
  can_manage_group_instances: boolean;
  can_manage_docker: boolean;
  can_manage_registry: boolean;
  effective_max_instances: number;
  allowed_template_ids: string[];
  group_ids: string[];
  direct_max_instances: number | null;
}

export const TIER_USER = 0;
export const TIER_MANAGER = 1;
export const TIER_ADMIN = 2;

export interface Group {
  id: string;
  name: string;
  description: string | null;
  kind: 'admin' | 'manager' | 'user' | null;
  can_create_template: boolean;
  can_manage_users: boolean;
  can_manage_group_instances: boolean;
  can_manage_docker: boolean;
  can_manage_registry: boolean;
  max_instances: number | null;
  template_ids: string[];
}

export type GroupInput = Omit<Group, 'id' | 'kind'>;

export interface GroupMembershipPayload {
  group_ids: string[];
}

export interface DirectMaxInstancesPayload {
  direct_max_instances: number | null;
}

export type UserPolicyUpdate = Partial<GroupMembershipPayload & DirectMaxInstancesPayload>;

export type PersistentVolumeStatus = 'active' | 'orphaned';

export interface PersistentVolume {
  id: string;
  host_path: string;
  owner_id: string | null;
  owner_username: string | null;
  status: PersistentVolumeStatus;
  created_at: string;
}

export interface ApiResult<T> {
  data?: T;
  error?: string;
  status?: number;
  rejection?: PreflightRejection;
}
