import { test, expect } from '@playwright/test';
import AxeBuilder from '@axe-core/playwright';
import {
  getOrCreateProject,
  getOrCreateItem,
  createFreshItem,
  createItemWithAssignee,
  createSprintWithItem,
  createAgentProfile,
  createExecution,
  enrollRunner,
  claimOnceWithLease,
  waitForApp,
  setPaletteAndTheme,
} from './helpers';

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

async function scan(page: import('@playwright/test').Page, extraDisabled: string[] = []) {
  const results = await new AxeBuilder({ page })
    .withTags(['wcag2a', 'wcag2aa', 'wcag21a', 'wcag21aa'])
    .disableRules([...KNOWN_ISSUES, ...extraDisabled])
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

// Palette × mode coverage. Design tokens live on two independent axes — mode
// (light/dark) × palette (teal/clay/graphite) — six combinations, and every
// scan above always runs under the one combination Playwright's default
// color scheme plus no stored palette produces: teal/light. The other five
// were never scanned by anything, ever, until this file added them; a hand
// probe found one of them (graphite/light) genuinely broken at the time —
// `--color-primary-600` used directly as text on light surfaces, a role only
// safe when the token is dark. Fixed at the token level (graphite's
// `primary-600` darkened, `on-accent` flipped to white, matching how
// teal/clay already pair a dark primary with white on-accent text) rather
// than worked around here, so all six cells below are unsuppressed gates.
//
// Scanning every existing page in every combination would multiply this
// file's cost six-fold for very little marginal signal — nearly every scan
// above exercises the same handful of tokens (surface, text, border, primary,
// semantic ramps) applied to different layouts, not different tokens. A
// single representative, chrome-heavy page exercises those tokens once per
// combination instead: the board view, which — via the persistent sidebar
// (`Sidebar.tsx`), breadcrumb (`Breadcrumb.tsx`) and work-lens tabs
// (`WorkTabs.tsx`) it renders in every project route — already covers most of
// the token surface a page-specific scan would add on top. This leaves every
// page-specific scan above running only ever under teal/light, and every
// feature-specific token usage those pages alone reach (e.g. economics'
// warning-band progress bar, the fleet health chips) unscanned under any
// other palette or mode.
const OTHER_MODES_AND_PALETTES: Array<['teal' | 'clay' | 'graphite', 'light' | 'dark']> = [
  ['teal', 'dark'],
  ['clay', 'light'],
  ['clay', 'dark'],
  ['graphite', 'dark'],
  ['graphite', 'light'],
];

for (const [palette, mode] of OTHER_MODES_AND_PALETTES) {
  test(`board view (${palette}/${mode}) has no accessibility violations`, async ({ page, request }) => {
    const projectId = await getOrCreateProject(request);
    await setPaletteAndTheme(page, palette, mode);
    await page.goto(`/projects/${projectId}/board`);
    await waitForApp(page);
    const violations = await scan(page);
    expect(violations, JSON.stringify(violations.map((v) => v.id), null, 2)).toEqual([]);
  });
}

// No other fixture in this file sets an `assignee`, so `<Avatar>`
// (`shared/ui/Avatar.tsx`) — initials over a per-name `hsl()` chip, with
// `textColorForHue` choosing black or white text per hue's own contrast
// against that background — never actually renders during any other scan.
// "Avery Green" gives a real, deterministic hue rather than a hand-picked
// one, so the scan below exercises whichever text color that hue resolves
// to, not just the component's presence.
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

// Agents page — Advanced section (frontend/src/features/agents/runnerFleet/**
// — a distinct, harness-agnostic execution runner from the legacy Docket
// scans above, which happen to share the word "fleet"). The operator
// execution/fleet/runner/profile routes this section calls
// (`/api/executions`, `/api/runner-fleets`, `/api/runners/*`,
// `/api/agent-profiles`) are NOT gated behind
// `TACK_ORCH_ENABLE` (`crates/tack-api/src/router.rs`'s
// `operator_execution_routes` is merged into `/api` independently of
// `orch_routes`) — so these scans hit the real, unmodified webServer with no
// `page.route` interception at all, including a genuine enroll round-trip
// against `POST /api/runners/enrollment`.
// Collapsed by default (its vocabulary lives only here), so every
// scan below opens it first.

async function openAdvanced(page: import('@playwright/test').Page) {
  await page.getByRole('button', { name: /Advanced/ }).click();
}

test('agents page — advanced section (default Runners tab, empty) has no accessibility violations', async ({
  page,
}) => {
  await page.goto('/agents');
  await waitForApp(page);
  await openAdvanced(page);
  await expect(page.getByRole('heading', { name: 'Enroll a runner' })).toBeVisible();
  await expect(page.getByRole('tablist')).toBeVisible();
  const violations = await scan(page);
  expect(violations, JSON.stringify(violations.map((v) => v.id), null, 2)).toEqual([]);
});

test('agents page — enrolling a runner and viewing the one-time token modal has no accessibility violations', async ({
  page,
}) => {
  await page.goto('/agents');
  await waitForApp(page);
  await openAdvanced(page);

  const runnerName = `e2e-runner-${Date.now()}`;
  await page.getByLabel('Name').fill(runnerName);
  await page.getByRole('button', { name: 'Enroll', exact: true }).click();

  await expect(page.getByRole('dialog', { name: 'Runner enrollment token' })).toBeVisible();
  await expect(page.getByText('Shown once')).toBeVisible();
  // The real enrollment token is a real secret round-tripped from the live
  // API — assert SOME token text rendered (not empty), without hard-coding
  // its value.
  await expect(page.getByText(/^enr_/)).toBeVisible();

  const violations = await scan(page);
  expect(violations, JSON.stringify(violations.map((v) => v.id), null, 2)).toEqual([]);

  // Close and confirm the roster now shows the runner as unconfirmed —
  // never a fabricated "Healthy" reading.
  await page.getByRole('button', { name: "I've copied it — close" }).click();
  await expect(page.getByRole('dialog')).toHaveCount(0);
  await expect(page.getByText(runnerName, { exact: true })).toBeVisible();
  await expect(page.getByText('Connection unconfirmed')).toBeVisible();

  const violationsAfterClose = await scan(page);
  expect(violationsAfterClose, JSON.stringify(violationsAfterClose.map((v) => v.id), null, 2)).toEqual([]);
});

test('agents page — Fleets/Agent profiles tabs have no accessibility violations', async ({ page }) => {
  await page.goto('/agents');
  await waitForApp(page);
  await openAdvanced(page);

  await page.getByRole('tab', { name: 'Fleets' }).click();
  await expect(page.getByRole('tabpanel')).toBeVisible();
  let violations = await scan(page);
  expect(violations, JSON.stringify(violations.map((v) => v.id), null, 2)).toEqual([]);

  await page.getByRole('tab', { name: 'Agent profiles' }).click();
  violations = await scan(page);
  expect(violations, JSON.stringify(violations.map((v) => v.id), null, 2)).toEqual([]);
});

test('agents page — creating a fleet via the form has no accessibility violations', async ({ page }) => {
  await page.goto('/agents');
  await waitForApp(page);
  await openAdvanced(page);

  await page.getByRole('tab', { name: 'Fleets' }).click();
  await page.getByRole('button', { name: '+ Create fleet' }).click();
  const fleetName = `e2e-fleet-${Date.now()}`;
  await page.getByLabel('Name').fill(fleetName);
  await page.getByRole('button', { name: 'Create', exact: true }).click();
  await expect(page.getByText(fleetName, { exact: true })).toBeVisible();

  const violations = await scan(page);
  expect(violations, JSON.stringify(violations.map((v) => v.id), null, 2)).toEqual([]);
});

// Dispatch UI (`frontend/src/shared/dispatch/**`). Same technique as the
// Fleet scans above: `TACK_ORCH_ENABLE` isn't set for this harness's
// webServer, so every dispatch route 404s by default — exactly the "no
// dispatch controls" state every other scan in this file already covers
// incidentally (none of them ever see a dispatch button, since it only
// renders once its own orchestration probe succeeds). These tests intercept
// the specific orch routes each surface depends on to render its *enabled*
// state, so axe actually scans the dispatch button, the per-outcome note,
// and the sprint dry-run/results modal — not just their absence.

test('item detail drawer with the dispatch control visible has no accessibility violations', async ({
  page,
  request,
}) => {
  const projectId = await getOrCreateProject(request);
  const itemId = await getOrCreateItem(request, projectId);

  await page.route(`**/api/items/${itemId}/agent-activity`, (route) =>
    route.fulfill({
      status: 200,
      contentType: 'application/json',
      body: JSON.stringify({ attempts: [], approvals: [], events_truncated: false, events_retention_days: 90 }),
    }),
  );

  await page.goto(`/projects/${projectId}/board?item=${itemId}`);
  await waitForApp(page);
  await expect(page.getByRole('dialog')).toBeVisible();
  await expect(page.getByRole('button', { name: 'Dispatch to agents' })).toBeVisible();

  const violations = await scan(page);
  expect(violations, JSON.stringify(violations.map((v) => v.id), null, 2)).toEqual([]);
});

test('item detail drawer after a blocked dispatch outcome has no accessibility violations', async ({
  page,
  request,
}) => {
  const projectId = await getOrCreateProject(request);
  const itemId = await getOrCreateItem(request, projectId);

  await page.route(`**/api/items/${itemId}/agent-activity`, (route) =>
    route.fulfill({
      status: 200,
      contentType: 'application/json',
      body: JSON.stringify({ attempts: [], approvals: [], events_truncated: false, events_retention_days: 90 }),
    }),
  );
  await page.route(`**/api/items/${itemId}/dispatch`, (route) =>
    route.fulfill({
      status: 200,
      contentType: 'application/json',
      body: JSON.stringify({
        outcome: 'blocked',
        task: null,
        policy_id: 'prompt-injection',
        message: 'destructive shell command in task description',
      }),
    }),
  );

  await page.goto(`/projects/${projectId}/board?item=${itemId}`);
  await waitForApp(page);
  await page.getByRole('button', { name: 'Dispatch to agents' }).click();
  // The blocked outcome must name the policy, not just say "blocked" — this
  // is a visible marker the note actually rendered in the item-details
  // surface before scanning. The same policy is also repeated in a transient
  // notification, so keep this locator scoped to the dialog rather than
  // relying on a globally unique text match.
  await expect(
    page.getByRole('dialog', { name: 'Item details' }).getByText('prompt-injection'),
  ).toBeVisible();

  const violations = await scan(page);
  expect(violations, JSON.stringify(violations.map((v) => v.id), null, 2)).toEqual([]);
});

test('sprint "Run sprint" dry-run preview has no accessibility violations', async ({ page, request }) => {
  const projectId = await getOrCreateProject(request);
  // `getOrCreateProject` reuses one shared project across this whole spec
  // file (and across the two sprint-dispatch tests specifically), and
  // `createSprintWithItem` always creates a fresh sprint rather than
  // reusing one — so a sprint left over from another test can still be
  // "active" (non-closed) with items assigned, and would render its own,
  // equally legitimate "Run sprint" button (`Sprints.tsx` renders one button
  // per eligible sprint, by design). A unique sprint name plus an accessible
  // name that includes it (`Run sprint: <name>`) is what disambiguates the
  // two real buttons instead of relying on there being exactly one sprint in
  // the project.
  const sprintName = 'E2E Sprint (dry-run preview)';
  const { sprintId, sprintName: uniqueSprintName } = await createSprintWithItem(
    request,
    projectId,
    sprintName,
  );

  // `useAgentActivityMap`'s bulk fetch is `Sprints.tsx`'s own "is
  // orchestration enabled" gate for the "Run sprint" button — the same probe
  // `Board.tsx` uses.
  await page.route(`**/api/projects/${projectId}/agent-activity`, (route) =>
    route.fulfill({ status: 200, contentType: 'application/json', body: JSON.stringify({ rows: [] }) }),
  );
  await page.route(`**/api/sprints/${sprintId}/dispatch/dry-run`, (route) =>
    route.fulfill({
      status: 200,
      contentType: 'application/json',
      body: JSON.stringify({
        sprint_id: sprintId,
        max_in_flight: 5,
        summary: {
          total: 3,
          dispatched: 0,
          waiting_approval: 0,
          blocked: 0,
          already_in_flight: 0,
          waiting_on_dependencies: 1,
          not_eligible: 0,
          no_dispatch_policy: 0,
          would_dispatch: 2,
          errored: 0,
        },
        items: [
          { item_id: 'a', title: 'Design schema', status: 'Ready', order: 0, decision: 'would_dispatch', blocked_by: null, policy_id: null, message: null, status_applied: null, status_map_rejected: null, approval_token: null, current_status: null, dispatch_from: null, error: null, task: null },
          { item_id: 'b', title: 'Build API', status: 'Ready', order: 1, decision: 'would_dispatch', blocked_by: null, policy_id: null, message: null, status_applied: null, status_map_rejected: null, approval_token: null, current_status: null, dispatch_from: null, error: null, task: null },
          { item_id: 'c', title: 'Write docs', status: 'Ready', order: 2, decision: 'waiting_on_dependencies', blocked_by: ['a'], policy_id: null, message: null, status_applied: null, status_map_rejected: null, approval_token: null, current_status: null, dispatch_from: null, error: null, task: null },
        ],
      }),
    }),
  );

  await page.goto(`/projects/${projectId}/sprint`);
  await waitForApp(page);
  await page.getByRole('button', { name: `Run sprint: ${uniqueSprintName}` }).click();
  await expect(page.getByRole('dialog')).toBeVisible();
  await expect(page.getByText('Design schema')).toBeVisible();

  const violations = await scan(page);
  expect(violations, JSON.stringify(violations.map((v) => v.id), null, 2)).toEqual([]);
});

test('sprint dispatch results (mixed outcomes) has no accessibility violations', async ({ page, request }) => {
  const projectId = await getOrCreateProject(request);
  // See the sibling "dry-run preview" test above for why this needs its own
  // unique sprint name: the project is shared across this spec file, so a
  // sprint from another test can still be eligible for its own, distinct
  // "Run sprint" button.
  const sprintName = 'E2E Sprint (dispatch results)';
  const { sprintId, sprintName: uniqueSprintName } = await createSprintWithItem(
    request,
    projectId,
    sprintName,
  );

  await page.route(`**/api/projects/${projectId}/agent-activity`, (route) =>
    route.fulfill({ status: 200, contentType: 'application/json', body: JSON.stringify({ rows: [] }) }),
  );
  await page.route(`**/api/sprints/${sprintId}/dispatch/dry-run`, (route) =>
    route.fulfill({
      status: 200,
      contentType: 'application/json',
      body: JSON.stringify({
        sprint_id: sprintId,
        max_in_flight: 2,
        summary: {
          total: 2,
          dispatched: 0,
          waiting_approval: 0,
          blocked: 0,
          already_in_flight: 0,
          waiting_on_dependencies: 0,
          not_eligible: 0,
          no_dispatch_policy: 0,
          would_dispatch: 2,
          errored: 0,
        },
        items: [
          { item_id: 'a', title: 'Item A', status: 'Ready', order: 0, decision: 'would_dispatch', blocked_by: null, policy_id: null, message: null, status_applied: null, status_map_rejected: null, approval_token: null, current_status: null, dispatch_from: null, error: null, task: null },
          { item_id: 'b', title: 'Item B', status: 'Ready', order: 1, decision: 'would_dispatch', blocked_by: null, policy_id: null, message: null, status_applied: null, status_map_rejected: null, approval_token: null, current_status: null, dispatch_from: null, error: null, task: null },
        ],
      }),
    }),
  );
  // Trailing `*` (not `**`) is required: `DispatchSprintModal`'s "Confirm
  // dispatch" pre-fills the in-flight cap from the dry-run response and
  // always sends it as a query param (card C4's fix #1 — `?max_in_flight=N`,
  // never a JSON body), so the real POST URL is
  // `.../dispatch?max_in_flight=2`, not the bare path. Playwright glob
  // routes are anchored (`^...$`), so an unqualified `.../dispatch` pattern
  // never matches a URL with a query string and this route silently never
  // fires, falling through to the real (orchestration-disabled) backend. A
  // single `*` is enough since query strings never contain `/`, so this
  // still can't accidentally swallow the sibling `.../dispatch/dry-run`
  // route registered above.
  await page.route(`**/api/sprints/${sprintId}/dispatch*`, (route) => {
    if (route.request().method() !== 'POST') return route.fallback();
    return route.fulfill({
      status: 200,
      contentType: 'application/json',
      body: JSON.stringify({
        sprint_id: sprintId,
        max_in_flight: 2,
        summary: {
          total: 2,
          dispatched: 1,
          waiting_approval: 1,
          blocked: 0,
          already_in_flight: 0,
          waiting_on_dependencies: 0,
          not_eligible: 0,
          no_dispatch_policy: 0,
          would_dispatch: 0,
          errored: 0,
        },
        items: [
          { item_id: 'a', title: 'Item A', status: 'Ready', order: 0, decision: 'dispatched', blocked_by: null, policy_id: null, message: null, status_applied: 'In Progress', status_map_rejected: null, approval_token: null, current_status: null, dispatch_from: null, error: null, task: null },
          { item_id: 'b', title: 'Item B', status: 'Ready', order: 1, decision: 'waiting_approval', blocked_by: null, policy_id: null, message: null, status_applied: null, status_map_rejected: null, approval_token: 'tok-1', current_status: null, dispatch_from: null, error: null, task: null },
        ],
      }),
    });
  });

  await page.goto(`/projects/${projectId}/sprint`);
  await waitForApp(page);
  await page.getByRole('button', { name: `Run sprint: ${uniqueSprintName}` }).click();
  await expect(page.getByRole('dialog')).toBeVisible();
  await page.getByRole('button', { name: /Confirm dispatch/ }).click();
  // Never a merged "2 dispatched" — the two outcomes stay in separate, named counts.
  await expect(page.getByText('1 waiting on approval')).toBeVisible();

  const violations = await scan(page);
  expect(violations, JSON.stringify(violations.map((v) => v.id), null, 2)).toEqual([]);
});

// "Run with agent" (`frontend/src/shared/runWithAgent/**`). Unlike every
// Docket dispatch scan above, these routes are NOT gated behind
// `TACK_ORCH_ENABLE` — no orchestration setup needed to reach this surface's
// populated state. The two states scanned are the highest-risk ones for
// focus/labelling per this file's own established precedent (a modal with
// several distinct field groups, and a tab panel rendering a list with
// inline action controls).

test('Board card "Run with agent" modal has no accessibility violations', async ({ page, request }) => {
  const projectId = await getOrCreateProject(request);
  await getOrCreateItem(request, projectId, `A11y RWA board ${Date.now()}`);

  await page.goto(`/projects/${projectId}/board`);
  await waitForApp(page);
  await page.getByRole('button', { name: /^Run with agent:/ }).first().click();
  await expect(page.getByRole('dialog', { name: /^Run with agent:/ })).toBeVisible();

  const violations = await scan(page);
  expect(violations, JSON.stringify(violations.map((v) => v.id), null, 2)).toEqual([]);
});

test('item detail Execution tab (with a real request) has no accessibility violations', async ({ page, request }) => {
  const projectId = await getOrCreateProject(request);
  // A guaranteed-fresh item: `getOrCreateItem` reuses one shared item across
  // a whole spec file, which would accumulate execution requests across
  // repeated runs instead of giving this test its own clean state.
  const itemId = await createFreshItem(request, projectId, `A11y RWA detail ${Date.now()}`);
  const profileId = await createAgentProfile(request, `A11y Profile ${Date.now()}`);
  // An exact runner, not a fleet: naming the runner directly is the shortest
  // setup that leaves the Execution tab populated for the scan, without a
  // fleet and a membership write first. Same choice, same reason, as
  // `scheduler-e2e.spec.ts`.
  const modelId = `opaque/model-a11y-${Date.now()}`;
  const { runnerId } = await enrollRunner(request, `A11y RWA Runner ${Date.now()}`, modelId);

  await page.goto(`/projects/${projectId}/board?item=${itemId}`);
  await waitForApp(page);
  const drawer = page.getByRole('dialog');
  await drawer.getByRole('button', { name: 'Run with agent' }).click();

  const modal = page.getByRole('dialog', { name: /^Run with agent:/ });
  // "Where it runs" disappears entirely whenever exactly one runner is
  // active and no fleet exists (`RunWithAgentModal.tsx`'s
  // `hideTargetPicker`) — select this test's own runner explicitly only
  // when the picker actually renders, matching how `scheduler-e2e.spec.ts`'s
  // `fillExactRunnerTarget` handles the same picker.
  const picker = modal.getByRole('combobox', { name: 'Machine or group' });
  if ((await picker.count()) > 0) {
    await picker.selectOption(`exact_runner:${runnerId}`);
  }
  await modal.getByRole('combobox', { name: 'Agent profile' }).selectOption(profileId);
  // The repository fieldset is a read-only summary until "Change for this
  // run" is clicked — the free-text Remote field doesn't exist in the DOM
  // before that.
  await modal.getByRole('button', { name: 'Change for this run' }).click();
  await modal.getByLabel('Remote').fill('git@example.com:org/repo.git');
  await modal.getByLabel('Base revision').fill('a11y-rwa-detail');
  // A specific, matching model choice: with no default model configured
  // anywhere (agent profile / project / fleet), the live-capability gate
  // refuses to submit an unresolved "Auto" request — it would queue
  // forever. The target declares exactly one combination (`enrollRunner`'s
  // fixed capability shape), so it is always index "0".
  await modal.getByLabel('Choose…').check();
  await modal.getByRole('combobox', { name: 'Model' }).selectOption('0');
  await modal.getByRole('button', { name: 'Run' }).click();
  await expect(modal).toBeHidden();

  await drawer.getByRole('tab', { name: 'Execution' }).click();
  await expect(drawer.getByText('Queued')).toBeVisible();

  const violations = await scan(page);
  expect(violations, JSON.stringify(violations.map((v) => v.id), null, 2)).toEqual([]);
});

// The attempt-detail panel (`AttemptList.tsx` — model provenance, usage
// economics, and the expanded events/decisions/artifacts sections). Scans
// with a real claimed attempt so the expanded state (radios, text fields,
// buttons across `EventTimeline`/`DecisionInbox`/`ArtifactDownloadPanel`) is
// actually present in the DOM, not just the collapsed row.
test('item detail Execution tab — expanded attempt detail (events/decisions/artifacts) has no accessibility violations', async ({
  page,
  request,
}) => {
  const projectId = await getOrCreateProject(request);
  const itemId = await createFreshItem(request, projectId, `A11y attempt detail ${Date.now()}`);
  const profileId = await createAgentProfile(request, `A11y Attempt Profile ${Date.now()}`);
  const modelId = 'opaque/model-alpha';

  const { runnerId, credential } = await enrollRunner(request, `A11y Attempt Runner ${Date.now()}`, modelId);
  const requestId = await createExecution(request, itemId, runnerId, profileId, modelId);
  const lease = await claimOnceWithLease(request, runnerId, credential, `a11y-attempt-claim-${Date.now()}`);
  expect(lease?.requestId).toBe(requestId);

  await page.goto(`/projects/${projectId}/board?item=${itemId}`);
  await waitForApp(page);
  const drawer = page.getByRole('dialog');
  await drawer.getByRole('tab', { name: 'Execution' }).click();
  await expect(drawer.getByText('Attempt #1')).toBeVisible();

  await drawer.getByRole('button', { name: /Show events, decisions & artifacts/ }).click();
  await expect(drawer.getByText('No events reported yet')).toBeVisible();

  const violations = await scan(page);
  expect(violations, JSON.stringify(violations.map((v) => v.id), null, 2)).toEqual([]);
});
