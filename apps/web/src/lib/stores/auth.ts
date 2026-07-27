import { writable, derived } from 'svelte/store';
import { api } from '$lib/api/client';
import type { User } from '$lib/types';

function createAuthStore() {
  const { subscribe, set } = writable<User | null>(null);

  return {
    subscribe,
    login: async (username: string, password: string): Promise<boolean> => {
      const res = await api.post<{ user: User }>('/auth/login', { username, password });
      if (res.data) {
        set(res.data.user);
        return true;
      }
      return false;
    },
    logout: async (): Promise<void> => {
      await api.post('/auth/logout');
      set(null);
    },
    check: async (): Promise<void> => {
      const res = await api.get<{ user: User }>('/auth/me');
      if (res.data) {
        set(res.data.user);
      } else {
        set(null);
      }
    }
  };
}

export const auth = createAuthStore();
export const isAuthenticated = derived(auth, ($auth) => $auth !== null);
export const isAdmin = derived(auth, ($auth) => $auth?.role === 'admin');
export const isManager = derived(auth, ($auth) => $auth?.role === 'admin' || $auth?.role === 'manager');
