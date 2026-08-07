import { writable, derived } from 'svelte/store';
import { api } from '$lib/api/client';
import { TIER_USER, type EffectiveContext } from '$lib/types';

type ContextFlag =
  | 'can_create_template'
  | 'can_manage_users'
  | 'can_manage_group_instances'
  | 'can_manage_docker'
  | 'can_manage_registry'
  | 'can_view_monitoring';

function adminOr(context: EffectiveContext | null, flag: ContextFlag): boolean {
  if (!context) return false;
  if (context.is_admin) return true;
  return context[flag];
}

function createAuthStore() {
  const { subscribe, set } = writable<EffectiveContext | null>(null);

  return {
    subscribe,
    login: async (username: string, password: string): Promise<boolean> => {
      const res = await api.post<{ context: EffectiveContext }>('/auth/login', { username, password });
      if (res.data) {
        set(res.data.context);
        return true;
      }
      return false;
    },
    logout: async (): Promise<void> => {
      await api.post('/auth/logout');
      set(null);
    },
    check: async (): Promise<void> => {
      const res = await api.get<{ context: EffectiveContext }>('/auth/me');
      if (res.data) {
        set(res.data.context);
      } else {
        set(null);
      }
    }
  };
}

export const auth = createAuthStore();
export const isAuthenticated = derived(auth, ($auth) => $auth !== null);
export const isAdmin = derived(auth, ($auth) => $auth?.is_admin === true);
export const userTier = derived(auth, ($auth) => $auth?.tier ?? TIER_USER);
export const canCreateTemplate = derived(auth, ($auth) => adminOr($auth, 'can_create_template'));
export const canManageUsers = derived(auth, ($auth) => adminOr($auth, 'can_manage_users'));
export const canManageGroupInstances = derived(auth, ($auth) => adminOr($auth, 'can_manage_group_instances'));
export const canManageDocker = derived(auth, ($auth) => adminOr($auth, 'can_manage_docker'));
export const canManageRegistry = derived(auth, ($auth) => adminOr($auth, 'can_manage_registry'));
export const canViewMonitoring = derived(auth, ($auth) => adminOr($auth, 'can_view_monitoring'));
export const effectiveMaxInstances = derived(auth, ($auth) => $auth?.effective_max_instances ?? 0);
export const allowedTemplateIds = derived(auth, ($auth) => $auth?.allowed_template_ids ?? []);
