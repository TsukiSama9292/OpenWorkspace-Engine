import { test, expect } from '@playwright/test';

test.describe('VNC UI - Page Load', () => {
  test('loads the page successfully', async ({ page }) => {
    await page.goto('/');
    await expect(page).toHaveTitle(/VNC/);
  });

  test('has no console errors on load', async ({ page }) => {
    const errors: string[] = [];
    page.on('console', msg => {
      if (msg.type() === 'error') {
        errors.push(msg.text());
      }
    });
    await page.goto('/');
    await page.waitForTimeout(2000);
    expect(errors).toHaveLength(0);
  });

  test('page responds with 200 status', async ({ page }) => {
    const response = await page.goto('/');
    expect(response?.status()).toBe(200);
  });
});

test.describe('VNC UI - Layout Structure', () => {
  test('renders main container', async ({ page }) => {
    await page.goto('/');
    const container = page.locator('.vnc-container');
    await expect(container).toBeVisible();
  });

  test('renders sidebar', async ({ page }) => {
    await page.goto('/');
    const sidebar = page.locator('.sidebar');
    await expect(sidebar).toBeVisible();
  });

  test('renders VNC viewport', async ({ page }) => {
    await page.goto('/');
    const viewport = page.locator('.vnc-viewport');
    await expect(viewport).toBeVisible();
  });

  test('viewport takes full remaining width', async ({ page }) => {
    await page.goto('/');
    const viewport = page.locator('.vnc-viewport');
    const box = await viewport.boundingBox();
    const viewportWidth = await page.evaluate(() => window.innerWidth);
    expect(box?.width).toBeGreaterThan(viewportWidth * 0.8);
  });
});

test.describe('VNC UI - Sidebar', () => {
  test('sidebar starts expanded', async ({ page }) => {
    await page.goto('/');
    const sidebar = page.locator('.sidebar');
    await expect(sidebar).not.toHaveClass(/collapsed/);
  });

  test('toggle button collapses sidebar', async ({ page }) => {
    await page.goto('/');
    const toggleBtn = page.locator('.toggle-btn');
    await toggleBtn.click();
    const sidebar = page.locator('.sidebar');
    await expect(sidebar).toHaveClass(/collapsed/);
  });

  test('toggle button expands collapsed sidebar', async ({ page }) => {
    await page.goto('/');
    const toggleBtn = page.locator('.toggle-btn');
    await toggleBtn.click();
    await expect(page.locator('.sidebar')).toHaveClass(/collapsed/);
    await toggleBtn.click();
    await expect(page.locator('.sidebar')).not.toHaveClass(/collapsed/);
  });

  test('shows status indicator', async ({ page }) => {
    await page.goto('/');
    const statusDot = page.locator('.status-dot');
    await expect(statusDot).toBeVisible();
  });

  test('shows status label', async ({ page }) => {
    await page.goto('/');
    const statusLabel = page.locator('.status-label');
    await expect(statusLabel).toBeVisible();
  });
});

test.describe('VNC UI - Theme', () => {
  test('has theme toggle button', async ({ page }) => {
    await page.goto('/');
    const themeBtn = page.locator('.theme-btn');
    await expect(themeBtn).toBeVisible();
  });

  test('toggles theme on click', async ({ page }) => {
    await page.goto('/');
    const html = page.locator('html');
    const initialTheme = await html.getAttribute('data-theme');
    
    const themeBtn = page.locator('.theme-btn');
    await themeBtn.click();
    
    const newTheme = await html.getAttribute('data-theme');
    expect(newTheme).not.toBe(initialTheme);
  });

  test('theme persists after page reload', async ({ page }) => {
    await page.goto('/');
    const html = page.locator('html');
    
    const themeBtn = page.locator('.theme-btn');
    await themeBtn.click();
    const themeAfterToggle = await html.getAttribute('data-theme');
    
    await page.reload();
    const themeAfterReload = await html.getAttribute('data-theme');
    expect(themeAfterReload).toBe(themeAfterToggle);
  });
});

test.describe('VNC UI - Responsive Design', () => {
  test('sidebar collapses on mobile viewport', async ({ page }) => {
    await page.setViewportSize({ width: 375, height: 667 });
    await page.goto('/');
    const sidebar = page.locator('.sidebar');
    await expect(sidebar).toHaveClass(/collapsed/);
  });

  test('toggle button visible on mobile', async ({ page }) => {
    await page.setViewportSize({ width: 375, height: 667 });
    await page.goto('/');
    const toggleBtn = page.locator('.toggle-btn');
    await expect(toggleBtn).toBeVisible();
  });

  test('viewport fills screen on mobile', async ({ page }) => {
    await page.setViewportSize({ width: 375, height: 667 });
    await page.goto('/');
    const viewport = page.locator('.vnc-viewport');
    await expect(viewport).toBeVisible();
    const box = await viewport.boundingBox();
    expect(box?.width).toBeGreaterThan(300);
  });
});

test.describe('VNC UI - Keyboard Accessibility', () => {
  test('toggle button is focusable', async ({ page }) => {
    await page.goto('/');
    const toggleBtn = page.locator('.toggle-btn');
    await toggleBtn.focus();
    await expect(toggleBtn).toBeFocused();
  });

  test('can toggle sidebar with keyboard', async ({ page }) => {
    await page.goto('/');
    const toggleBtn = page.locator('.toggle-btn');
    await toggleBtn.focus();
    await page.keyboard.press('Enter');
    const sidebar = page.locator('.sidebar');
    await expect(sidebar).toHaveClass(/collapsed/);
  });
});

test.describe('VNC UI - Performance', () => {
  test('page loads within 5 seconds', async ({ page }) => {
    const startTime = Date.now();
    await page.goto('/');
    await page.waitForLoadState('networkidle');
    const loadTime = Date.now() - startTime;
    expect(loadTime).toBeLessThan(5000);
  });

  test('no layout shift on initial load', async ({ page }) => {
    await page.goto('/');
    await page.waitForLoadState('networkidle');
    
    const layoutShift = await page.evaluate(() => {
      return new Promise<number>(resolve => {
        let shift = 0;
        const observer = new PerformanceObserver(list => {
          for (const entry of list.getEntries()) {
            if ('hadRecentInput' in entry && !entry.hadRecentInput) {
              shift += entry.value;
            }
          }
        });
        observer.observe({ type: 'layout-shift', buffered: true });
        setTimeout(() => {
          observer.disconnect();
          resolve(shift);
        }, 1000);
      });
    });
    
    expect(layoutShift).toBeLessThan(0.1);
  });
});
