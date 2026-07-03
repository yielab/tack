import { test, expect } from '@playwright/test';
import AxeBuilder from '@axe-core/playwright';
import { getOrCreateProject, getOrCreateItem, waitForApp } from './helpers';

// Accessibility scans (WCAG 2.0/2.1 A & AA) on the key surfaces. axe-core finds
// the machine-detectable ~40% of issues: contrast, missing labels, ARIA misuse,
// non-focusable controls. Run only on chromium — a11y is engine-independent and
// scanning three times adds noise without coverage.
//
// New violations fail CI. To triage existing debt without blocking, add the
// rule id to KNOWN_ISSUES with a tracking note rather than deleting the assertion.

test.skip(({ browserName }) => browserName !== 'chromium', 'a11y scan runs on chromium only');

// Suppress known, justified violations here ONLY so the gate keeps blocking
// *new* classes of regression. Add an axe rule id with a tracking note rather
// than deleting the assertion; remove it once the underlying issue is fixed.
// (Currently empty — the initial color-contrast and select-name findings are
// fixed: see index.css token darkening and the Sidebar select aria-label.)
const KNOWN_ISSUES: string[] = [];

async function scan(page: import('@playwright/test').Page) {
  const results = await new AxeBuilder({ page })
    .withTags(['wcag2a', 'wcag2aa', 'wcag21a', 'wcag21aa'])
    .disableRules(KNOWN_ISSUES)
    .analyze();
  return results.violations;
}

test('home page has no accessibility violations', async ({ page }) => {
  await page.goto('/');
  await waitForApp(page);
  const violations = await scan(page);
  expect(violations, JSON.stringify(violations.map((v) => v.id), null, 2)).toEqual([]);
});

test('board view has no accessibility violations', async ({ page, request }) => {
  const projectId = await getOrCreateProject(request);
  await page.goto(`/projects/${projectId}/board`);
  await waitForApp(page);
  const violations = await scan(page);
  expect(violations, JSON.stringify(violations.map((v) => v.id), null, 2)).toEqual([]);
});

test('global settings has no accessibility violations', async ({ page }) => {
  await page.goto('/settings');
  await waitForApp(page);
  const violations = await scan(page);
  expect(violations, JSON.stringify(violations.map((v) => v.id), null, 2)).toEqual([]);
});

// Every work lens is a distinct surface with its own controls (drag handles,
// grids, legends). Scan each so a regression in one view can't hide behind a
// clean board scan.
for (const lens of ['table', 'timeline', 'calendar', 'sprint'] as const) {
  test(`${lens} view has no accessibility violations`, async ({ page, request }) => {
    const projectId = await getOrCreateProject(request);
    await getOrCreateItem(request, projectId);
    await page.goto(`/projects/${projectId}/${lens}`);
    await waitForApp(page);
    const violations = await scan(page);
    expect(violations, JSON.stringify(violations.map((v) => v.id), null, 2)).toEqual([]);
  });
}

test('item detail drawer has no accessibility violations', async ({ page, request }) => {
  const projectId = await getOrCreateProject(request);
  const itemId = await getOrCreateItem(request, projectId);
  // The drawer is driven by the `item` search param on any project route.
  await page.goto(`/projects/${projectId}/board?item=${itemId}`);
  await waitForApp(page);
  // Wait for the drawer to mount (lazy-loaded) before scanning.
  await expect(page.getByRole('dialog')).toBeVisible();
  const violations = await scan(page);
  expect(violations, JSON.stringify(violations.map((v) => v.id), null, 2)).toEqual([]);
});
