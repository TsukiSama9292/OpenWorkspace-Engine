import {
  expect,
  type APIRequestContext,
  type Browser,
  type BrowserContext,
  type Page,
  test,
} from '@playwright/test';

// Full-stack quota lifecycle (ticket 03): billing-group attribution, the
// three quota layers, manager quota editing, pool tightening, host caps, the
// `-1` convention round-trip, and stop/restart accounting — against the
// running dev stack with real containers. Follows the house pattern from
// observability.full.spec.ts: idempotent ensure-* fixtures, per-test
// try/finally for instances, suite-level afterAll for groups/users/templates.

const ADMIN = { username: 'admin', password: 'admin' };
const IMAGE = 'tsukisama9292/ow-kasmvnc-ubuntu:jammy';

const BIG_GROUP = 'e2e-quota-big';
const SMALL_GROUP = 'e2e-quota-small';
const MULTI_USER = { username: 'e2e_quota_multi', password: 'pw123456' };
const SINGLE_USER = { username: 'e2e_quota_single', password: 'pw123456' };
const MGR_USER = { username: 'e2e_quota_mgr', password: 'pw123456' };
const TEMPLATE_NAME = 'e2e-quota-template';
const UNLIM_TEMPLATE_NAME = 'e2e-quota-unlim-template';
const UI_TEMPLATE_NAME = 'e2e-quota-ui-template';

// Pools and member caps. Small pool CPU 3 with a 2-core template: one filler
// (2 used) leaves 1, so a second 2-core launch 409s on the pool while a
// member with cap 2 still fits (0 + 2 <= 2).
const BIG_POOL = { cpu: 8, mem: 16384, gpu: 2 };
const SMALL_POOL = { cpu: 3, mem: 8192, gpu: 1 };
const MULTI_BIG_CAP = { cpu: 8, mem: 16384, gpu: 2 };
const MULTI_SMALL_CAP = { cpu: 2, mem: 4096, gpu: 1 };
const SINGLE_CAP = { cpu: 3, mem: 8192, gpu: 1 };

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

async function deleteInstance(req: APIRequestContext, id: string): Promise<void> {
  const del = await req.delete(`/api/instances/${id}`);
  expect(del.ok()).toBeTruthy();
}

function groupBody(name: string, pool: { cpu: number; mem: number; gpu: number }): Record<string, unknown> {
  return {
    name,
    description: 'E2E quota fixture',
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
    pool_cpu_cores: pool.cpu,
    pool_memory_mb: pool.mem,
    pool_gpu_count: pool.gpu,
  };
}

async function ensureGroup(
  req: APIRequestContext,
  name: string,
  pool: { cpu: number; mem: number; gpu: number }
): Promise<string> {
  const list = await getJson(req, '/api/groups');
  const existing = (list.groups ?? []).find((g: any) => g.name === name);
  if (existing) {
    // Reconcile: a previous partial run may have left the pool or member
    // caps edited. Zero the member caps first (0 fits any pool, so the pool
    // PUT below never trips the member-quota invariant); beforeAll re-sets
    // the intended caps afterwards.
    const id = existing.id as string;
    for (const m of (existing.members ?? []) as { user_id: string }[]) {
      await req.put(`/api/groups/${id}/members/${m.user_id}/quota`, {
        data: { cpu_quota: 0, memory_quota: 0, gpu_quota: 0 },
      });
    }
    const res = await req.put(`/api/groups/${id}`, { data: groupBody(name, pool) });
    expect(res.ok()).toBeTruthy();
    return id;
  }
  const res = await req.post('/api/groups', { data: groupBody(name, pool) });
  expect(res.ok()).toBeTruthy();
  const id = ((await res.json()).group as { id: string }).id;
  fixtureIds.groups.push(id);
  return id;
}

async function userIdByName(req: APIRequestContext, username: string): Promise<string | null> {
  const users = await getJson(req, '/api/users');
  return ((users.users ?? []).find((u: any) => u.username === username)?.id as string) ?? null;
}

async function ensureUser(
  req: APIRequestContext,
  username: string,
  password: string,
  groupIds: string[]
): Promise<string> {
  const existing = await userIdByName(req, username);
  if (existing) return existing;
  const res = await req.post('/api/users', { data: { username, password, group_ids: groupIds } });
  expect(res.ok()).toBeTruthy();
  const id = ((await res.json()).user as { id: string }).id;
  fixtureIds.users.push(id);
  return id;
}

async function setMemberQuota(
  req: APIRequestContext,
  groupId: string,
  userId: string,
  cap: { cpu: number; mem: number; gpu: number }
): Promise<void> {
  const res = await req.put(`/api/groups/${groupId}/members/${userId}/quota`, {
    data: { cpu_quota: cap.cpu, memory_quota: cap.mem, gpu_quota: cap.gpu },
  });
  expect(res.ok()).toBeTruthy();
}

async function groupByName(req: APIRequestContext, name: string): Promise<any> {
  const list = await getJson(req, '/api/groups');
  const group = (list.groups ?? []).find((g: any) => g.name === name);
  expect(group).toBeTruthy();
  return group;
}

async function memberQuota(
  req: APIRequestContext,
  groupId: string,
  userId: string
): Promise<{ cpu: number; mem: number; gpu: number }> {
  const list = await getJson(req, '/api/groups');
  const group = (list.groups ?? []).find((g: any) => g.id === groupId);
  const member = (group?.members ?? []).find((m: any) => m.user_id === userId);
  expect(member).toBeTruthy();
  return { cpu: member.cpu_quota, mem: member.memory_quota, gpu: member.gpu_quota };
}

async function ensureTemplate(req: APIRequestContext, name: string, cores: number): Promise<string> {
  const list = await getJson(req, '/api/templates');
  const existing = (list.templates ?? []).find((t: any) => t.name === name);
  if (existing) return existing.id as string;
  const res = await req.post('/api/templates', {
    data: {
      name,
      description: 'E2E quota fixture',
      image: IMAGE,
      cores,
      memory: 4294967296,
      gpu_count: 0,
      remote_type: 'kasmvnc',
      container_runtime: 'runc',
      visibility: 'public',
      network_bandwidth_up_mbps: -1,
      network_bandwidth_down_mbps: -1,
    },
  });
  expect(res.ok()).toBeTruthy();
  const id = ((await res.json()).template as { id: string }).id;
  fixtureIds.templates.push(id);
  return id;
}

async function launchApi(
  req: APIRequestContext,
  templateId: string,
  groupId?: string
): Promise<{ status: number; body: any }> {
  const res = await req.post('/api/instances', {
    data: groupId ? { template_id: templateId, owner_group_id: groupId } : { template_id: templateId },
  });
  return { status: res.status(), body: (await res.json()) as any };
}

async function setHostCaps(
  req: APIRequestContext,
  caps: { limit: number; cpu: number; mem: number; gpu: number }
): Promise<void> {
  const res = await req.put('/api/admin/settings', {
    data: {
      host_instance_limit: caps.limit,
      host_cpu_cores: caps.cpu,
      host_memory_mb: caps.mem,
      host_gpu_count: caps.gpu,
    },
  });
  expect(res.ok()).toBeTruthy();
}

async function loginUserCtx(
  browser: Browser,
  username: string,
  password: string
): Promise<{ ctx: BrowserContext; page: Page }> {
  const ctx = await browser.newContext();
  const page = await ctx.newPage();
  await loginAs(page, username, password);
  return { ctx, page };
}

async function loginAdminCtx(browser: Browser): Promise<{ ctx: BrowserContext; page: Page }> {
  return loginUserCtx(browser, ADMIN.username, ADMIN.password);
}

async function openTemplateCard(page: Page, templateName: string): Promise<void> {
  await page.goto('/');
  const card = page.locator('.template-card').filter({ hasText: templateName });
  await expect(card.first()).toBeVisible({ timeout: 15_000 });
  await card.first().click();
  await expect(page.locator('.modal-confirm')).toBeVisible();
}

async function dismissRejection(page: Page): Promise<void> {
  await page.locator('[data-testid="rejection-notice"] button').click();
  await expect(page.locator('[data-testid="rejection-notice"]')).toHaveCount(0);
}

test.beforeAll(async ({ request }) => {
  await request.post('/api/auth/login', { data: { username: ADMIN.username, password: ADMIN.password } });

  const bigId = await ensureGroup(request, BIG_GROUP, BIG_POOL);
  const smallId = await ensureGroup(request, SMALL_GROUP, SMALL_POOL);

  const groups = await getJson(request, '/api/groups');
  const managerGroupId = (groups.groups ?? []).find((g: any) => g.kind === 'manager')?.id as string;
  expect(managerGroupId).toBeTruthy();

  const multiId = await ensureUser(request, MULTI_USER.username, MULTI_USER.password, [bigId, smallId]);
  const singleId = await ensureUser(request, SINGLE_USER.username, SINGLE_USER.password, [smallId]);
  await ensureUser(request, MGR_USER.username, MGR_USER.password, [managerGroupId, smallId]);

  await setMemberQuota(request, bigId, multiId, MULTI_BIG_CAP);
  await setMemberQuota(request, smallId, multiId, MULTI_SMALL_CAP);
  await setMemberQuota(request, smallId, singleId, SINGLE_CAP);

  await ensureTemplate(request, TEMPLATE_NAME, 2);
  await ensureTemplate(request, UNLIM_TEMPLATE_NAME, -1);
  await setHostCaps(request, { limit: -1, cpu: -1, mem: -1, gpu: -1 });
});

test.afterAll(async ({ request }) => {
  await request.post('/api/auth/login', { data: { username: ADMIN.username, password: ADMIN.password } });
  await setHostCaps(request, { limit: -1, cpu: -1, mem: -1, gpu: -1 }).catch(() => undefined);
  for (const id of fixtureIds.templates) {
    await request.delete(`/api/templates/${id}`).catch(() => undefined);
  }
  for (const id of fixtureIds.users) {
    await request.delete(`/api/users/${id}`).catch(() => undefined);
  }
  for (const id of fixtureIds.groups) {
    await request.delete(`/api/groups/${id}`).catch(() => undefined);
  }

  // Fixture catalogs are empty again after teardown.
  const templates = await getJson(request, '/api/templates');
  expect(
    (templates.templates ?? []).some((t: any) => (t.name as string).startsWith('e2e-quota'))
  ).toBe(false);
  const users = await getJson(request, '/api/users');
  for (const name of [MULTI_USER.username, SINGLE_USER.username, MGR_USER.username]) {
    expect((users.users ?? []).some((u: any) => u.username === name)).toBe(false);
  }
  const groups = await getJson(request, '/api/groups');
  for (const name of [BIG_GROUP, SMALL_GROUP]) {
    expect((groups.groups ?? []).some((g: any) => g.name === name)).toBe(false);
  }
});

test('multi-group launch defaults the billing picker to the highest-cap group and labels the instance', async ({
  browser,
}) => {
  const { ctx, page } = await loginUserCtx(browser, MULTI_USER.username, MULTI_USER.password);
  let instanceId = '';
  try {
    const me = await getJson(ctx.request, '/api/auth/me');
    const billing = (me.context.group_billing ?? []) as { group_id: string; group_name: string }[];
    expect(billing.length).toBe(2);
    const bigId = billing.find((g) => g.group_name === BIG_GROUP)?.group_id;
    expect(bigId).toBeTruthy();

    await openTemplateCard(page, TEMPLATE_NAME);
    const picker = page.locator('#launch-billing');
    await expect(picker).toBeVisible();
    // Default = highest-cap membership (big group).
    expect(await picker.inputValue()).toBe(bigId as string);
    await expect(picker.locator('option')).toHaveCount(2);
    await page.locator('.modal-confirm').click();

    await page.waitForURL(/\/instances\/[^/]+/, { timeout: 30_000 });
    instanceId = page.url().split('/instances/')[1]?.split('/')[0] ?? '';
    expect(instanceId).toMatch(/^[0-9a-f-]{36}$/);
    await waitForStatus(ctx.request, instanceId, 'running');

    // The dashboard card bills the launch to the picked group.
    await page.goto('/');
    const instCard = page.locator('.ws-card').filter({ hasText: BIG_GROUP });
    await expect(instCard.first()).toBeVisible({ timeout: 15_000 });
    await expect(
      instCard.first().locator('.ws-billing', { hasText: 'Billed to:' })
    ).toContainText(`Billed to: ${BIG_GROUP}`);
  } finally {
    if (instanceId) await deleteInstance(ctx.request, instanceId);
    await ctx.close();
  }
});

test('single-group launch shows no picker and auto-attributes to the only group', async ({
  browser,
}) => {
  const admin = await loginAdminCtx(browser);
  const { ctx, page } = await loginUserCtx(browser, SINGLE_USER.username, SINGLE_USER.password);
  let instanceId = '';
  try {
    // API launch without a billing group: single membership auto-attributes.
    const templateId = await ensureTemplate(admin.ctx.request, TEMPLATE_NAME, 2);
    const launched = await launchApi(ctx.request, templateId);
    expect(launched.status).toBe(200);
    instanceId = launched.body.instance.id as string;
    const small = await groupByName(admin.ctx.request, SMALL_GROUP);
    expect(launched.body.instance.owner_group_id).toBe(small.id);
    await waitForStatus(ctx.request, instanceId, 'running');

    // The launch modal offers no picker to single-group users.
    await openTemplateCard(page, TEMPLATE_NAME);
    await expect(page.locator('#launch-billing')).toHaveCount(0);
    await page.locator('.modal-cancel').click();

    // The running instance carries its billing label.
    await page.goto('/');
    const instCard = page.locator('.ws-card').filter({ hasText: SMALL_GROUP });
    await expect(
      instCard.first().locator('.ws-billing', { hasText: 'Billed to:' })
    ).toContainText(`Billed to: ${SMALL_GROUP}`);
  } finally {
    if (instanceId) await deleteInstance(ctx.request, instanceId);
    await ctx.close();
    await admin.ctx.close();
  }
});

test('member-cap rejection renders with readable numbers', async ({ browser }) => {
  const admin = await loginAdminCtx(browser);
  const user = await loginUserCtx(browser, MULTI_USER.username, MULTI_USER.password);
  const small = await groupByName(admin.ctx.request, SMALL_GROUP);
  const multiId = await userIdByName(admin.ctx.request, MULTI_USER.username);
  try {
    // Tighten multi's small-group CPU cap below the 2-core template.
    await setMemberQuota(admin.ctx.request, small.id, multiId as string, { cpu: 1, mem: 4096, gpu: 1 });

    await openTemplateCard(user.page, TEMPLATE_NAME);
    // Bill to the small group explicitly (the default big group still fits).
    await user.page.locator('#launch-billing').selectOption(small.id);
    await user.page.locator('.modal-confirm').click();

    const notice = user.page.locator('[data-testid="rejection-notice"]');
    await expect(notice).toBeVisible({ timeout: 15_000 });
    await expect(notice).toContainText('Personal CPU quota reached');
    await expect(notice).toContainText('requested 2');
    await dismissRejection(user.page);
  } finally {
    await setMemberQuota(admin.ctx.request, small.id, multiId as string, {
      cpu: MULTI_SMALL_CAP.cpu,
      mem: MULTI_SMALL_CAP.mem,
      gpu: MULTI_SMALL_CAP.gpu,
    });
    await admin.ctx.close();
    await user.ctx.close();
  }
});

test('group-pool rejection renders when the pool is exhausted', async ({ browser }) => {
  const admin = await loginAdminCtx(browser);
  const filler = await loginUserCtx(browser, SINGLE_USER.username, SINGLE_USER.password);
  const user = await loginUserCtx(browser, MULTI_USER.username, MULTI_USER.password);
  let fillerId = '';
  try {
    const templateId = await ensureTemplate(admin.ctx.request, TEMPLATE_NAME, 2);
    const small = await groupByName(admin.ctx.request, SMALL_GROUP);

    // Filler consumes 2 of the 3-CPU small pool.
    const launched = await launchApi(filler.ctx.request, templateId, small.id);
    expect(launched.status).toBe(200);
    fillerId = launched.body.instance.id as string;
    await waitForStatus(filler.ctx.request, fillerId, 'running');

    // Multi's own cap still fits (0 + 2 <= 2) but the pool does not (2 + 2 > 3).
    await openTemplateCard(user.page, TEMPLATE_NAME);
    await user.page.locator('#launch-billing').selectOption(small.id);
    await user.page.locator('.modal-confirm').click();

    const notice = user.page.locator('[data-testid="rejection-notice"]');
    await expect(notice).toBeVisible({ timeout: 15_000 });
    await expect(notice).toContainText('Group CPU pool cap reached');
    await dismissRejection(user.page);
  } finally {
    if (fillerId) await deleteInstance(filler.ctx.request, fillerId);
    await filler.ctx.close();
    await user.ctx.close();
    await admin.ctx.close();
  }
});

test('host memory-cap rejection renders formatted bytes', async ({ browser }) => {
  const admin = await loginAdminCtx(browser);
  const filler = await loginUserCtx(browser, MULTI_USER.username, MULTI_USER.password);
  const user = await loginUserCtx(browser, SINGLE_USER.username, SINGLE_USER.password);
  let fillerId = '';
  try {
    const templateId = await ensureTemplate(admin.ctx.request, TEMPLATE_NAME, 2);
    const big = await groupByName(admin.ctx.request, BIG_GROUP);

    // Host memory fits exactly one 4 GiB template.
    await setHostCaps(admin.ctx.request, { limit: -1, cpu: -1, mem: 4096, gpu: -1 });

    const launched = await launchApi(filler.ctx.request, templateId, big.id);
    expect(launched.status).toBe(200);
    fillerId = launched.body.instance.id as string;
    await waitForStatus(filler.ctx.request, fillerId, 'running');

    // Single-group user: no picker; the host layer rejects with byte rendering.
    await openTemplateCard(user.page, TEMPLATE_NAME);
    await expect(user.page.locator('#launch-billing')).toHaveCount(0);
    await user.page.locator('.modal-confirm').click();

    const notice = user.page.locator('[data-testid="rejection-notice"]');
    await expect(notice).toBeVisible({ timeout: 15_000 });
    await expect(notice).toContainText('Host memory cap reached');
    await expect(notice).toContainText('4 GB');
    await dismissRejection(user.page);

    // The API rejection carries the host scope with MB numbers.
    const rejected = await launchApi(user.ctx.request, templateId);
    expect(rejected.status).toBe(409);
    expect(rejected.body.rejection.scope).toBe('host_resource_memory');
  } finally {
    if (fillerId) await deleteInstance(filler.ctx.request, fillerId);
    await setHostCaps(admin.ctx.request, { limit: -1, cpu: -1, mem: -1, gpu: -1 });
    await filler.ctx.close();
    await user.ctx.close();
    await admin.ctx.close();
  }
});

test('stop releases quota and restart re-checks the pool', async ({ browser }) => {
  const admin = await loginAdminCtx(browser);
  const single = await loginUserCtx(browser, SINGLE_USER.username, SINGLE_USER.password);
  const multi = await loginUserCtx(browser, MULTI_USER.username, MULTI_USER.password);
  let firstId = '';
  let secondId = '';
  try {
    const templateId = await ensureTemplate(admin.ctx.request, TEMPLATE_NAME, 2);
    const small = await groupByName(admin.ctx.request, SMALL_GROUP);

    const first = await launchApi(single.ctx.request, templateId, small.id);
    expect(first.status).toBe(200);
    firstId = first.body.instance.id as string;
    await waitForStatus(single.ctx.request, firstId, 'running');

    // Pool exhausted (2 of 3 used): multi's 2-core launch 409s on the pool.
    const blocked = await launchApi(multi.ctx.request, templateId, small.id);
    expect(blocked.status).toBe(409);
    expect(blocked.body.rejection.scope).toBe('group_pool_cpu');

    // Stopping the first instance releases its share: the retry succeeds.
    expect((await single.ctx.request.post(`/api/instances/${firstId}/stop`)).ok()).toBeTruthy();
    await waitForStatus(single.ctx.request, firstId, 'stopped');
    const retry = await launchApi(multi.ctx.request, templateId, small.id);
    expect(retry.status).toBe(200);
    secondId = retry.body.instance.id as string;
    await waitForStatus(multi.ctx.request, secondId, 'running');

    // Restarting the stopped instance against the refilled pool stays stopped.
    const restart = await single.ctx.request.post(`/api/instances/${firstId}/start`);
    expect(restart.status()).toBe(409);
    await waitForStatus(single.ctx.request, firstId, 'stopped');
  } finally {
    if (firstId) await deleteInstance(single.ctx.request, firstId);
    if (secondId) await deleteInstance(multi.ctx.request, secondId);
    await single.ctx.close();
    await multi.ctx.close();
    await admin.ctx.close();
  }
});

test('manager edits a lower-tier member quota in the Groups tab and it takes effect', async ({
  browser,
}) => {
  const admin = await loginAdminCtx(browser);
  const mgr = await loginUserCtx(browser, MGR_USER.username, MGR_USER.password);
  const single = await loginUserCtx(browser, SINGLE_USER.username, SINGLE_USER.password);
  const small = await groupByName(admin.ctx.request, SMALL_GROUP);
  const singleId = await userIdByName(admin.ctx.request, SINGLE_USER.username);
  let instanceId = '';
  try {
    const templateId = await ensureTemplate(admin.ctx.request, TEMPLATE_NAME, 2);

    // Manager opens the Groups tab and edits the tier-0 member's CPU cap to 1.
    await mgr.page.goto('/');
    await mgr.page.locator('.sidebar').hover();
    await mgr.page.locator('.sidebar .nav-item').filter({ hasText: 'Groups' }).click();
    await expect(mgr.page.locator('.panel-head-title')).toHaveText('Group Management', {
      timeout: 15_000,
    });
    const smallRow = mgr.page.locator('tr').filter({ hasText: SMALL_GROUP }).first();
    await smallRow.locator('button.link-btn').click();
    const row = mgr.page.locator('.member-row').filter({ hasText: SINGLE_USER.username });
    await expect(row).toBeVisible();
    await row.locator('button').filter({ hasText: 'Edit quotas' }).click();
    await mgr.page.locator('[aria-label="CPU Quota (cores) mode"]').selectOption('custom');
    await mgr.page.locator('[aria-label="CPU Quota (cores) value"]').fill('1');
    await Promise.all([
      mgr.page.waitForResponse(
        (r) => r.url().includes(`/api/groups/${small.id}/members/${singleId}/quota`) && r.status() === 200
      ),
      mgr.page.locator('.modal-confirm').filter({ hasText: 'Save Quotas' }).click(),
    ]);

    // The tighter cap rejects the member's next 2-core launch.
    const blocked = await launchApi(single.ctx.request, templateId, small.id);
    expect(blocked.status).toBe(409);
    expect(blocked.body.rejection.scope).toBe('member_quota_cpu');

    // Restoring the cap lets the same launch through.
    await setMemberQuota(admin.ctx.request, small.id, singleId as string, SINGLE_CAP);
    const launched = await launchApi(single.ctx.request, templateId, small.id);
    expect(launched.status).toBe(200);
    instanceId = launched.body.instance.id as string;
    await waitForStatus(single.ctx.request, instanceId, 'running');
  } finally {
    if (instanceId) await deleteInstance(single.ctx.request, instanceId);
    const singleIdNow = await userIdByName(admin.ctx.request, SINGLE_USER.username);
    if (singleIdNow) await setMemberQuota(admin.ctx.request, small.id, singleIdNow, SINGLE_CAP);
    await single.ctx.close();
    await mgr.ctx.close();
    await admin.ctx.close();
  }
});

test('admin pool edit below a member cap is refused, then reset-all unblocks it', async ({
  browser,
}) => {
  const admin = await loginAdminCtx(browser);
  const small = await groupByName(admin.ctx.request, SMALL_GROUP);
  const singleId = await userIdByName(admin.ctx.request, SINGLE_USER.username);
  const multiId = await userIdByName(admin.ctx.request, MULTI_USER.username);
  try {
    // API: shrinking the pool below single's finite cap 409s.
    const shrinkBody = {
      name: SMALL_GROUP,
      description: 'E2E quota fixture',
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
      pool_cpu_cores: 2,
      pool_memory_mb: SMALL_POOL.mem,
      pool_gpu_count: SMALL_POOL.gpu,
    };
    const shrink = await admin.ctx.request.put(`/api/groups/${small.id}`, { data: shrinkBody });
    expect(shrink.status()).toBe(409);

    // UI: the same edit keeps the group modal open on the 409.
    await admin.page.goto('/');
    await admin.page.locator('.sidebar').hover();
    await admin.page.locator('.sidebar .nav-item').filter({ hasText: 'Groups' }).click();
    await expect(admin.page.locator('.panel-head-title')).toHaveText('Group Management', {
      timeout: 15_000,
    });
    const groupRow = admin.page.locator('tr').filter({ hasText: SMALL_GROUP }).first();
    await groupRow.locator('button').filter({ hasText: 'Edit' }).first().click();
    await admin.page.locator('[aria-label="Pool CPU (cores) mode"]').selectOption('custom');
    await admin.page.locator('[aria-label="Pool CPU (cores) value"]').fill('2');
    const saveResp = admin.page.waitForResponse(
      (r) => r.url().includes(`/api/groups/${small.id}`) && r.request().method() === 'PUT'
    );
    await admin.page.locator('.modal-confirm').filter({ hasText: 'Save Changes' }).click();
    expect((await saveResp).status()).toBe(409);
    // Still editing: the modal did not close on error.
    await expect(admin.page.locator('[aria-label="Pool CPU (cores) mode"]')).toBeVisible();
    await admin.page.locator('.modal-cancel').filter({ hasText: 'Cancel' }).click();

    // UI: one-click reset drops every member quota to 0, then the pool edit lands.
    const smallRowUi = admin.page.locator('tr').filter({ hasText: SMALL_GROUP }).first();
    await smallRowUi.locator('button.link-btn').click();
    admin.page.on('dialog', (dialog) => dialog.accept());
    await admin.page.locator('button').filter({ hasText: 'Reset all quotas to 0' }).click();
    await expect
      .poll(async () => (await memberQuota(admin.ctx.request, small.id, singleId as string)).cpu, {
        timeout: 15_000,
      })
      .toBe(0);
    const shrinkOk = await admin.ctx.request.put(`/api/groups/${small.id}`, { data: shrinkBody });
    expect(shrinkOk.status()).toBe(200);
  } finally {
    // Restore the pool and the member caps the suite depends on.
    const restoreBody = {
      name: SMALL_GROUP,
      description: 'E2E quota fixture',
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
      pool_cpu_cores: SMALL_POOL.cpu,
      pool_memory_mb: SMALL_POOL.mem,
      pool_gpu_count: SMALL_POOL.gpu,
    };
    await admin.ctx.request.put(`/api/groups/${small.id}`, { data: restoreBody });
    if (singleId) await setMemberQuota(admin.ctx.request, small.id, singleId, SINGLE_CAP);
    if (multiId) {
      const multiIdNow = await userIdByName(admin.ctx.request, MULTI_USER.username);
      if (multiIdNow) await setMemberQuota(admin.ctx.request, small.id, multiIdNow, MULTI_SMALL_CAP);
    }
    await admin.ctx.close();
  }
});

test('unlimited template request is refused by a finite host cap', async ({ browser }) => {
  const admin = await loginAdminCtx(browser);
  try {
    const templateId = await ensureTemplate(admin.ctx.request, UNLIM_TEMPLATE_NAME, -1);
    await setHostCaps(admin.ctx.request, { limit: -1, cpu: 8, mem: -1, gpu: -1 });

    // The admin's own membership and pool are `-1`, so a `-1` cores request
    // sails through member and pool and is refused at the finite host cap.
    const rejected = await launchApi(admin.ctx.request, templateId);
    expect(rejected.status).toBe(409);
    expect(rejected.body.rejection.scope).toBe('host_resource_cpu');
  } finally {
    await setHostCaps(admin.ctx.request, { limit: -1, cpu: -1, mem: -1, gpu: -1 });
    await admin.ctx.close();
  }
});

test('the -1 convention round-trips through the template form and launches', async ({
  browser,
}) => {
  const admin = await loginAdminCtx(browser);
  let templateId = '';
  let instanceId = '';
  try {
    // Create through the UI with the Unlimited toggles for cores/RAM/bandwidth.
    await admin.page.goto('/');
    await admin.page.locator('.sidebar').hover();
    await admin.page.locator('.sidebar .nav-item').filter({ hasText: 'Templates' }).click();
    await admin.page.locator('button').filter({ hasText: '+ New Template' }).click();
    await expect(admin.page.locator('h1')).toContainText('New Template');
    await admin.page.locator('input[placeholder="e.g. AI Lab"]').fill(UI_TEMPLATE_NAME);
    await admin.page.locator('select').filter({ hasText: 'KasmVNC' }).first().selectOption('kasmvnc');
    await admin.page
      .locator('select')
      .filter({ hasText: 'Public' })
      .first()
      .selectOption('public');
    await admin.page.locator('[aria-label="CPU Cores * unlimited"]').check();
    await admin.page.locator('[aria-label="RAM (GB) * unlimited"]').check();
    await admin.page.locator('button').filter({ hasText: 'Show Advanced' }).click();
    await admin.page.locator('[aria-label="Upload Limit (Mbps) unlimited"]').check();
    await admin.page.locator('[aria-label="Download Limit (Mbps) unlimited"]').check();
    await Promise.all([
      admin.page.waitForResponse(
        (r) => r.url().includes('/api/templates') && r.request().method() === 'POST'
      ),
      admin.page.locator('button').filter({ hasText: 'Create Template' }).click(),
    ]);

    // Persisted as -1 on every unlimited field.
    const list = await getJson(admin.ctx.request, '/api/templates');
    const created = (list.templates ?? []).find((t: any) => t.name === UI_TEMPLATE_NAME);
    expect(created).toBeTruthy();
    templateId = created.id as string;
    if (!fixtureIds.templates.includes(templateId)) fixtureIds.templates.push(templateId);
    expect(created.cores).toBe(-1);
    expect(created.memory).toBe(-1);
    expect(created.network_bandwidth_up_mbps).toBe(-1);
    expect(created.network_bandwidth_down_mbps).toBe(-1);

    // An unlimited request launches wherever every layer is unlimited (admin).
    const launched = await launchApi(admin.ctx.request, templateId);
    expect(launched.status).toBe(200);
    instanceId = launched.body.instance.id as string;
    await waitForStatus(admin.ctx.request, instanceId, 'running');
  } finally {
    if (instanceId) await deleteInstance(admin.ctx.request, instanceId);
    if (templateId) await admin.ctx.request.delete(`/api/templates/${templateId}`);
    await admin.ctx.close();
  }
});

test('a zero pool blocks launches and -1 round-trips through the Settings tab', async ({
  browser,
}) => {
  const admin = await loginAdminCtx(browser);
  const single = await loginUserCtx(browser, SINGLE_USER.username, SINGLE_USER.password);
  const small = await groupByName(admin.ctx.request, SMALL_GROUP);
  try {
    const templateId = await ensureTemplate(admin.ctx.request, TEMPLATE_NAME, 2);

    // A 0 pool blocks every finite launch into the group.
    const zeroed = await admin.ctx.request.put(`/api/groups/${small.id}`, {
      data: {
        name: SMALL_GROUP,
        description: 'E2E quota fixture',
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
        pool_cpu_cores: 0,
        pool_memory_mb: SMALL_POOL.mem,
        pool_gpu_count: SMALL_POOL.gpu,
      },
    });
    // Pool 0 sits below single's finite cap 3: the tightening invariant refuses.
    expect(zeroed.status()).toBe(409);

    // Zero the member caps first (as the UI reset-all does), then the 0 pool lands.
    const singleId = await userIdByName(admin.ctx.request, SINGLE_USER.username);
    const multiId = await userIdByName(admin.ctx.request, MULTI_USER.username);
    await setMemberQuota(admin.ctx.request, small.id, singleId as string, { cpu: 0, mem: 0, gpu: 0 });
    await setMemberQuota(admin.ctx.request, small.id, multiId as string, { cpu: 0, mem: 0, gpu: 0 });
    const zeroedOk = await admin.ctx.request.put(`/api/groups/${small.id}`, {
      data: {
        name: SMALL_GROUP,
        description: 'E2E quota fixture',
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
        pool_cpu_cores: 0,
        pool_memory_mb: SMALL_POOL.mem,
        pool_gpu_count: SMALL_POOL.gpu,
      },
    });
    expect(zeroedOk.status()).toBe(200);
    // Lift single's cap to unlimited so the member layer skips and the 0
    // pool is what binds (`-1` is exempt from the pool invariant).
    await setMemberQuota(admin.ctx.request, small.id, singleId as string, { cpu: -1, mem: -1, gpu: -1 });
    const blocked = await launchApi(single.ctx.request, templateId, small.id);
    expect(blocked.status).toBe(409);
    expect(blocked.body.rejection.scope).toBe('group_pool_cpu');

    // Settings tab: -1 host caps save and read back as unlimited.
    await admin.page.goto('/');
    await admin.page.locator('.sidebar').hover();
    await admin.page.locator('.sidebar .nav-item').filter({ hasText: 'Settings' }).click();
    await expect(admin.page.locator('.card-title')).toContainText('Host Resource Policy', {
      timeout: 15_000,
    });
    await expect(admin.page.locator('[aria-label="Host CPU (cores) mode"]')).toHaveValue('unlimited');
    await Promise.all([
      admin.page.waitForResponse(
        (r) => r.url().includes('/api/admin/settings') && r.request().method() === 'PUT'
      ),
      admin.page.locator('.save-btn').filter({ hasText: 'Save Changes' }).click(),
    ]);
    await expect(admin.page.locator('.save-saved')).toBeVisible();
    const settings = await getJson(admin.ctx.request, '/api/admin/settings');
    expect(settings.settings.host_cpu_cores).toBe(-1);
    expect(settings.settings.host_memory_mb).toBe(-1);
  } finally {
    const restoreBody = {
      name: SMALL_GROUP,
      description: 'E2E quota fixture',
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
      pool_cpu_cores: SMALL_POOL.cpu,
      pool_memory_mb: SMALL_POOL.mem,
      pool_gpu_count: SMALL_POOL.gpu,
    };
    await admin.ctx.request.put(`/api/groups/${small.id}`, { data: restoreBody });
    const singleIdNow = await userIdByName(admin.ctx.request, SINGLE_USER.username);
    if (singleIdNow) await setMemberQuota(admin.ctx.request, small.id, singleIdNow, SINGLE_CAP);
    const multiIdNow = await userIdByName(admin.ctx.request, MULTI_USER.username);
    if (multiIdNow) await setMemberQuota(admin.ctx.request, small.id, multiIdNow, MULTI_SMALL_CAP);
    await single.ctx.close();
    await admin.ctx.close();
  }
});

test('Quota E2E leaves no running instances behind and restores host caps', async ({
  browser,
}) => {
  const admin = await loginAdminCtx(browser);
  try {
    const instances = await getJson(admin.ctx.request, '/api/instances');
    const leakedInstances = (instances.instances ?? []).filter((i: any) =>
      (i.template_name as string).startsWith('e2e-quota')
    );
    expect(leakedInstances).toHaveLength(0);

    const settings = await getJson(admin.ctx.request, '/api/admin/settings');
    expect(settings.settings.host_cpu_cores).toBe(-1);
    expect(settings.settings.host_memory_mb).toBe(-1);
    expect(settings.settings.host_gpu_count).toBe(-1);
    expect(settings.settings.host_instance_limit).toBe(-1);
  } finally {
    await admin.ctx.close();
  }
});
