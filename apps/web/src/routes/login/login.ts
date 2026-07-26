import { auth } from '$lib/stores/auth';
import { goto } from '$app/navigation';

export async function handleLogin(username: string, password: string): Promise<boolean> {
  const success = await auth.login(username, password);
  if (success) goto('/');
  return success;
}
