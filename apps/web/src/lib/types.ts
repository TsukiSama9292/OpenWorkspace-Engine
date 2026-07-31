export type Role = 'admin' | 'manager' | 'user';

export interface User {
  id: string;
  username: string;
  role: Role;
}

export type RemoteType = 'kasmvnc' | 'ttyd' | 'jupyter';

export type TimeoutAction = 'remove' | 'stop' | 'pause';

export interface Template {
  id: string;
  name: string;
  description: string;
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
  owner_role: Role;
  status: 'running' | 'stopped' | 'paused' | 'error' | 'starting';
  instance_number: number;
  container_id: string;
  mount_persistent: boolean;
  resolved_volume_host_path?: string;
  access_token?: string;
  access_password?: string;
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

export interface ApiResult<T> {
  data?: T;
  error?: string;
}
