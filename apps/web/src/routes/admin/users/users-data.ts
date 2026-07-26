import { api } from '$lib/api/client';
import { get } from 'svelte/store';
import { isAdmin } from '$lib/stores/auth';
import { browser } from '$app/environment';
import { goto } from '$app/navigation';

export interface AdminUser {
  id: string;
  username: string;
  role: string;
  created_at: string;
}

export async function loadUsers(): Promise<AdminUser[]> {
  if (browser && !get(isAdmin)) {
    goto('/');
    return [];
  }
  const res = await api.get<{ users: AdminUser[] }>('/users');
  return res.data?.users ?? [];
}
