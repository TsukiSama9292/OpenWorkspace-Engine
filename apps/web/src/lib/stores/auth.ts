import { writable, derived } from 'svelte/store';
import { api } from '$lib/api';

interface User {
  id: string;
  username: string;
  role: 'admin' | 'user';
}

function createAuthStore() {
  const { subscribe, set, update } = writable<User | null>(null);

  return {
    subscribe,
    login: async (username: string, password: string) => {
      const res = await api.post<{ user: User }>('/auth/login', { username, password });
      if (res.data) {
        set(res.data.user);
        return true;
      }
      return false;
    },
    logout: async () => {
      await api.post('/auth/logout');
      set(null);
    },
    check: async () => {
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
