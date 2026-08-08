import { expect, type APIRequestContext, type Browser, type Page, test } from '@playwright/test';

const ADMIN = { username: 'admin', password: 'admin' };
const AUDIT_USER = { username: 'e2e_obs_usr', password: 'pw123456' };
const AUDIT_GROUP = 'e2e-obs-usr';
const TEMPLATE_NAME = 'e2e-observability-template';
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

async function ensureTemplate(req: APIRequestContext): Promise<string> {
  const list = await getJson(req, '/api/templates');
  const existing = (list.templates ?? []).find((t: any) => t.name === TEMPLATE_NAME);
  if (existing) return existing.id as string;

  const res = await req.post('/api/templates', {
    data: {
      name: TEMPLATE_NAME,
      description: 'E2E observability fixture',
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

async function waitForStatus(req: APIRequestContext, id: string, status: string): Promise<void> {
  await expect
    .poll(
      async () => {
        const body = await getJson(req, `/api/instances/${id}`);
        return (body.instance as { status: string }).status;
      },
      { timeout: 120_000, intervals: [3_000] }
    )
    .toBe(status);
}

async function openLogsTab(page: Page): Promise<void> {
  await page.goto('/');
  await page.locator('.sidebar').hover();
  await page.locator('.sidebar .nav-item').filter({ hasText: 'Logs' }).click();
  await expect(page.locator('.panel-head-title')).toHaveText('Audit Logs', { timeout: 15_000 });
}

async function openInstanceLogs(page: Page, instanceName: string): Promise<void> {
  await page.goto('/');
  const card = page.locator('.ws-card').filter({ hasText: instanceName });
  await expect(card).toBeVisible({ timeout: 15_000 });
  await card.locator('.launch-btn.logs').click();
  await expect(page.locator('.logs-modal')).toBeVisible({ timeout: 15_000 });
}

async function ensureAuditUser(req: APIRequestContext): Promise<void> {
  const groups = await getJson(req, '/api/groups');
  const existing = (groups.groups ?? []).find((g: any) => g.name === AUDIT_GROUP);
  let groupId: string;
  if (existing) {
    groupId = existing.id as string;
  } else {
    const res = await req.post('/api/groups', {
      data: {
        name: AUDIT_GROUP,
        description: 'E2E audit permission fixture',
        can_view_audit_logs: false,
        max_instances: 1,
        template_ids: [],
      },
    });
    expect(res.ok()).toBeTruthy();
    groupId = ((await res.json()).group as { id: string }).id;
    created.groups.push(groupId);
  }

  const users = await getJson(req, '/api/users');
  const exists = (users.users ?? []).some((u: any) => u.username === AUDIT_USER.username);
  if (exists) return;

  const res = await req.post('/api/users', {
    data: { username: AUDIT_USER.username, password: AUDIT_USER.password, group_ids: [groupId] },
  });
  expect(res.ok()).toBeTruthy();
  created.users.push(((await res.json()).user as { id: string }).id);
}

test('Admin sees the audit trail and filters narrow it by event type', async ({ page }) => {
  await loginAs(page, ADMIN.username, ADMIN.password);

  let templateId = '';
  try {
    templateId = await ensureTemplate(page.request);

    // template.create is audited asynchronously (best-effort writer); wait
    // until the event is actually queryable before asserting the UI.
    await expect
      .poll(
        async () => {
          const res = await page.request.get('/api/audit?action=template.create');
          if (res.status() !== 200) return 0;
          const body = (await res.json()) as { entries: unknown[] };
          return body.entries.length;
        },
        { timeout: 20_000, intervals: [1_000] }
      )
      .toBeGreaterThan(0);

    await openLogsTab(page);
    await expect(page.locator('.audit-table')).toBeVisible();

    await page.locator('#log-filter-action').selectOption({ label: 'Template created' });
    await page.locator('.filter-apply').click();

    const rows = page.locator('.audit-row');
    await expect(rows.first()).toBeVisible({ timeout: 15_000 });
    const chips = await page.locator('.action-chip').allTextContents();
    expect(chips.length).toBeGreaterThan(0);
    expect(chips.every((c) => c === 'Template created')).toBe(true);

    // A row carries actor, action, outcome and a timestamp.
    await expect(rows.first().locator('.td-owner')).not.toBeEmpty();
    await expect(rows.first().locator('.status-badge')).toContainText('success');
    await expect(rows.first().locator('.td-date:not(.td-ip)')).not.toBeEmpty();
    await expect(rows.first().locator('.td-ip')).not.toBeEmpty();
  } finally {
    if (templateId) await page.request.delete(`/api/templates/${templateId}`);
  }
});

test('Container Logs panel streams a running instance and closes cleanly', async ({ page }) => {
  await loginAs(page, ADMIN.username, ADMIN.password);

  let templateId = '';
  let instance: Launched | null = null;
  try {
    templateId = await ensureTemplate(page.request);
    instance = await launchInstance(page.request, templateId);
    await waitForStatus(page.request, instance.id, 'running');

    await openInstanceLogs(page, instance.name);
    await expect(page.locator('.logs-title')).toContainText(instance.name);
    // Live follow: the panel reports the streaming state while the instance
    // is running, and the container's boot output renders as tail lines.
    await expect(page.locator('.logs-sub')).toContainText('streaming');
    await expect(page.locator('.logs-body .log-line').first()).toBeVisible({ timeout: 30_000 });

    // Closing the panel aborts the stream cleanly.
    await page.locator('.logs-modal .modal-cancel').filter({ hasText: '×' }).click();
    await expect(page.locator('.logs-modal')).toHaveCount(0);
  } finally {
    if (instance) await page.request.delete(`/api/instances/${instance.id}`);
    if (templateId) await page.request.delete(`/api/templates/${templateId}`);
  }
});

test('Audit RBAC: a user without the flag sees no Logs tab and is denied audit data and foreign logs', async ({
  page,
  browser,
}) => {
  await loginAs(page, ADMIN.username, ADMIN.password);

  let templateId = '';
  let instance: Launched | null = null;
  try {
    templateId = await ensureTemplate(page.request);
    instance = await launchInstance(page.request, templateId);
    await waitForStatus(page.request, instance.id, 'running');

    await ensureAuditUser(page.request);

    const plainCtx = await browser.newContext();
    const plainPage = await plainCtx.newPage();
    await loginAs(plainPage, AUDIT_USER.username, AUDIT_USER.password);

    // No Logs tab in the sidebar.
    await plainPage.locator('.sidebar').hover();
    await expect(plainPage.locator('.sidebar .nav-text').filter({ hasText: 'Logs' })).toHaveCount(
      0
    );

    // No audit data via the API.
    const auditRes = await plainCtx.request.get('/api/audit');
    expect(auditRes.status()).toBe(403);

    // No Logs button anywhere for a user who controls nothing, and the API
    // rejects opening another user's instance logs.
    await expect(plainPage.locator('.launch-btn.logs')).toHaveCount(0);
    const logsRes = await plainCtx.request.get(`/api/instances/${instance.id}/logs?tail=5&follow=false`);
    expect(logsRes.status()).toBe(403);

    await plainCtx.close();
  } finally {
    if (instance) await page.request.delete(`/api/instances/${instance.id}`);
    if (templateId) await page.request.delete(`/api/templates/${templateId}`);
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

test('Opening logs on a stopped instance shows the tail and the ended state with the stop reason', async ({
  page,
}) => {
  await loginAs(page, ADMIN.username, ADMIN.password);

  let templateId = '';
  let instance: Launched | null = null;
  try {
    templateId = await ensureTemplate(page.request);
    instance = await launchInstance(page.request, templateId);
    await waitForStatus(page.request, instance.id, 'running');

    const stop = await page.request.post(`/api/instances/${instance.id}/stop`);
    expect(stop.ok()).toBeTruthy();
    await waitForStatus(page.request, instance.id, 'stopped');

    await openInstanceLogs(page, instance.name);
    // Not followable: the stream tails what exists, then reports the ended
    // state with the stop reason instead of hanging.
    await expect(page.locator('.logs-sub')).toContainText('static');
    await expect(page.locator('.logs-ended')).toContainText('Session ended', { timeout: 15_000 });
    await expect(page.locator('.logs-ended')).toContainText('stopped');
  } finally {
    if (instance) await page.request.delete(`/api/instances/${instance.id}`);
    if (templateId) await page.request.delete(`/api/templates/${templateId}`);
  }
});

test('Observability E2E leaves the dev stack clean (no leaked test resources)', async ({ page }) => {
  await loginAs(page, ADMIN.username, ADMIN.password);

  const instances = await getJson(page.request, '/api/instances');
  const leaked = (instances.instances ?? []).filter((i: any) =>
    (i.name as string).startsWith(TEMPLATE_NAME)
  );
  expect(leaked).toHaveLength(0);

  const templates = await getJson(page.request, '/api/templates');
  expect((templates.templates ?? []).some((t: any) => t.name === TEMPLATE_NAME)).toBe(false);

  const users = await getJson(page.request, '/api/users');
  expect((users.users ?? []).some((u: any) => u.username === AUDIT_USER.username)).toBe(false);

  const groups = await getJson(page.request, '/api/groups');
  expect((groups.groups ?? []).some((g: any) => g.name === AUDIT_GROUP)).toBe(false);
});
