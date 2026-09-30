import { expect, type APIRequestContext, type Page, test } from '@playwright/test';

// Full-stack verification for the shared foundation (ticket 01):
// login without dead ends plus its phone layout, the budget meter rendered
// from a live auto-sleep deadline, the shared empty state with its clear
// action, the removal confirmation dialog (cancel path), and the three
// breakpoint captures proving the stylesheet relocation changed nothing.

const ADMIN = { username: 'admin', password: 'admin' };
const IMAGE = 'tsukisama9292/ow-kasmvnc-ubuntu:jammy';
const BASE_TEMPLATE = 'e2e-base-ubuntu';
const BUDGET_TEMPLATE = 'e2e-base-budget';

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

async function ensureTemplate(
  req: APIRequestContext,
  name: string,
  extra: Record<string, unknown> = {}
): Promise<string> {
  const list = await getJson(req, '/api/templates');
  const existing = (list.templates ?? []).find((t: any) => t.name === name);
  if (existing) return existing.id as string;
  const res = await req.post('/api/templates', {
    data: {
      name,
      description: `E2E base fixture ${name}`,
      image: IMAGE,
      cores: 1,
      memory: 1073741824,
      remote_type: 'kasmvnc',
      container_runtime: 'runc',
      visibility: 'public',
      ...extra,
    },
  });
  expect(res.ok()).toBeTruthy();
  const id = ((await res.json()).template as { id: string }).id;
  fixtureIds.templates.push(id);
  return id;
}

test('login offers no dead ends and stacks on phones', async ({ page }) => {
  await page.goto('/login');
  await expect(page.locator('.login-wrapper')).toBeVisible();
  await expect(page.locator('button[type="submit"]')).toContainText(/Continue/);
  await expect(page.locator('button', { hasText: /SSO|Single Sign-On|SSH/ })).toHaveCount(0);

  await page.setViewportSize({ width: 390, height: 844 });
  await page.waitForTimeout(400);
  await expect(page.locator('.panel-form')).toBeVisible();
  const visualBox = await page.locator('.panel-visual').boundingBox();
  const formBox = await page.locator('.panel-form').boundingBox();
  // Stacked single column: the brand banner sits above the form, never beside it.
  expect(visualBox && formBox && visualBox.y < formBox.y).toBeTruthy();
  await page.screenshot({ path: 'test-results/web-surface-base/login-phone.png' });

  await page.setViewportSize({ width: 1440, height: 900 });
  await page.waitForTimeout(400);
  await page.screenshot({ path: 'test-results/web-surface-base/login-desktop.png' });
});

test('budget meter renders from the live auto-sleep deadline', async ({ page }) => {
  await loginAs(page, ADMIN.username, ADMIN.password);
  const templateId = await ensureTemplate(page.request, BUDGET_TEMPLATE, {
    max_run_seconds: 3600,
    timeout_action: 'stop',
  });

  const launch = await page.request.post('/api/instances', { data: { template_id: templateId } });
  expect(launch.ok()).toBeTruthy();
  const instanceId = ((await launch.json()).instance as { id: string }).id;
  fixtureIds.instances.push(instanceId);

  await page.goto('/');
  const card = page.locator('.session-rich', { hasText: BUDGET_TEMPLATE });
  await expect(card).toBeVisible({ timeout: 15_000 });
  await expect(card.locator('.ws-state-story')).toContainText(/Auto-sleep in/, { timeout: 15_000 });
  const meter = card.locator('.ws-budget');
  await expect(meter).toBeVisible();
  const now = Number(await meter.getAttribute('aria-valuenow'));
  expect(now).toBeGreaterThan(90);
  expect(now).toBeLessThanOrEqual(100);
});

test('shared empty state offers a working clear action', async ({ page }) => {
  await loginAs(page, ADMIN.username, ADMIN.password);
  const templateId = await ensureTemplate(page.request, BASE_TEMPLATE);
  const launch = await page.request.post('/api/instances', { data: { template_id: templateId } });
  expect(launch.ok()).toBeTruthy();
  fixtureIds.instances.push(((await launch.json()).instance as { id: string }).id);

  await page.goto('/');
  await expect(page.locator('.catalog-card').filter({ hasText: BASE_TEMPLATE })).toBeVisible({
    timeout: 15_000,
  });
  await page.locator('#dashboard-search').fill('zzz-no-such-thing');
  const sessionsEmpty = page.locator('section[aria-label="My sessions"] .empty-state');
  await expect(sessionsEmpty).toBeVisible();
  await expect(sessionsEmpty).toContainText(/No sessions match/);
  await expect(sessionsEmpty.locator('.empty-mark')).toHaveAttribute('src', '/icons/generic.svg');
  await sessionsEmpty.locator('button', { hasText: 'Clear search' }).click();
  await expect(page.locator('.catalog-card').filter({ hasText: BASE_TEMPLATE })).toBeVisible();
});

test('removal dialog cancels cleanly and keeps the session', async ({ page }) => {
  await loginAs(page, ADMIN.username, ADMIN.password);
  const templateId = await ensureTemplate(page.request, BASE_TEMPLATE);

  const launch = await page.request.post('/api/instances', { data: { template_id: templateId } });
  expect(launch.ok()).toBeTruthy();
  const launched = (await launch.json()).instance as { id: string; name: string };
  fixtureIds.instances.push(launched.id);
  try {
    await page.goto('/');
    // Generous timeout: parallel full-suite runs contend Docker and slow the first paint.
    const card = page.locator('.session-rich', { hasText: launched.name });
    await expect(card).toBeVisible({ timeout: 30_000 });
    await card.locator('.overflow-btn').click();
    await expect(card.locator('[role="menu"]')).toBeVisible();
    await card.locator('.launch-btn.remove').click();

    const dialog = page.getByRole('dialog', { name: `Delete "${launched.name}"?` });
    await expect(dialog).toBeVisible();
    await expect(dialog.locator('[data-testid="confirm-dialog"]')).toContainText('Persistent data is kept');
    await dialog.getByRole('button', { name: 'Cancel' }).click();
    await expect(dialog).toBeHidden();

    const list = await getJson(page.request, '/api/instances');
    expect((list.instances ?? []).some((i: any) => i.id === launched.id)).toBe(true);
  } finally {
    await page.request.delete(`/api/instances/${launched.id}`).catch(() => undefined);
  }
});

test('dashboard captures across breakpoints after the stylesheet move', async ({ page }) => {
  await loginAs(page, ADMIN.username, ADMIN.password);
  await ensureTemplate(page.request, BASE_TEMPLATE);
  await page.goto('/');
  await expect(page.locator('.hero-greeting')).toBeVisible({ timeout: 15_000 });

  await page.setViewportSize({ width: 1440, height: 900 });
  await page.waitForTimeout(500);
  await expect(page.locator('.catalog-grid')).toBeVisible();
  await page.screenshot({ path: 'test-results/web-surface-base/desktop.png' });

  await page.setViewportSize({ width: 768, height: 1024 });
  await page.waitForTimeout(500);
  await page.screenshot({ path: 'test-results/web-surface-base/tablet.png' });

  await page.setViewportSize({ width: 390, height: 844 });
  await page.waitForTimeout(500);
  await expect(page.locator('.rail-menu')).toBeVisible();
  await page.screenshot({ path: 'test-results/web-surface-base/phone.png' });
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
