import { test, expect } from '@playwright/test';
import {
  API,
  getOrCreateProject,
  getOrCreateItem,
  createFreshItem,
  createSprintWithItem,
  createFleet,
  createAgentProfile,
  waitForApp,
} from './helpers';

// "Run with agent" — item/sprint execution UI (TODO.md III-E4, Wave 4 /
// Phase 54). Distinct from the older Docket "Dispatch to agents"/"Run
// sprint" features covered by `journey.spec.ts`/`a11y.spec.ts`'s dispatch
// tests: this feature targets `/api/executions`, `/api/runner-fleets`,
// `/api/agent-profiles`, `/api/model-profiles` — an always-on operator
// surface, NOT gated behind `TACK_ORCH_ENABLE` (unlike every Docket dispatch
// route), so no orchestration-enable setup is needed for these specs. See
// `crates/tack-api/src/router.rs`'s own comment on `orch_routes` vs. card
// C1's operator execution/fleet routes.

test('Board: "Run with agent" opens the shared modal, and required-field reasons block submit', async ({
  page,
  request,
}) => {
  const projectId = await getOrCreateProject(request);
  const itemId = await getOrCreateItem(request, projectId, `RWA board item ${Date.now()}`);

  await page.goto(`/projects/${projectId}/board`);
  await waitForApp(page);

  const trigger = page.getByRole('button', { name: /^Run with agent:/ }).first();
  await expect(trigger).toBeVisible();
  await trigger.click();

  const dialog = page.getByRole('dialog', { name: /^Run with agent:/ });
  await expect(dialog).toBeVisible();

  // Nothing is filled in yet — the submit control must be disabled with
  // visible, specific reasons (this card's acceptance bar: "disabled +
  // reasoned, not merely rejected server-side").
  const runButton = dialog.getByRole('button', { name: 'Run' });
  await expect(runButton).toBeDisabled();
  await expect(dialog.getByText('Select a fleet.')).toBeVisible();
  await expect(dialog.getByText('Select an agent profile.')).toBeVisible();
  await expect(dialog.getByText('Enter a repository remote.')).toBeVisible();

  // Escape closes it — the same keyboard path every other modal in this app
  // supports (`shared/ui/Modal.tsx`), exercised here for this card's own
  // acceptance bar ("keyboard/focus path passes").
  await page.keyboard.press('Escape');
  await expect(dialog).toBeHidden();
  void itemId;
});

test('item-detail: submitting a run creates the request and it appears in the Execution tab without navigation', async ({
  page,
  request,
}) => {
  const projectId = await getOrCreateProject(request);
  // A guaranteed-fresh item (not `getOrCreateItem`, which would reuse
  // whatever item this project already has and accumulate a "Queued" badge
  // per past test run against the same persistent e2e.db).
  const itemId = await createFreshItem(request, projectId, `RWA detail item ${Date.now()}`);
  const fleetId = await createFleet(request, `E2E Fleet ${Date.now()}`);
  const profileId = await createAgentProfile(request, `E2E Profile ${Date.now()}`);

  await page.goto(`/projects/${projectId}/board?item=${itemId}`);
  await waitForApp(page);

  const drawer = page.getByRole('dialog');
  await expect(drawer).toBeVisible();

  await drawer.getByRole('button', { name: 'Run with agent' }).click();
  const modal = page.getByRole('dialog', { name: /^Run with agent:/ });
  await expect(modal).toBeVisible();

  // `getByLabel('Fleet')` would also match the "Fleet" *radio* button next
  // to the "Exact runner" radio — scope to the combobox role to get the
  // `<select>` specifically.
  await modal.getByRole('combobox', { name: 'Fleet' }).selectOption(fleetId);
  await modal.getByRole('combobox', { name: 'Agent profile' }).selectOption(profileId);
  await modal.getByLabel('Remote').fill('git@example.com:org/repo.git');

  const runButton = modal.getByRole('button', { name: 'Run' });
  await expect(runButton).toBeEnabled();
  await runButton.click();

  // The modal closes on success, with no page navigation — the request
  // shows up via the shared store, not a reload.
  await expect(modal).toBeHidden();
  await expect(page).toHaveURL(new RegExp(`item=${itemId}`));

  const executionTab = drawer.getByRole('tab', { name: 'Execution' });
  await executionTab.click();
  await expect(drawer.getByText('Queued')).toBeVisible();
  // III-F4 wired the real attempts endpoint (card III-E6 added it; this was
  // a typed "not available yet" placeholder before) — a freshly-created,
  // unclaimed request honestly shows zero attempts, not the old placeholder.
  await expect(drawer.getByText('No attempts yet.')).toBeVisible();

  // Confirm the request is real, not just an optimistic client-side fake —
  // it round-trips through the actual API.
  const list = await request.get(`${API}/executions`).then((r) => r.json());
  expect((list.data as Array<{ item_id: string }>).some((r) => r.item_id === itemId)).toBe(true);
});

test('Sprint: the per-item "Run with agent" trigger is present and opens the same shared modal', async ({
  page,
  request,
}) => {
  const projectId = await getOrCreateProject(request);
  const { sprintId, itemId } = await createSprintWithItem(request, projectId, `RWA sprint ${Date.now()}`);

  await page.goto(`/projects/${projectId}/sprint`);
  await waitForApp(page);

  const trigger = page.getByRole('button', { name: /^Run with agent:/ }).first();
  await expect(trigger).toBeVisible();
  await trigger.click();

  const dialog = page.getByRole('dialog', { name: /^Run with agent:/ });
  await expect(dialog).toBeVisible();
  // Opening the per-item trigger must not also open the item-detail drawer
  // (the card's own click handler stops propagation) — the sprint board
  // itself, not a drawer, should still be what's behind the modal.
  await expect(page.getByRole('dialog', { name: 'Item details' })).toBeHidden();

  void sprintId;
  void itemId;
});
