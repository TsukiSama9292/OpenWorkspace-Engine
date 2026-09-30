import { expect, type Page, test } from '@playwright/test';

// Full-stack verification for the administration pages (ticket 03): groups,
// users, and settings render on the shared chrome with working filters,
// plus phone captures per page.

const ADMIN = { username: 'admin', password: 'admin' };

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

async function openTab(page: Page, name: string): Promise<void> {
  await page.locator('.sidebar .nav-item').filter({ hasText: name }).click();
}

test('groups tab renders the management table with working filters', async ({ page }) => {
  await loginAs(page, ADMIN.username, ADMIN.password);
  await openTab(page, 'Groups');

  await expect(page.locator('.panel-head-title')).toContainText(/Group Management/, {
    timeout: 15_000,
  });
  await expect(page.locator('.instances-table')).toBeVisible();
  await expect(page.locator('.instances-table th', { hasText: 'Permissions' })).toBeVisible();

  await page.locator('.panel-search').fill('Admin');
  await expect(page.locator('.instances-table tbody tr').first()).toContainText(/Admin/);

  await page.setViewportSize({ width: 390, height: 844 });
  await page.waitForTimeout(500);
  await expect(page.locator('.panel-head-title')).toBeVisible();
  await page.screenshot({ path: 'test-results/web-surface-admin/groups-phone.png' });
});

test('users tab surfaces memberships and ceilings', async ({ page }) => {
  await loginAs(page, ADMIN.username, ADMIN.password);
  await openTab(page, 'Users');

  await expect(page.locator('.panel-head-title')).toContainText(/User Management/, {
    timeout: 15_000,
  });
  const row = page.locator('.instances-table tbody tr').first();
  await expect(row).toBeVisible();
  await expect(page.locator('.instances-table th', { hasText: 'Memberships' })).toBeVisible();
  await expect(page.locator('.instances-table th', { hasText: 'Personal Ceiling' })).toBeVisible();

  await page.setViewportSize({ width: 390, height: 844 });
  await page.waitForTimeout(500);
  await expect(row).toBeVisible();
  await page.screenshot({ path: 'test-results/web-surface-admin/users-phone.png' });
});

test('settings tab explains every option and saves without surprises', async ({ page }) => {
  await loginAs(page, ADMIN.username, ADMIN.password);
  await openTab(page, 'Settings');

  await expect(page.locator('.card-title')).toContainText(/Host Resource Policy/, {
    timeout: 15_000,
  });
  await expect(page.locator('.field-desc').first()).toContainText(/maximum number of instances/);

  // Saving unchanged values takes the direct path — no confirmation dialog.
  await page.locator('.card-footer button', { hasText: 'Save Changes' }).click();
  await expect(page.locator('.save-saved')).toContainText(/Saved/, { timeout: 15_000 });
  await expect(page.locator('[data-testid="confirm-dialog"]')).toHaveCount(0);

  await page.setViewportSize({ width: 390, height: 844 });
  await page.waitForTimeout(500);
  await expect(page.locator('.card-title')).toBeVisible();
  await page.screenshot({ path: 'test-results/web-surface-admin/settings-phone.png' });
});
