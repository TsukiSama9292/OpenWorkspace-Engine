# E2E Testing Guide

## Overview

This guide covers running E2E tests for the VNC UI, including integration tests with a live VNC server.

## Prerequisites

1. **Docker & Docker Compose** - For running KasmVNC containers
2. **Node.js 18+** - For running tests
3. **pnpm** - Package manager

## Quick Start

### 1. Start VNC Server

```bash
# From project root
docker compose up -d kasm kasm2 nginx
```

Wait for containers to be healthy:
```bash
docker compose ps
```

### 2. Build the UI

```bash
cd apps/vnc-ui
pnpm build
```

### 3. Run E2E Tests

```bash
# Run all E2E tests
pnpm test:e2e

# Open Playwright UI for interactive testing
pnpm test:e2e:ui
```

## Test Categories

### Page Load Tests
- Verifies page loads successfully
- Checks for console errors
- Validates HTTP status codes

### Layout Structure Tests
- Confirms main container renders
- Validates sidebar and viewport presence
- Checks viewport dimensions

### Sidebar Tests
- Tests expand/collapse functionality
- Verifies status indicator visibility
- Validates toggle button behavior

### Theme Tests
- Tests theme toggle functionality
- Verifies theme persistence in localStorage
- Validates CSS variable updates

### Responsive Design Tests
- Tests mobile viewport behavior (375x667)
- Validates automatic sidebar collapse
- Checks touch-friendly button sizes

### Keyboard Accessibility Tests
- Tests focus management
- Validates keyboard navigation
- Ensures screen reader compatibility

### Performance Tests
- Measures page load time (< 5s target)
- Checks for layout shifts (CLS < 0.1)

## Integration Testing with Live VNC

For tests that require an actual VNC connection:

### Prerequisites
- KasmVNC containers running on ports 6901, 6902
- nginx proxy configured on port 80/443

### Running Integration Tests

```bash
# Ensure VNC is running
docker compose up -d

# Run tests
pnpm test:e2e

# Or run specific test file
npx playwright test e2e/vnc-ui.test.ts
```

### Mocking VNC for Unit Tests

Unit tests use jsdom and mock the WebSocket connection. No VNC server needed.

## Bundle Analysis

Analyze build output sizes:

```bash
pnpm build
pnpm analyze
```

This will report:
- File sizes by type (JS, CSS, WASM, etc.)
- Gzip compression ratios
- Size warnings/thresholds

## Troubleshooting

### Tests fail with "Cannot find module"
```bash
pnpm install
```

### VNC connection refused
Ensure Docker containers are running:
```bash
docker compose ps
docker compose logs kasm
```

### Port conflicts
Check if ports 4173, 80, 443 are in use:
```bash
lsof -i :4173
lsof -i :80
```

### Playwright browsers not installed
```bash
npx playwright install chromium
```

## CI/CD Integration

For GitHub Actions or similar:

```yaml
- name: Run E2E Tests
  run: |
    docker compose up -d
    sleep 30  # Wait for VNC to start
    cd apps/vnc-ui
    pnpm build
    pnpm test:e2e
```

## Writing New Tests

### Test File Structure

```typescript
import { test, expect } from '@playwright/test';

test.describe('Feature Name', () => {
  test('test description', async ({ page }) => {
    await page.goto('/');
    // Test logic here
    await expect(element).toBeVisible();
  });
});
```

### Best Practices

1. Use descriptive test names
2. Group related tests with `test.describe`
3. Use locators with semantic selectors (`.class`, `[role]`, `text=`)
4. Avoid hard-coded waits - use `waitFor` utilities
5. Test both happy paths and error cases
6. Keep tests independent - no shared state

### Available Locators

```typescript
// By CSS class
page.locator('.sidebar')

// By role
page.locator('button')

// By text
page.locator('text=Settings')

// By attribute
page.locator('[title="Clipboard"]')

// Chained
page.locator('.sidebar').locator('button')
```

## Coverage

Current E2E test coverage:
- ✅ Page load and rendering
- ✅ Sidebar functionality
- ✅ Theme switching
- ✅ Responsive design
- ✅ Keyboard accessibility
- ✅ Performance metrics
- ⏳ VNC connection (requires live server)
- ⏳ Clipboard operations (requires user interaction)
- ⏳ Settings persistence (requires localStorage)
