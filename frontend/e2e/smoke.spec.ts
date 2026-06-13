import { test, expect } from '@playwright/test';
import { getOrCreateProject, waitForApp } from './helpers';

// Smoke tests: every primary surface loads and renders without a blank screen
// or an uncaught error. Runs on chromium, firefox and webkit (see projects in
// playwright.config.ts) — this is the cross-browser safety net.

let projectId: string;

test.beforeAll(async ({ request }) => {
  projectId = await getOrCreateProject(request);
});

// Fail the test on any uncaught page error — a blank-but-200 page is still a bug.
test.beforeEach(async ({ page }) => {
  page.on('pageerror', (err) => {
    throw new Error(`Uncaught page error: ${err.message}`);
  });
});

test('home / projects page renders', async ({ page }) => {
  await page.goto('/');
  await waitForApp(page);
  await expect(page.locator('body')).not.toBeEmpty();
  // Some recognizable shell element (nav/header) must be present.
  await expect(page.locator('nav, header, [role="navigation"]').first()).toBeVisible();
});

const views = [
  { name: 'board', path: 'board' },
  { name: 'list', path: 'list' },
  { name: 'calendar', path: 'calendar' },
  { name: 'timeline', path: 'timeline' },
  { name: 'sprint', path: 'sprint' },
];

for (const view of views) {
  test(`project ${view.name} view renders`, async ({ page }) => {
    await page.goto(`/projects/${projectId}/${view.path}`);
    await waitForApp(page);
    await expect(page.locator('main, [role="main"], body > div').first()).toBeVisible();
  });
}

test('project overview and settings render', async ({ page }) => {
  await page.goto(`/projects/${projectId}/overview`);
  await waitForApp(page);
  await expect(page.locator('body')).not.toBeEmpty();

  await page.goto(`/projects/${projectId}/settings`);
  await waitForApp(page);
  await expect(page.locator('body')).not.toBeEmpty();
});

test('global settings and templates render', async ({ page }) => {
  await page.goto('/settings');
  await waitForApp(page);
  await expect(page.locator('body')).not.toBeEmpty();

  await page.goto('/templates');
  await waitForApp(page);
  await expect(page.locator('body')).not.toBeEmpty();
});

test('unknown route shows the 404 page (not a blank screen)', async ({ page }) => {
  await page.goto('/this-route-does-not-exist');
  await waitForApp(page);
  await expect(page.getByText(/not found/i)).toBeVisible();
});

test('mobile viewport renders the shell', async ({ page }) => {
  await page.setViewportSize({ width: 390, height: 844 });
  await page.goto('/');
  await waitForApp(page);
  await expect(page.locator('body')).not.toBeEmpty();
});
