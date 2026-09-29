import { expect, type APIRequestContext, type Browser, type Page, test } from '@playwright/test';

// Full-stack verification for the sidebar-dashboard redesign (ticket 03):
// hero search + quota, rich session cards + lifecycle with removal confirm,
// catalog artwork served locally, locked-template denial, three breakpoint
// captures, and untouched management surfaces + permission gating.

const ADMIN = { username: 'admin', password: 'admin' };
const IMAGE = 'tsukisama9292/ow-kasmvnc-ubuntu:jammy';
const UBUNTU_TEMPLATE = 'e2e-dash-ubuntu';
const RUST_TEMPLATE = 'e2e-dash-rust';
const PRIVATE_TEMPLATE = 'e2e-dash-private';
const DASH_GROUP = 'e2e-dash-group';
const DASH_USER = { username: 'e2e_dash_user', password: 'pw123456' };

const fixtureIds: { groups: string[]; users: string[]; templates: string[] } = {
  groups: [],
  users: [],
  templates: [],
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
  visibility: 'public' | 'private'
): Promise<string> {
  const list = await getJson(req, '/api/templates');
  const existing = (list.templates ?? []).find((t: any) => t.name === name);
  if (existing) return existing.id as string;
  const res = await req.post('/api/templates', {
    data: {
      name,
      description: `E2E dashboard fixture ${name}`,
      image: IMAGE,
      cores: 1,
      memory: 1073741824,
      remote_type: 'kasmvnc',
      container_runtime: 'runc',
      visibility,
    },
  });
  expect(res.ok()).toBeTruthy();
  const id = ((await res.json()).template as { id: string }).id;
  fixtureIds.templates.push(id);
  return id;
}

async function ensureGroup(req: APIRequestContext, name: string): Promise<string> {
  const list = await getJson(req, '/api/groups');
  const existing = (list.groups ?? []).find((g: any) => g.name === name);
  if (existing) return existing.id as string;
  const res = await req.post('/api/groups', {
    data: {
      name,
      description: 'E2E dashboard fixture',
      can_create_template: false,
      can_manage_users: false,
      can_manage_group_instances: false,
      can_manage_docker: false,
      can_manage_registry: false,
      can_view_monitoring: false,
      can_view_audit_logs: false,
      max_instances: 10,
      template_ids: [],
      billing_model: 'shared',
      pool_cpu_cores: 8,
      pool_memory_mb: 16384,
      pool_gpu_count: 1,
    },
  });
  expect(res.ok()).toBeTruthy();
  const id = ((await res.json()).group as { id: string }).id;
  fixtureIds.groups.push(id);
  return id;
}

async function ensureUser(
  req: APIRequestContext,
  username: string,
  password: string,
  groupIds: string[]
): Promise<string> {
  const users = await getJson(req, '/api/users');
  const existing = (users.users ?? []).find((u: any) => u.username === username);
  if (existing) return existing.id as string;
  const res = await req.post('/api/users', { data: { username, password, group_ids: groupIds } });
  expect(res.ok()).toBeTruthy();
  const id = ((await res.json()).user as { id: string }).id;
  fixtureIds.users.push(id);
  return id;
}

test('hero, rich cards, and locally served catalog artwork', async ({ page }) => {
  await loginAs(page, ADMIN.username, ADMIN.password);
  await ensureTemplate(page.request, UBUNTU_TEMPLATE, 'public');

  const iconResponses: { url: string; status: number; cache: string }[] = [];
  const thirdParty: string[] = [];
  page.on('response', (res) => {
    const url = res.url();
    if (url.includes('/icons/')) {
      iconResponses.push({
        url,
        status: res.status(),
        cache: res.headers()['cache-control'] ?? '',
      });
    }
    if (/^https?:\/\//.test(url) && !url.startsWith('http://localhost')) {
      thirdParty.push(url);
    }
  });

  await page.goto('/');
  await expect(page.locator('.hero-greeting')).toBeVisible({ timeout: 15_000 });
  await expect(page.locator('#dashboard-search')).toBeVisible();
  await expect(page.locator('.hero-totals')).toContainText(/sessions used/);
  await expect(page.locator('.section-title')).toHaveText(['My sessions', 'Template catalog']);

  const card = page.locator('.catalog-card').filter({ hasText: UBUNTU_TEMPLATE });
  await expect(card).toBeVisible({ timeout: 15_000 });
  const mark = card.locator('img.catalog-mark');
  await expect(mark).toHaveAttribute('src', '/icons/ubuntu.svg');
  await expect(card).toContainText(/CPU/);

  await page.waitForTimeout(2_000);
  expect(iconResponses.length).toBeGreaterThan(0);
  for (const r of iconResponses) {
    // 200 first paint, 304 revalidation — both prove local serving + caching.
    expect([200, 304]).toContain(r.status);
  }
  expect(thirdParty.filter((u) => /\.(png|svg|ico|jpg)$/.test(u))).toHaveLength(0);

  await page.setViewportSize({ width: 1440, height: 900 });
  await page.screenshot({ path: 'test-results/sidebar-dashboard/desktop.png' });
});

test('hero search filters sessions and templates', async ({ page }) => {
  await loginAs(page, ADMIN.username, ADMIN.password);
  await ensureTemplate(page.request, UBUNTU_TEMPLATE, 'public');
  await ensureTemplate(page.request, RUST_TEMPLATE, 'public');

  await page.goto('/');
  await expect(page.locator('.catalog-card').filter({ hasText: RUST_TEMPLATE })).toBeVisible({
    timeout: 15_000,
  });

  await page.locator('#dashboard-search').fill('rust');
  await expect(page.locator('.catalog-card').filter({ hasText: UBUNTU_TEMPLATE })).toHaveCount(0);
  await expect(page.locator('.catalog-card').filter({ hasText: RUST_TEMPLATE })).toBeVisible();

  await page.locator('#dashboard-search').fill('');
  await expect(page.locator('.catalog-card').filter({ hasText: UBUNTU_TEMPLATE })).toBeVisible();
});

test('session lifecycle: rich card, stop, and removal with confirmation', async ({ page }) => {
  await loginAs(page, ADMIN.username, ADMIN.password);
  const templateId = await ensureTemplate(page.request, UBUNTU_TEMPLATE, 'public');

  const launch = await page.request.post('/api/instances', { data: { template_id: templateId } });
  expect(launch.ok()).toBeTruthy();
  const launched = (await launch.json()).instance as { id: string };
  const instanceId = launched.id;
  try {
    await page.goto('/');
    const card = page.locator('.session-rich').first();
    await expect(card).toBeVisible({ timeout: 15_000 });
    await expect(card.locator('.ws-state-story')).toBeVisible();
    await expect(card.locator('.overflow-btn')).toHaveText(/More/);

    page.on('dialog', (d) => void d.accept());
    await card.locator('.overflow-btn').click();
    await expect(card.locator('[role="menu"]')).toBeVisible();
    await card.locator('.launch-btn.remove').click();
    await expect
      .poll(
        async () => {
          const list = await getJson(page.request, '/api/instances');
          return (list.instances ?? []).some((i: any) => i.id === instanceId);
        },
        { timeout: 30_000, intervals: [1_000] }
      )
      .toBe(false);
  } finally {
    await page.request.delete(`/api/instances/${instanceId}`).catch(() => undefined);
  }
});

test('locked template explains the cause and surfaces the denial notice', async ({ browser }: { browser: Browser }) => {
  const adminCtx = await browser.newContext();
  const adminPage = await adminCtx.newPage();
  await loginAs(adminPage, ADMIN.username, ADMIN.password);
  await ensureTemplate(adminPage.request, PRIVATE_TEMPLATE, 'private');
  const groupId = await ensureGroup(adminPage.request, DASH_GROUP);
  await ensureUser(adminPage.request, DASH_USER.username, DASH_USER.password, [groupId]);
  await adminCtx.close();

  const ctx = await browser.newContext();
  const page = await ctx.newPage();
  try {
    await loginAs(page, DASH_USER.username, DASH_USER.password);
    await page.goto('/');
    const card = page.locator('.catalog-card').filter({ hasText: PRIVATE_TEMPLATE });
    await expect(card).toBeVisible({ timeout: 15_000 });
    await expect(card).toContainText(/Not allowed|Hidden/);
    await card.locator('.catalog-launch').click();
    await expect(page.locator('[data-testid="rejection-notice"]')).toBeVisible({ timeout: 15_000 });
  } finally {
    await ctx.close();
  }
});

test('tablet and phone breakpoints with drawer navigation', async ({ page }) => {
  await loginAs(page, ADMIN.username, ADMIN.password);
  await ensureTemplate(page.request, UBUNTU_TEMPLATE, 'public');
  await page.goto('/');
  await expect(page.locator('.hero-greeting')).toBeVisible({ timeout: 15_000 });

  await page.setViewportSize({ width: 768, height: 1024 });
  await page.waitForTimeout(500);
  await page.screenshot({ path: 'test-results/sidebar-dashboard/tablet.png' });

  await page.setViewportSize({ width: 390, height: 844 });
  await page.waitForTimeout(500);
  await expect(page.locator('.rail-menu')).toBeVisible();
  await page.locator('.rail-menu').click();
  await expect(page.locator('.sidebar.drawer')).toBeVisible();
  await page.screenshot({ path: 'test-results/sidebar-dashboard/phone.png' });
});

test('management surfaces keep their arrangement and gating', async ({ browser }: { browser: Browser }) => {
  const setupCtx = await browser.newContext();
  const setupPage = await setupCtx.newPage();
  await loginAs(setupPage, ADMIN.username, ADMIN.password);
  const groupId = await ensureGroup(setupPage.request, DASH_GROUP);
  await ensureUser(setupPage.request, DASH_USER.username, DASH_USER.password, [groupId]);
  await setupCtx.close();
  void groupId;

  const userCtx = await browser.newContext();
  const userPage = await userCtx.newPage();
  try {
    await loginAs(userPage, DASH_USER.username, DASH_USER.password);
    await userPage.goto('/');
    await expect(userPage.locator('.dashboard')).toBeVisible();
    await expect(userPage.locator('.sidebar .nav-item').filter({ hasText: 'Groups' })).toHaveCount(0);
    await expect(userPage.locator('.sidebar .nav-item').filter({ hasText: 'Users' })).toHaveCount(0);
    await expect(userPage.locator('.sidebar .nav-item').filter({ hasText: 'Settings' })).toHaveCount(0);
  } finally {
    await userCtx.close();
  }

  const adminCtx = await browser.newContext();
  const adminPage = await adminCtx.newPage();
  try {
    await loginAs(adminPage, ADMIN.username, ADMIN.password);
    await adminPage.goto('/');
    await adminPage.locator('.sidebar .nav-item').filter({ hasText: 'Templates' }).click();
    await expect(adminPage.locator('.templates-header')).toBeVisible({ timeout: 15_000 });
    await expect(adminPage.locator('.btn-create')).toHaveText('+ New Template');
  } finally {
    await adminCtx.close();
  }
});

test.afterAll(async ({ request }) => {
  await request.post('/api/auth/login', { data: { username: ADMIN.username, password: ADMIN.password } });
  for (const id of fixtureIds.templates) {
    await request.delete(`/api/templates/${id}`).catch(() => undefined);
  }
  for (const id of fixtureIds.users) {
    await request.delete(`/api/users/${id}`).catch(() => undefined);
  }
  for (const id of fixtureIds.groups) {
    await request.delete(`/api/groups/${id}`).catch(() => undefined);
  }
});
