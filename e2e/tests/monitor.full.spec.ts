import { expect, type APIRequestContext, type Browser, type Page, test } from '@playwright/test';

const ADMIN = { username: 'admin', password: 'admin' };
const MANAGER = { username: 'e2e_mon_mgr', password: 'pw123456' };
const PLAIN = { username: 'e2e_mon_usr', password: 'pw123456' };
const TEMPLATE_NAME = 'e2e-monitor-template';
const IMAGE = 'tsukisama9292/ow-kasmvnc-ubuntu:jammy';

const created: { users: string[]; groups: string[] } = { users: [], groups: [] };

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

interface Launched {
  id: string;
  name: string;
}

async function ensureMonitorTemplate(req: APIRequestContext): Promise<string> {
  const list = await getJson(req, '/api/templates');
  const existing = (list.templates ?? []).find((t: any) => t.name === TEMPLATE_NAME);
  if (existing) return existing.id as string;

  const res = await req.post('/api/templates', {
    data: {
      name: TEMPLATE_NAME,
      description: 'E2E monitor dashboard fixture',
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

async function launchInstance(req: APIRequestContext, templateId: string): Promise<Launched> {
  const res = await req.post('/api/instances', { data: { template_id: templateId } });
  expect(res.ok()).toBeTruthy();
  const instance = ((await res.json()).instance as { id: string; name: string }) ?? {
    id: '',
    name: '',
  };
  return { id: instance.id, name: instance.name };
}

async function waitForRunning(req: APIRequestContext, id: string): Promise<void> {
  await expect
    .poll(
      async () => {
        const body = await getJson(req, `/api/instances/${id}`);
        return (body.instance as { status: string }).status;
      },
      { timeout: 120_000, intervals: [3_000] }
    )
    .toBe('running');
}

async function openMonitorTab(page: Page): Promise<void> {
  await page.goto('/');
  await page.locator('.sidebar').hover();
  await page.locator('.sidebar .nav-item').filter({ hasText: 'Monitor' }).click();
  await expect(page.locator('.monitor-panel')).toBeVisible({ timeout: 15_000 });
}

async function createFlaggedUser(req: APIRequestContext, username: string, flag: boolean): Promise<void> {
  const groupName = flag ? 'e2e-mon-mgr' : 'e2e-mon-usr';
  const groups = await getJson(req, '/api/groups');
  const existing = (groups.groups ?? []).find((g: any) => g.name === groupName);
  let groupId: string;
  if (existing) {
    groupId = existing.id as string;
  } else {
    const res = await req.post('/api/groups', {
      data: {
        name: groupName,
        description: 'E2E monitor permission fixture',
        can_view_monitoring: flag,
        max_instances: 2,
        template_ids: [],
      },
    });
    expect(res.ok()).toBeTruthy();
    groupId = ((await res.json()).group as { id: string }).id;
    created.groups.push(groupId);
  }

  const users = await getJson(req, '/api/users');
  const exists = (users.users ?? []).some((u: any) => u.username === username);
  if (exists) return;

  const res = await req.post('/api/users', {
    data: { username, password: 'pw123456', group_ids: [groupId] },
  });
  expect(res.ok()).toBeTruthy();
  created.users.push(((await res.json()).user as { id: string }).id);
}

test('Monitor tab shows live host + instance metrics, range toggle, and paused badge', async ({
  page,
}) => {
  await loginAs(page, ADMIN.username, ADMIN.password);

  let templateId = '';
  let instance: Launched | null = null;
  try {
    templateId = await ensureMonitorTemplate(page.request);
    instance = await launchInstance(page.request, templateId);
    expect(instance.id).toMatch(/^[0-9a-f-]{36}$/);
    await waitForRunning(page.request, instance.id);

    const snapshotUrls: string[] = [];
    page.on('request', (req) => {
      if (req.url().includes('/api/monitor/snapshot')) snapshotUrls.push(req.url());
    });

    await openMonitorTab(page);

    await expect(page.locator('.host-card')).toHaveCount(3, { timeout: 15_000 });
    await expect
      .poll(
        () =>
          page
            .locator('.host-card')
            .first()
            .locator('svg[data-testid="sparkline"] path.sparkline-line')
            .getAttribute('d'),
        { timeout: 90_000, intervals: [3_000] }
      )
      .toBeTruthy();

    const row = page.locator('.monitor-row').filter({ hasText: instance.name });
    await expect(row).toBeVisible({ timeout: 90_000 });

    const rowSparks = row.locator('svg[data-testid="sparkline"]');
    await expect(rowSparks).toHaveCount(2, { timeout: 90_000 });
    for (let i = 0; i < 2; i++) {
      await expect
        .poll(() => rowSparks.nth(i).locator('path.sparkline-line').getAttribute('d'), {
          timeout: 90_000,
          intervals: [3_000],
        })
        .toBeTruthy();
    }

    await page.locator('.range-btn', { hasText: '24h' }).click();
    await expect
      .poll(() => snapshotUrls.some((u) => u.includes('range=24h')), { timeout: 15_000 })
      .toBe(true);
    await expect(page.locator('.host-card')).toHaveCount(3);
    await expect(page.locator('.monitor-row').filter({ hasText: instance.name })).toBeVisible();

    const pause = await page.request.post(`/api/instances/${instance.id}/pause`);
    expect(pause.ok()).toBeTruthy();
    const pausedRow = page.locator('.monitor-row.paused').filter({ hasText: instance.name });
    await expect(pausedRow).toBeVisible({ timeout: 30_000 });
    await expect(pausedRow.locator('.status-badge.paused')).toHaveText('[paused]');
  } finally {
    const instanceId = instance?.id;
    if (instanceId) {
      await page.request.delete(`/api/instances/${instanceId}`);
      await expect
        .poll(
          async () => {
            const list = await getJson(page.request, '/api/instances');
            return (list.instances ?? []).some((i: any) => i.id === instanceId);
          },
          { timeout: 30_000 }
        )
        .toBe(false);
    }
    if (templateId) {
      await page.request.delete(`/api/templates/${templateId}`);
    }
  }
});

test('Monitor permission boundary: manager with the flag sees it, plain user does not (403)', async ({
  page,
  browser,
}) => {
  await loginAs(page, ADMIN.username, ADMIN.password);
  await createFlaggedUser(page.request, MANAGER.username, true);
  await createFlaggedUser(page.request, PLAIN.username, false);

  try {
    const managerCtx = await browser.newContext();
    const managerPage = await managerCtx.newPage();
    await loginAs(managerPage, MANAGER.username, MANAGER.password);
    await managerPage.locator('.sidebar').hover();
    await expect(
      managerPage.locator('.sidebar .nav-text').filter({ hasText: 'Monitor' })
    ).toBeVisible();
    const mgrSnap = await managerCtx.request.get('/api/monitor/snapshot');
    expect(mgrSnap.status()).toBe(200);
    await managerCtx.close();

    const plainCtx = await browser.newContext();
    const plainPage = await plainCtx.newPage();
    await loginAs(plainPage, PLAIN.username, PLAIN.password);
    await plainPage.locator('.sidebar').hover();
    await expect(plainPage.locator('.sidebar .nav-text').filter({ hasText: 'Monitor' })).toHaveCount(
      0
    );
    const plainSnap = await plainCtx.request.get('/api/monitor/snapshot');
    expect(plainSnap.status()).toBe(403);
    await plainCtx.close();
  } finally {
    for (const id of created.users) {
      await page.request.delete(`/api/users/${id}`);
    }
    created.users = [];
    for (const id of created.groups) {
      await page.request.delete(`/api/groups/${id}`);
    }
    created.groups = [];
  }
});

test('Monitor E2E leaves the dev stack clean (no leaked test resources)', async ({ page }) => {
  await loginAs(page, ADMIN.username, ADMIN.password);

  const instances = await getJson(page.request, '/api/instances');
  expect((instances.instances ?? []).filter((i: any) => (i.name as string).includes('e2e-monitor'))).toHaveLength(0);

  const templates = await getJson(page.request, '/api/templates');
  expect((templates.templates ?? []).some((t: any) => t.name === TEMPLATE_NAME)).toBe(false);

  const users = await getJson(page.request, '/api/users');
  for (const u of [MANAGER.username, PLAIN.username]) {
    expect((users.users ?? []).some((x: any) => x.username === u)).toBe(false);
  }
  const groups = await getJson(page.request, '/api/groups');
  for (const g of ['e2e-mon-mgr', 'e2e-mon-usr']) {
    expect((groups.groups ?? []).some((x: any) => x.name === g)).toBe(false);
  }
});
