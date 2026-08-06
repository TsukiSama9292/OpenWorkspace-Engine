import type { FullConfig } from '@playwright/test';

const TIMEOUT_MS = 5_000;

async function reachable(url: string): Promise<boolean> {
  const controller = new AbortController();
  const timer = setTimeout(() => controller.abort(), TIMEOUT_MS);
  try {
    const res = await fetch(url, { signal: controller.signal, redirect: 'manual' });
    return res.status >= 100 && res.status < 500;
  } catch {
    return false;
  } finally {
    clearTimeout(timer);
  }
}

export default async function globalSetup(config: FullConfig): Promise<void> {
  const baseURL = config.projects[0]?.use.baseURL ?? 'http://localhost';

  const rootOk = await reachable(`${baseURL}/`);
  const apiOk = await reachable(`${baseURL}/api/auth/me`);

  if (!rootOk || !apiOk) {
    throw new Error(
      `E2E dev stack not reachable at ${baseURL} (root=${rootOk ? 'up' : 'down'}, ` +
        `/api/auth/me=${apiOk ? 'up' : 'down'}).\n` +
        `The E2E suites target the already-running dev stack (Traefik :80, full ` +
        `browser → Traefik → SPA/API/instance path) and never start their own servers. ` +
        `Start the stack first with: pnpm run dev`
    );
  }
}
