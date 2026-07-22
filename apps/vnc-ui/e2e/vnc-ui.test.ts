import { test, expect } from '@playwright/test';

test.describe('VNC UI', () => {
  test('loads the page', async ({ page }) => {
    await page.goto('/');
    await expect(page).toHaveTitle(/VNC/);
  });

  test('renders status bar', async ({ page }) => {
    await page.goto('/');
    const statusBar = page.locator('.status-bar');
    await expect(statusBar).toBeVisible();
  });

  test('renders canvas element', async ({ page }) => {
    await page.goto('/');
    const canvas = page.locator('.vnc-canvas');
    await expect(canvas).toBeVisible();
  });

  test('shows ready status', async ({ page }) => {
    await page.goto('/');
    const statusText = page.locator('.status-text');
    await expect(statusText).toHaveText('ready');
  });
});
