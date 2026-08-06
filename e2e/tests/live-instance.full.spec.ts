import { expect, type Page, test } from '@playwright/test';

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

test('launches a real instance, opens the KasmVNC viewer over the proxied WebSocket, and tears down', async ({
  page,
}) => {
  await loginAsAdmin(page);

  const quickLaunch = page.locator('.template-card').first();
  await quickLaunch.waitFor({ state: 'attached', timeout: 15_000 }).catch(() => {});
  const launchable = page.locator('.template-card:not(.locked)').first();
  if ((await launchable.count()) === 0) {
    test.skip(true, 'No launchable templates in the dev stack — create one on the Templates tab first.');
  }

  const wsUrls: string[] = [];
  page.on('websocket', (ws) => {
    wsUrls.push(ws.url());
  });

  let instanceId = '';
  try {
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
  }
});
