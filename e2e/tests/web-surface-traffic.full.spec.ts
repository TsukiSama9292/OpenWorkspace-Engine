import { expect, type APIRequestContext, type Page, test } from '@playwright/test';

// Full-stack verification for the traffic pages (ticket 02): the templates
// tab speaking the catalog language with local marks and locked reasons, the
// sessions table leading with an Open action, the volumes tab with its
// ownership table and shared empty states, plus a phone capture.

const ADMIN = { username: 'admin', password: 'admin' };
const IMAGE = 'tsukisama9292/ow-kasmvnc-ubuntu:jammy';
const TRAFFIC_TEMPLATE = 'e2e-traffic-ubuntu';

const fixtureIds: { templates: string[]; instances: string[] } = {
  templates: [],
  instances: [],
};

async function loginAs(page: Page, username: string, password: string): Promise<void> {
  await page.goto('/login');
  await page.locator('#acc').fill(username);
  await page.locator('#pwd').fill(password);
  await Promise.all([
    page.waitForURL(/\/$/, { timeout: 15_000 }),
    page.locator('button[type="submit"]').click(),
  ]);
  await expect(page.locator('.dashboard')).toBeVisible();
}

async function getJson(req: APIRequestContext, url: string): Promise<any> {
  const res = await req.get(url);
  expect(res.ok()).toBeTruthy();
  return res.json();
}

async function ensureTemplate(req: APIRequestContext, name: string): Promise<string> {
  const list = await getJson(req, '/api/templates');
  const existing = (list.templates ?? []).find((t: any) => t.name === name);
  if (existing) return existing.id as string;
  const res = await req.post('/api/templates', {
    data: {
      name,
      description: 'E2E traffic fixture template',
      image: IMAGE,
      cores: 2,
      memory: 4294967296,
      remote_type: 'kasmvnc',
      container_runtime: 'runc',
      visibility: 'public',
    },
  });
  expect(res.ok()).toBeTruthy();
  const id = ((await res.json()).template as { id: string }).id;
  fixtureIds.templates.push(id);
  return id;
}

async function openTab(page: Page, name: string): Promise<void> {
  await page.locator('.sidebar .nav-item').filter({ hasText: name }).click();
}

test('templates tab speaks the catalog language with local marks', async ({ page }) => {
  await loginAs(page, ADMIN.username, ADMIN.password);
  await ensureTemplate(page.request, TRAFFIC_TEMPLATE);

  // The dashboard loads its template list once at page load, so reload after
  // creating the fixture before asserting on the tab.
  await page.goto('/');
  await expect(page.locator('.hero-greeting')).toBeVisible({ timeout: 15_000 });
  await openTab(page, 'Templates');
  const card = page.locator('.catalog-card').filter({ hasText: TRAFFIC_TEMPLATE });
  await expect(card).toBeVisible({ timeout: 15_000 });
  await expect(card.locator('img.catalog-mark')).toHaveAttribute('src', '/icons/ubuntu.svg');
  // Computed-style guard: the catalog language lives in the shared sheet, so
  // it must resolve on child panels too (a scoped-only rule renders a giant
  // unstyled mark here instead).
  await expect(card.locator('img.catalog-mark')).toHaveCSS('width', '52px');
  await expect(card.locator('.catalog-cover').first()).toHaveCSS('height', '112px');
  await expect(card).toContainText(/E2E traffic fixture template/);
  await expect(card).toContainText(/2 CPU/);
  await expect(card.locator('.launchable-badge')).toContainText(/May launch/);

  const bodyText = await page.locator('.main-content').textContent();
  expect(bodyText).not.toMatch(/🐧|🐍|🧠|⚙|📦/);

  await page.setViewportSize({ width: 390, height: 844 });
  await page.waitForTimeout(500);
  await expect(card).toBeVisible();
  await page.screenshot({ path: 'test-results/web-surface-traffic/templates-phone.png' });
});

test('sessions table leads with an Open action and a Budget column', async ({ page }) => {
  await loginAs(page, ADMIN.username, ADMIN.password);
  const templateId = await ensureTemplate(page.request, TRAFFIC_TEMPLATE);

  const launch = await page.request.post('/api/instances', { data: { template_id: templateId } });
  expect(launch.ok()).toBeTruthy();
  const instanceId = ((await launch.json()).instance as { id: string }).id;
  fixtureIds.instances.push(instanceId);
  try {
    await openTab(page, 'Sessions');
    const row = page.locator('.instances-table tbody tr').first();
    await expect(row).toBeVisible({ timeout: 15_000 });
    const open = row.locator('a', { hasText: 'Open' });
    await expect(open).toBeVisible();
    expect(await open.getAttribute('href')).toMatch(/^\/(kasmvnc|open)\//);
    await expect(page.locator('.instances-table th', { hasText: 'Budget' })).toBeVisible();
    // State badges stay subtle: translucent wash, no neon fill or glow.
    await expect(row.locator('.status-badge')).toHaveCSS('box-shadow', 'none');

    await page.setViewportSize({ width: 390, height: 844 });
    await page.waitForTimeout(500);
    await expect(row).toBeVisible();
    await page.screenshot({ path: 'test-results/web-surface-traffic/sessions-phone.png' });
  } finally {
    await page.request.delete(`/api/instances/${instanceId}`).catch(() => undefined);
  }
});

test('volumes tab shows ownership with shared empty states', async ({ page }) => {
  await loginAs(page, ADMIN.username, ADMIN.password);

  await openTab(page, 'Volumes');
  await expect(page.locator('.panel-head-title')).toContainText(/Orphaned Volumes/, {
    timeout: 15_000,
  });
  await expect(page.locator('.empty-state')).toContainText(/No orphaned volumes/);
  await expect(page.locator('.empty-mark')).toHaveAttribute('src', '/icons/generic.svg');

  await page.setViewportSize({ width: 390, height: 844 });
  await page.waitForTimeout(500);
  await expect(page.locator('.panel-head-title')).toBeVisible();
  await page.screenshot({ path: 'test-results/web-surface-traffic/volumes-phone.png' });
});

test.afterAll(async ({ request }) => {
  await request.post('/api/auth/login', { data: { username: ADMIN.username, password: ADMIN.password } });
  for (const id of fixtureIds.instances) {
    await request.delete(`/api/instances/${id}`).catch(() => undefined);
  }
  for (const id of fixtureIds.templates) {
    await request.delete(`/api/templates/${id}`).catch(() => undefined);
  }
});
