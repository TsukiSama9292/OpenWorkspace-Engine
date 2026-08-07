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

test('unauthenticated visitors are redirected to login and see no dashboard tabs', async ({ page }) => {
  await page.goto('/');

  await expect(page).toHaveURL(/\/login\/?/, { timeout: 10_000 });
  await expect(page.locator('.dashboard')).toHaveCount(0);
  await expect(page.locator('.nav-item')).toHaveCount(0);
  await expect(page.locator('.section-title')).toHaveCount(0);
});

test('admin can log in and sees the dashboard with all admin-gated tabs', async ({ page }) => {
  await loginAsAdmin(page);

  await expect(page.locator('.section-title')).toHaveText(['Instances', 'Quick Launch']);
  // The instances section renders a grid when instances exist and an
  // empty-state message when the stack has none — the smoke test is read-only
  // and must pass in either state.
  const instanceGrid = page.locator('.instance-grid');
  const noInstances = page.locator('.empty-text').filter({ hasText: 'No instances yet' });
  await expect(instanceGrid.or(noInstances).first()).toBeVisible();

  await page.locator('.sidebar').hover();
  await expect(page.locator('.sidebar .nav-text')).toHaveText([
    'Instances',
    'Templates',
    'Sessions',
    'Volumes',
    'Groups',
    'Users',
    'Monitor',
    'Settings',
    'Logs',
  ]);

  await page.locator('.sidebar .nav-item').filter({ hasText: 'Templates' }).click();
  await expect(page.locator('.templates-header')).toBeVisible();
  await expect(page.locator('.btn-create')).toHaveText('+ New Template');
});
