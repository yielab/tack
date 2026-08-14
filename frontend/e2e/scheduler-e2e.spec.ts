import { test, expect } from '@playwright/test';
import {
  getOrCreateProject,
  createFreshItem,
  createAgentProfile,
  createModelProfile,
  enrollRunner,
  claimOnce,
  waitForApp,
} from './helpers';

// Cross-surface E2E for the wave's acceptance bar (TODO.md III-E6): "healthy
// fleet selection, saturation, exact runner, unsupported model, and realtime
// updates all pass through production routes — not mocks — in ... the UI."
//
// Every request in this file is created through the real, unmocked
// `RunWithAgentModal` form (`shared/runWithAgent/`) against the real
// production router; runner-side actions (enroll/claim) use direct HTTP —
// the CLI/UI operator surface has no runner-protocol commands, since that
// is `tack-runner`'s job, a different actor than the operator UI/CLI this
// wave's acceptance bar is about (see `helpers.ts#enrollRunner`'s own
// comment).
//
// **Why "exact runner," not "fleet," selects the runner in every scenario
// below:** `agent_fleet_members` (the fleet-membership join table) has no
// write route through any API surface — a pre-existing, already-documented
// gap (III-E3's own handoff: "agent_fleet_members exists in the schema with
// no route") that this integration card deliberately left as a read-only
// gap rather than widen scope with a write route nobody asked for. Without
// a way to place a runner into a fleet via HTTP, a `fleet`-selector request
// would have zero eligible members forever, through no API surface. Fleet
// membership eligibility (including the previously-unenforced
// `concurrency_limit`) is proven directly against the database in
// `crates/tack-orch/tests/scheduler_wiring_test.rs`
// (`an_unsaturated_fleet_still_allows_a_member_to_claim`/
// `a_saturated_fleet_concurrency_limit_blocks_a_fleet_selector_request`);
// exact-runner selection exercises the identical downstream scheduler
// eligibility code (`tack_orch::scheduler::wiring::choose_request_for_runner`)
// minus only the selector-kind branch itself.

const BASE_REVISION = 'e2e-scheduler-base';

/** Opens the shared "Run with agent" modal from the item-detail drawer,
 *  fills the common fields, and returns the modal locator plus the
 *  `Run` button — every scenario below customizes target/model from here. */
async function openRunModal(page: import('@playwright/test').Page, projectId: string, itemId: string) {
  await page.goto(`/projects/${projectId}/board?item=${itemId}`);
  await waitForApp(page);
  const drawer = page.getByRole('dialog', { name: 'Item details' });
  await expect(drawer).toBeVisible();
  await drawer.getByRole('button', { name: 'Run with agent' }).click();
  const modal = page.getByRole('dialog', { name: /^Run with agent:/ });
  await expect(modal).toBeVisible();
  return { drawer, modal };
}

async function fillExactRunnerTarget(
  modal: import('@playwright/test').Locator,
  runnerId: string,
  agentProfileId: string,
) {
  await modal.getByLabel('Exact runner').check();
  await modal.getByLabel('Runner id').fill(runnerId);
  await modal.getByRole('combobox', { name: 'Agent profile' }).selectOption(agentProfileId);
  await modal.getByLabel('Remote').fill('git@example.com:org/e2e-scheduler.git');
  await modal.getByLabel('Base revision').fill(BASE_REVISION);
}

test('healthy exact-runner selection is claimed, and the UI reflects it without a reload (realtime)', async ({
  page,
  request,
}) => {
  const projectId = await getOrCreateProject(request);
  const itemId = await createFreshItem(request, projectId, `Scheduler E2E healthy ${Date.now()}`);
  const agentProfileId = await createAgentProfile(request, `E2E healthy profile ${Date.now()}`);
  const modelId = `opaque/model-healthy-${Date.now()}`;
  const modelProfileId = await createModelProfile(request, `E2E healthy model ${Date.now()}`, 'openai', modelId);
  const { runnerId, credential } = await enrollRunner(request, `healthy-runner-${Date.now()}`, modelId);

  const { drawer, modal } = await openRunModal(page, projectId, itemId);
  await fillExactRunnerTarget(modal, runnerId, agentProfileId);

  // A specific, real, matching model choice — the live-capability gate
  // (card III-E6: `RunWithAgentModal.tsx` now fetches `GET /api/runners`)
  // must show it as genuinely supported, not merely "unverified."
  await modal.getByLabel('Choose a model').check();
  await modal.getByRole('combobox', { name: 'Model' }).selectOption(modelProfileId);
  await expect(modal.getByText('Supported', { exact: true })).toBeVisible();

  const runButton = modal.getByRole('button', { name: 'Run' });
  await expect(runButton).toBeEnabled();
  await runButton.click();
  await expect(modal).toBeHidden();

  const executionTab = drawer.getByRole('tab', { name: 'Execution' });
  await executionTab.click();
  await expect(drawer.getByText('Queued')).toBeVisible();

  // The runner claims it over the real runner-v1 wire — a background HTTP
  // call, not a UI action; the page is never reloaded or re-navigated
  // afterward.
  const claimedId = await claimOnce(request, runnerId, credential, `healthy-claim-${Date.now()}`);
  expect(claimedId, 'the scheduler must have picked this exact runner for its own request').not.toBeNull();

  // The store's bounded poll (default 4s, `shared/execution/realtime.ts`)
  // must pick up the state change on its own — no reload, no manual
  // refetch trigger from this test. Card III-F4 wired the real attempts
  // endpoint into this same tab, so the request's own state badge AND the
  // now-visible attempt row's state badge both read "Leased" — `.first()`
  // targets the request-level one this test's own name is about.
  await expect(drawer.getByText('Leased', { exact: true }).first()).toBeVisible({ timeout: 10_000 });
});

test('a saturated runner leaves a second exact-runner request visibly queued', async ({ page, request }) => {
  const projectId = await getOrCreateProject(request);
  const modelId = `opaque/model-saturated-${Date.now()}`;
  const agentProfileId = await createAgentProfile(request, `E2E saturation profile ${Date.now()}`);
  const modelProfileId = await createModelProfile(request, `E2E saturation model ${Date.now()}`, 'openai', modelId);
  const { runnerId, credential } = await enrollRunner(request, `saturated-runner-${Date.now()}`, modelId, 1);

  const firstItemId = await createFreshItem(request, projectId, `Scheduler E2E saturation 1 ${Date.now()}`);
  {
    const { modal } = await openRunModal(page, projectId, firstItemId);
    await fillExactRunnerTarget(modal, runnerId, agentProfileId);
    await modal.getByLabel('Choose a model').check();
    await modal.getByRole('combobox', { name: 'Model' }).selectOption(modelProfileId);
    await modal.getByRole('button', { name: 'Run' }).click();
    await expect(modal).toBeHidden();
  }
  const firstClaim = await claimOnce(request, runnerId, credential, `saturation-claim-1-${Date.now()}`);
  expect(firstClaim).not.toBeNull();

  const secondItemId = await createFreshItem(request, projectId, `Scheduler E2E saturation 2 ${Date.now()}`);
  const { drawer, modal } = await openRunModal(page, projectId, secondItemId);
  await fillExactRunnerTarget(modal, runnerId, agentProfileId);
  await modal.getByLabel('Choose a model').check();
  await modal.getByRole('combobox', { name: 'Model' }).selectOption(modelProfileId);
  await modal.getByRole('button', { name: 'Run' }).click();
  await expect(modal).toBeHidden();

  // The runner's one slot is already in use — a second claim attempt must
  // find nothing, and the UI must keep showing the honest, unclaimed state.
  const secondClaim = await claimOnce(request, runnerId, credential, `saturation-claim-2-${Date.now()}`);
  expect(secondClaim, "a saturated runner's slot must not be double-leased").toBeNull();

  const executionTab = drawer.getByRole('tab', { name: 'Execution' });
  await executionTab.click();
  await expect(drawer.getByText('Queued')).toBeVisible();
  await page.waitForTimeout(4500); // one full poll cycle
  await expect(drawer.getByText('Queued')).toBeVisible();
});

test('an exact-runner request is never claimed by a different, otherwise-eligible runner', async ({
  page,
  request,
}) => {
  const projectId = await getOrCreateProject(request);
  const modelId = `opaque/model-exact-${Date.now()}`;
  const agentProfileId = await createAgentProfile(request, `E2E exact profile ${Date.now()}`);
  const modelProfileId = await createModelProfile(request, `E2E exact model ${Date.now()}`, 'openai', modelId);
  const target = await enrollRunner(request, `exact-target-${Date.now()}`, modelId);
  const bystander = await enrollRunner(request, `exact-bystander-${Date.now()}`, modelId);

  const itemId = await createFreshItem(request, projectId, `Scheduler E2E exact runner ${Date.now()}`);
  const { drawer, modal } = await openRunModal(page, projectId, itemId);
  await fillExactRunnerTarget(modal, target.runnerId, agentProfileId);
  await modal.getByLabel('Choose a model').check();
  await modal.getByRole('combobox', { name: 'Model' }).selectOption(modelProfileId);
  await modal.getByRole('button', { name: 'Run' }).click();
  await expect(modal).toBeHidden();

  const bystanderClaim = await claimOnce(
    request,
    bystander.runnerId,
    bystander.credential,
    `exact-bystander-claim-${Date.now()}`,
  );
  expect(
    bystanderClaim,
    'an exact-runner selector must exclude every runner except the one it names, even an identically-capable one',
  ).toBeNull();

  const executionTab = drawer.getByRole('tab', { name: 'Execution' });
  await executionTab.click();
  await expect(drawer.getByText('Queued')).toBeVisible();

  // The *named* runner still can claim it — proves the exclusion above was
  // about identity, not a broken request.
  const targetClaim = await claimOnce(request, target.runnerId, target.credential, `exact-target-claim-${Date.now()}`);
  expect(targetClaim).not.toBeNull();
});

test('an unsupported model is blocked client-side with a named reason, using the same live capability data the scheduler enforces', async ({
  page,
  request,
}) => {
  const projectId = await getOrCreateProject(request);
  const declaredModelId = `opaque/model-declared-${Date.now()}`;
  const undeclaredModelId = `opaque/model-not-declared-${Date.now()}`;
  const agentProfileId = await createAgentProfile(request, `E2E unsupported profile ${Date.now()}`);
  const undeclaredModelProfileId = await createModelProfile(
    request,
    `E2E unsupported model ${Date.now()}`,
    'openai',
    undeclaredModelId,
  );
  const { runnerId } = await enrollRunner(request, `unsupported-model-runner-${Date.now()}`, declaredModelId);

  const itemId = await createFreshItem(request, projectId, `Scheduler E2E unsupported model ${Date.now()}`);
  const { modal } = await openRunModal(page, projectId, itemId);
  await fillExactRunnerTarget(modal, runnerId, agentProfileId);

  await modal.getByLabel('Choose a model').check();
  await modal.getByRole('combobox', { name: 'Model' }).selectOption(undeclaredModelProfileId);

  // The live capability gate (real `GET /api/runners` data — the runner
  // enrolled above declared a *different* model) must name this
  // unsupported, not merely leave the control in an ambiguous "unverified"
  // state, and must disable submission entirely.
  await expect(modal.getByText('Unsupported', { exact: true })).toBeVisible();
  await expect(modal.getByRole('button', { name: 'Run' })).toBeDisabled();
});
