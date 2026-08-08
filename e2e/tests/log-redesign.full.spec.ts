import { expect, type APIRequestContext, type Browser, type Page, test } from '@playwright/test';

const ADMIN = { username: 'admin', password: 'admin' };
const TEMPLATE_NAME = 'e2e-logredesign-template';
const IMAGE = 'tsukisama9292/ow-kasmvnc-ubuntu:jammy';
const TIME_RE = /^\d{4}-\d{2}-\d{2} \d{2}:\d{2}$/;

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

async function ensureTemplate(req: APIRequestContext): Promise<string> {
  const list = await getJson(req, '/api/templates');
  const existing = (list.templates ?? []).find((t: any) => t.name === TEMPLATE_NAME);
  if (existing) return existing.id as string;

  const res = await req.post('/api/templates', {
    data: {
      name: TEMPLATE_NAME,
      description: 'E2E log-redesign fixture',
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

async function launchInstance(req: APIRequestContext, templateId: string): Promise<string> {
  const res = await req.post('/api/instances', { data: { template_id: templateId } });
  expect(res.ok()).toBeTruthy();
  const instance = (await res.json()).instance as { id: string; name: string };
  return instance.id;
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

async function openInstanceLogs(page: Page, instanceId: string, instanceName: string): Promise<void> {
  await page.goto('/');
  const card = page.locator('.ws-card').filter({ hasText: instanceName });
  await expect(card).toBeVisible({ timeout: 15_000 });
  await card.locator('.launch-btn.logs').click();
  await expect(page.locator('.logs-modal')).toBeVisible({ timeout: 15_000 });
}

async function pollAudit(
  req: APIRequestContext,
  params: string,
  predicate: (body: any) => boolean
): Promise<void> {
  await expect
    .poll(
      async () => {
        const res = await req.get(`/api/audit?${params}`);
        if (res.status() !== 200) return false;
        return predicate(await res.json());
      },
      { timeout: 30_000, intervals: [1_000] }
    )
    .toBe(true);
}

test('Audit filter grid + action row render and apply/clear round-trips against live data', async ({
  page,
}) => {
  await loginAs(page, ADMIN.username, ADMIN.password);

  let templateId = '';
  try {
    templateId = await ensureTemplate(page.request);
    await pollAudit(page.request, 'action=template.create', (b) => b.entries.length > 0);

    await openLogsTab(page);

    // Redesigned filter bar: a grid of fields plus a separated action row.
    await expect(page.locator('.filter-grid')).toBeVisible();
    await expect(page.locator('.filter-pair')).toBeVisible();
    await expect(page.locator('.filter-actions-row .filter-apply')).toBeVisible();
    await expect(page.locator('.filter-actions-row .filter-count')).toHaveText(/\d+ entries?/);
    await expect(page.locator('.filter-grid .filter-actions-row')).toHaveCount(0);

    // Apply a filter round-trips against the real endpoint.
    await page.locator('#log-filter-action').selectOption({ label: 'Template created' });
    await expect(page.locator('.filter-actions-row .filter-clear')).toBeVisible();
    await page.locator('.filter-actions-row .filter-apply').click();

    const rows = page.locator('.audit-row');
    await expect(rows.first()).toBeVisible({ timeout: 15_000 });
    const chips = await page.locator('.action-chip').allTextContents();
    expect(chips.length).toBeGreaterThan(0);
    expect(chips.every((c) => c === 'Template created')).toBe(true);

    // Clearing restores the unfiltered list and hides the Clear button again.
    await page.locator('.filter-actions-row .filter-clear').click();
    await expect(page.locator('.filter-actions-row .filter-clear')).toHaveCount(0);
    await expect(page.locator('#log-filter-action')).toHaveValue('');
  } finally {
    if (templateId) await page.request.delete(`/api/templates/${templateId}`);
  }
});

test('Audit rows show compact timestamps and an edit chevron expands/collapses by keyboard', async ({
  page,
}) => {
  await loginAs(page, ADMIN.username, ADMIN.password);

  let templateId = '';
  try {
    templateId = await ensureTemplate(page.request);

    // A template update produces a template.update event with a before/after diff.
    const put = await page.request.put(`/api/templates/${templateId}`, {
      data: {
        name: TEMPLATE_NAME,
        description: 'E2E log-redesign fixture — edited',
        image: IMAGE,
        cores: 1,
        memory: 1073741824,
        remote_type: 'kasmvnc',
        container_runtime: 'runc',
        visibility: 'public',
      },
    });
    expect(put.ok()).toBeTruthy();
    await pollAudit(page.request, 'action=template.update', (b) => b.entries.length > 0);

    await openLogsTab(page);
    await page.locator('#log-filter-action').selectOption({ label: 'Template updated' });
    await page.locator('.filter-apply').click();

    const row = page.locator('.audit-row').filter({ hasText: 'Template updated' }).first();
    await expect(row).toBeVisible({ timeout: 15_000 });

    // Compact timestamp in the row.
    await expect(row.locator('td.td-date:not(.td-ip)')).toHaveText(TIME_RE);

    // Chevron is a real button; Enter toggles expansion and aria-expanded.
    const toggle = row.locator('.diff-toggle');
    await expect(toggle).toBeVisible();
    await expect(toggle).toHaveAttribute('aria-expanded', 'false');

    await toggle.focus();
    await page.keyboard.press('Enter');
    await expect(toggle).toHaveAttribute('aria-expanded', 'true');
    await expect(page.locator('.audit-diff').first()).toBeVisible();

    await page.keyboard.press('Enter');
    await expect(toggle).toHaveAttribute('aria-expanded', 'false');
    await expect(page.locator('.audit-diff').first()).toBeHidden();
  } finally {
    if (templateId) await page.request.delete(`/api/templates/${templateId}`);
  }
});

test('Load more walks at least one cursor page on a populated trail', async ({ page }) => {
  await loginAs(page, ADMIN.username, ADMIN.password);

  try {
    // Fill past the 50-entry page size with cheap auth.login events.
    for (let i = 0; i < 55; i++) {
      const res = await page.request.post('/api/auth/login', {
        data: { username: ADMIN.username, password: ADMIN.password },
      });
      expect(res.ok()).toBeTruthy();
    }

    await pollAudit(page.request, 'limit=50', (b) => b.next_cursor != null);

    await openLogsTab(page);
    const loadMore = page.locator('.load-more-row .launch-btn.resume');
    await expect(loadMore).toBeVisible({ timeout: 15_000 });

    const before = await page.locator('.audit-row').count();
    expect(before).toBeGreaterThan(0);

    await loadMore.click();
    await expect
      .poll(async () => page.locator('.audit-row').count(), { timeout: 15_000 })
      .toBeGreaterThan(before);
  } finally {
    // Nothing to clean: logins are audited by design and templates are untouched.
  }
});

test('Log modal: wrap toggle, fullscreen toggle, and font size persists across reopen', async ({
  page,
}) => {
  await loginAs(page, ADMIN.username, ADMIN.password);

  let templateId = '';
  let instanceId = '';
  let instanceName = '';
  try {
    templateId = await ensureTemplate(page.request);
    instanceId = await launchInstance(page.request, templateId);
    await waitForStatus(page.request, instanceId, 'running');
    instanceName = ((await getJson(page.request, `/api/instances/${instanceId}`)).instance as any).name;

    await openInstanceLogs(page, instanceId, instanceName);
    const body = page.locator('.logs-body');

    // Wrap toggle (default on) → alignment-faithful no-wrap mode and back.
    await expect(body).not.toHaveClass(/nowrap/);
    await page.getByRole('checkbox', { name: /Wrap/ }).uncheck();
    await expect(body).toHaveClass(/nowrap/);
    await page.getByRole('checkbox', { name: /Wrap/ }).check();
    await expect(body).not.toHaveClass(/nowrap/);

    // A−/A+ changes the font size and the choice survives reopening.
    await page.getByLabel('Increase log font size').click();
    await page.getByLabel('Increase log font size').click();
    await expect(body).toHaveCSS('font-size', '15px');

    await page.locator('.logs-modal .modal-cancel').filter({ hasText: '×' }).click();
    await expect(page.locator('.logs-modal')).toHaveCount(0);

    await openInstanceLogs(page, instanceId, instanceName);
    await expect(page.locator('.logs-body')).toHaveCSS('font-size', '15px');

    // Fullscreen toggle expands and restores the modal.
    await page.getByText('Fullscreen', { exact: true }).click();
    await expect(page.locator('.logs-modal')).toHaveClass(/fullscreen/);
    await page.getByText('Exit', { exact: true }).click();
    await expect(page.locator('.logs-modal')).not.toHaveClass(/fullscreen/);
  } finally {
    if (instanceId) await page.request.delete(`/api/instances/${instanceId}`);
    if (templateId) await page.request.delete(`/api/templates/${templateId}`);
  }
});

test('Log modal: follow pauses on scroll-up and resumes at the bottom of a live stream', async ({
  page,
  browser,
}) => {
  await loginAs(page, ADMIN.username, ADMIN.password);

  let templateId = '';
  let instanceId = '';
  try {
    templateId = await ensureTemplate(page.request);
    instanceId = await launchInstance(page.request, templateId);
    await waitForStatus(page.request, instanceId, 'running');

    // A short viewport makes the log body overflow, so scroll-pause is real.
    const narrowCtx = await browser.newContext({ viewport: { width: 1000, height: 600 } });
    const narrowPage = await narrowCtx.newPage();
    await loginAs(narrowPage, ADMIN.username, ADMIN.password);

    const instanceName = ((await getJson(page.request, `/api/instances/${instanceId}`)).instance as any).name;
    await openInstanceLogs(narrowPage, instanceId, instanceName);

    await expect(narrowPage.locator('.logs-sub')).toContainText('streaming');
    await expect(narrowPage.locator('.logs-body .log-line').first()).toBeVisible({
      timeout: 30_000,
    });

    const body = narrowPage.locator('.logs-body');
    const geometry = await body.evaluate((el) => ({
      scrollHeight: el.scrollHeight,
      clientHeight: el.clientHeight,
    }));

    if (geometry.scrollHeight > geometry.clientHeight + 24) {
      await body.evaluate((el) => {
        el.scrollTop = el.scrollHeight;
        el.dispatchEvent(new Event('scroll'));
      });
      await expect(narrowPage.locator('.logs-sub')).toContainText('streaming');

      await body.evaluate((el) => {
        el.scrollTop = 0;
        el.dispatchEvent(new Event('scroll'));
      });
      await expect(narrowPage.locator('.logs-sub')).toContainText(/paused/);

      await body.evaluate((el) => {
        el.scrollTop = el.scrollHeight;
        el.dispatchEvent(new Event('scroll'));
      });
      await expect(narrowPage.locator('.logs-sub')).toContainText('streaming');
    }

    await narrowCtx.close();
  } finally {
    if (instanceId) await page.request.delete(`/api/instances/${instanceId}`);
    if (templateId) await page.request.delete(`/api/templates/${templateId}`);
  }
});

test('Log-redesign E2E leaves the dev stack clean (no leaked test resources)', async ({ page }) => {
  await loginAs(page, ADMIN.username, ADMIN.password);

  const instances = await getJson(page.request, '/api/instances');
  const leaked = (instances.instances ?? []).filter((i: any) =>
    (i.name as string).includes('e2e-logredesign')
  );
  expect(leaked).toHaveLength(0);

  const templates = await getJson(page.request, '/api/templates');
  expect((templates.templates ?? []).some((t: any) => t.name === TEMPLATE_NAME)).toBe(false);
});
