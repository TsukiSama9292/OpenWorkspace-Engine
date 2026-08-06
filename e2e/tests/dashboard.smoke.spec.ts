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
  await expect(page.locator('.instance-grid')).toBeVisible();

  await page.locator('.sidebar').hover();
  await expect(page.locator('.sidebar .nav-text')).toHaveText([
    'Instances',
    'Templates',
    'Sessions',
    'Volumes',
    'Groups',
    'Users',
    'Settings',
    'Monitor',
    'Logs',
  ]);

  await page.locator('.sidebar .nav-item').filter({ hasText: 'Templates' }).click();
  await expect(page.locator('.templates-header')).toBeVisible();
  await expect(page.locator('.btn-create')).toHaveText('+ New Template');
});
