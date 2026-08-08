import { expect, type APIRequestContext, type Page, test } from '@playwright/test';

const TEMPLATE_NAME = 'e2e-live-instance-template';
const IMAGE = 'tsukisama9292/ow-kasmvnc-ubuntu:jammy';

async function loginAsAdmin(page: Page): Promise<void> {
  await page.goto('/login');
  await page.locator('#acc').fill('admin');
  await page.locator('#pwd').fill('admin');
  await Promise.all([
    page.waitForURL(/\/$/, { timeout: 15_000 }),
    page.locator('button[type="submit"]').click(),
  ]);
  await expect(page.locator('.dashboard')).toBeVisible();
}

async function ensureTemplate(req: APIRequestContext): Promise<string> {
  const list = await req.get('/api/templates');
  const existing = ((await list.json()).templates ?? []).find((t: any) => t.name === TEMPLATE_NAME);
  if (existing) return existing.id as string;

  const res = await req.post('/api/templates', {
    data: {
      name: TEMPLATE_NAME,
      description: 'E2E live-instance fixture',
      image: IMAGE,
      cores: 1,
      memory: 1073741824,
      remote_type: 'kasmvnc',
      container_runtime: 'runc',
      visibility: 'public',
    },
  });
  expect(res.ok()).toBeTruthy();
  return ((await res.json()).template as { id: string }).id;
}

test('launches a real instance, opens the KasmVNC viewer over the proxied WebSocket, and tears down', async ({
  page,
}) => {
  await loginAsAdmin(page);

  let templateId = '';
  let instanceId = '';
  try {
    templateId = await ensureTemplate(page.request);
    await page.goto('/');

    const quickLaunch = page.locator('.template-card').first();
    await quickLaunch.waitFor({ state: 'attached', timeout: 15_000 });
    const launchable = page.locator('.template-card:not(.locked)').first();
    if ((await launchable.count()) === 0) {
      test.skip(true, 'No launchable templates in the dev stack — create one on the Templates tab first.');
    }

    const wsUrls: string[] = [];
    page.on('websocket', (ws) => {
      wsUrls.push(ws.url());
    });

    await quickLaunch.click();
    await expect(page.locator('.modal-confirm')).toBeVisible();
    await page.locator('.modal-confirm').click();

    await page.waitForURL(/\/instances\/[^/]+/, { timeout: 30_000 });
    instanceId = page.url().split('/instances/')[1]?.split('/')[0] ?? '';
    expect(instanceId).toMatch(/^[0-9a-f-]{36}$/);

    await page.waitForURL(/\/kasmvnc\/[^/]+\/?/, { timeout: 180_000 });
    expect(page.url()).toContain('/kasmvnc/');

    await expect
      .poll(() => wsUrls.filter((u) => u.includes('/websockify')).length, { timeout: 60_000 })
      .toBeGreaterThan(0);
    const wsUrl = wsUrls.find((u) => u.includes('/websockify'));
    expect(wsUrl).toBeTruthy();
    expect(new URL(wsUrl as string).pathname).toMatch(/^\/kasmvnc\/[^/]+\/websockify/);

    await expect(page.locator('canvas[tabindex="-1"]')).toBeVisible({ timeout: 60_000 });
  } finally {
    if (instanceId) {
      const del = await page.request.delete(`/api/instances/${instanceId}`);
      expect(del.ok()).toBeTruthy();

      await expect
        .poll(
          async () => {
            const list = await page.request.get('/api/instances');
            const body = (await list.json()) as { instances: { id: string }[] };
            return (body.instances ?? []).some((i) => i.id === instanceId);
          },
          { timeout: 15_000 }
        )
        .toBe(false);
    }
    if (templateId) {
      await page.request.delete(`/api/templates/${templateId}`);
    }
  }
});
