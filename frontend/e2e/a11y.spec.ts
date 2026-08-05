import { test, expect } from '@playwright/test';
import AxeBuilder from '@axe-core/playwright';
import { getOrCreateProject, getOrCreateItem, createItemWithAssignee, waitForApp } from './helpers';

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

// TODO.md §6 "A12": no fixture here ever set an `assignee`, so `<Avatar>`
// (shared/ui/Avatar.tsx) — white initials over a per-name `hsl()` chip — never
// actually rendered during a scan. ~56% of the generated hues failed AA
// against fixed white text; this went undetected for months. "Avery Green"
// (hue 58) sits right in that failing band (old white-on-bg contrast ~2.1:1),
// so this fixture actually exercises the fix rather than getting lucky with a
// hue that happened to pass even under the bug.
test('board view with an assigned item (populated avatar) has no accessibility violations', async ({
  page,
  request,
}) => {
  const projectId = await getOrCreateProject(request);
  await createItemWithAssignee(request, projectId, 'Avery Green');
  await page.goto(`/projects/${projectId}/board`);
  await waitForApp(page);
  await expect(page.getByTitle('Avery Green').first()).toBeVisible();
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

// Fleet view (frontend/src/features/fleet/**, TODO.md §6 "A5"). `GET
// /api/fleet` 404s unless the server is started with TACK_ORCH_ENABLE=true —
// this e2e harness's webServer (playwright.config.ts) does not set it, and
// that file isn't owned by this spec, so the disabled state below is the
// real, unmodified default a fresh install sees. There is also no
// control-plane-registration UI yet (A5's handoff, TODO.md §6) to seed a
// populated row through the API even if orchestration were on. So the
// populated scan below intercepts the browser's `GET /api/fleet` request
// directly (`page.route`) and fulfills it with a payload shaped exactly like
// `frontend/src/features/fleet/api.ts`'s `FleetResponse`/`FleetRow` — the
// file A5's card designates as the single source of truth for the wire
// shape. This still renders and scans the real `FleetPage`/`FleetRow`/
// `HealthChip` components against real (mocked-network) data, covering all
// four `ControlPlaneHealth` states — including the stale/dashed-field
// treatment for `unreachable`/`unknown` rows, which is the card's central
// accessibility-relevant decision (no opacity-dimming, no confident-looking
// zero). It does not exercise the real `tack-orch` reconciler or `GET
// /api/fleet` handler themselves — only the frontend's rendering of their
// documented contract.

test('fleet page (orchestration disabled) has no accessibility violations', async ({ page }) => {
  await page.goto('/fleet');
  await waitForApp(page);
  // Assert the real disabled-state copy rendered — a blank or broken page
  // must not slip through as a "clean" scan just because nothing was there
  // to flag.
  await expect(page.getByText('Agent-fleet orchestration is disabled')).toBeVisible();
  const violations = await scan(page);
  expect(violations, JSON.stringify(violations.map((v) => v.id), null, 2)).toEqual([]);
});

test('fleet page (populated) has no accessibility violations', async ({ page }) => {
  const now = new Date();
  const isoAgo = (ms: number) => new Date(now.getTime() - ms).toISOString();

  const mockResponse = {
    rows: [
      {
        project_id: 'e2e-fleet-healthy',
        project_name: 'Fleet Row Healthy',
        control_plane_id: 'cp-healthy',
        control_plane_name: 'docket-prod',
        control_plane_kind: 'docket',
        health: 'healthy',
        last_seen_at: isoAgo(30_000),
        consecutive_failures: 0,
        gateway: 'active',
        roster: [
          { id: 'agent-1', name: 'Backend Dev', role: 'backend-dev', model: 'claude-sonnet-5' },
          { id: 'agent-2', name: 'Reviewer', role: 'reviewer', model: 'claude-opus-5' },
        ],
        last_activity_at: isoAgo(30_000),
        tokens_in: 128_400,
        tokens_out: 45_200,
        cost_usd_estimated: 12.34,
        pricing_snapshot_at: isoAgo(86_400_000),
        budget_usd: 100,
        pending_approval_count: 2,
      },
      {
        project_id: 'e2e-fleet-degraded',
        project_name: 'Fleet Row Degraded',
        control_plane_id: 'cp-degraded',
        control_plane_name: 'docket-staging',
        control_plane_kind: 'docket',
        health: 'degraded',
        last_seen_at: isoAgo(5 * 60_000),
        consecutive_failures: 3,
        gateway: 'active',
        roster: [{ id: 'agent-3', name: 'Frontend Dev', role: 'frontend-dev', model: 'claude-sonnet-5' }],
        last_activity_at: isoAgo(5 * 60_000),
        tokens_in: 4200,
        tokens_out: 900,
        cost_usd_estimated: null,
        pricing_snapshot_at: null,
        budget_usd: null,
        pending_approval_count: 0,
      },
      {
        project_id: 'e2e-fleet-unreachable',
        project_name: 'Fleet Row Unreachable',
        control_plane_id: 'cp-unreachable',
        control_plane_name: 'docket-remote',
        control_plane_kind: 'docket',
        health: 'unreachable',
        last_seen_at: isoAgo(3_600_000),
        consecutive_failures: 12,
        gateway: 'inactive',
        roster: [{ id: 'agent-4', name: 'QA', role: 'qa', model: 'claude-haiku-5' }],
        last_activity_at: isoAgo(3_600_000),
        tokens_in: 9000,
        tokens_out: 2000,
        cost_usd_estimated: 3.5,
        pricing_snapshot_at: isoAgo(86_400_000),
        budget_usd: 50,
        pending_approval_count: 1,
      },
      {
        project_id: 'e2e-fleet-unknown',
        project_name: 'Fleet Row Unknown',
        control_plane_id: 'cp-unknown',
        control_plane_name: 'docket-new',
        control_plane_kind: 'docket',
        health: 'unknown',
        last_seen_at: null,
        consecutive_failures: 0,
        gateway: 'unknown',
        roster: [],
        last_activity_at: null,
        tokens_in: 0,
        tokens_out: 0,
        cost_usd_estimated: null,
        pricing_snapshot_at: null,
        budget_usd: null,
        pending_approval_count: 0,
      },
    ],
  };

  await page.route('**/api/fleet', (route) =>
    route.fulfill({ status: 200, contentType: 'application/json', body: JSON.stringify(mockResponse) }),
  );
  await page.goto('/fleet');
  await waitForApp(page);
  await expect(page.getByRole('table')).toBeVisible();
  await expect(page.getByText('Fleet Row Unreachable')).toBeVisible();
  const violations = await scan(page);
  expect(violations, JSON.stringify(violations.map((v) => v.id), null, 2)).toEqual([]);
});
