import { test, expect } from '@playwright/test';
import {
  getOrCreateProject,
  createFreshProject,
  createFreshItem,
  createAgentProfile,
  enrollRunner,
  claimOnce,
  setProjectDefaultModel,
  waitForApp,
} from './helpers';

// Cross-surface E2E: a request made in the UI is claimed and the UI shows it
// without a reload, and a model no runner attests is blocked in the form from
// the same live capability data the scheduler enforces — through production
// routes, not mocks. Which runner may claim what (saturation, an exact-runner
// selector excluding every other runner) is the scheduler's own rule, pinned
// in `crates/tack-orch/src/scheduler/select/tests.rs`, not re-proved here.
//
// Every request in this file is created through the real, unmocked
// `RunWithAgentModal` form (`shared/runWithAgent/`) against the real
// production router; runner-side actions (enroll/claim) use direct HTTP —
// the CLI/UI operator surface has no runner-protocol commands, since that
// is `tack-runner`'s job, a different actor than the operator UI/CLI these
// scenarios exercise (see `helpers.ts#enrollRunner`'s own comment).
//
// **Why "exact runner," not "fleet," selects the runner in every scenario
// below:** naming the runner directly is the shortest setup that makes a
// request claimable — a fleet selector would need a fleet created and a
// member added before any of these scenarios could assert anything about
// scheduling. Fleet membership eligibility (including the
// `concurrency_limit`) is proven directly against the
// database in `crates/tack-orch/tests/scheduling/wiring.rs`
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
  // "Where it runs" is one combined picker over active runners/fleets
  // (`RunWithAgentModal.tsx`'s `targetOptions`) that disappears entirely,
  // auto-selecting the sole target, whenever exactly one runner is active
  // and no fleet exists — so it only needs an explicit selection when a
  // real choice exists (which, on a server this whole suite shares, is true
  // as soon as a second runner from any concurrently running test has
  // enrolled).
  const picker = modal.getByRole('combobox', { name: 'Machine or group' });
  if ((await picker.count()) > 0) {
    await picker.selectOption(`exact_runner:${runnerId}`);
  }
  await modal.getByRole('combobox', { name: 'Agent profile' }).selectOption(agentProfileId);
  // The repository fieldset is a read-only summary until "Change for this
  // run" is clicked — the free-text Remote/Base revision fields don't exist
  // in the DOM before that.
  await modal.getByRole('button', { name: 'Change for this run' }).click();
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
  const { runnerId, credential } = await enrollRunner(request, `healthy-runner-${Date.now()}`, modelId);

  const { drawer, modal } = await openRunModal(page, projectId, itemId);
  await fillExactRunnerTarget(modal, runnerId, agentProfileId);

  // A specific, real, matching model choice — the live-capability gate
  // (`RunWithAgentModal.tsx`'s "Choose…" list, built from the target's own
  // `GET /api/runners` capability report) must show it as genuinely
  // supported, not merely "unverified." The target declares exactly one
  // combination (`helpers.ts#enrollRunner`'s fixed capability shape), so it
  // is always index "0".
  await modal.getByLabel('Choose…').check();
  await modal.getByRole('combobox', { name: 'Model' }).selectOption('0');
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
  // refetch trigger from this test. The real attempts endpoint feeds this
  // same tab, so the request's own state badge AND the now-visible attempt
  // row's state badge both read "Leased" — `.first()` targets the
  // request-level one this test's own name is about.
  await expect(drawer.getByText('Leased', { exact: true }).first()).toBeVisible({ timeout: 10_000 });
});

test('an unsupported model is blocked client-side with a named reason, using the same live capability data the scheduler enforces', async ({
  page,
  request,
}) => {
  // A fresh project, never the shared one `getOrCreateProject` returns —
  // this test writes the project's own default model, which the API has no
  // route to clear once set (`helpers.ts#createFreshProject`'s own doc
  // comment). `RunWithAgentModal.tsx`'s "Choose…" list only ever offers a
  // combination the target itself declares (or a free-text override when it
  // attests `model_passthrough`, which this fixture does not), so there is
  // no dropdown entry for an arbitrary undeclared model any more — the
  // project's own default-model tier is the one path left that reaches an
  // explicit, chosen model the target genuinely doesn't support.
  const projectId = await createFreshProject(request, `Scheduler E2E unsupported model project ${Date.now()}`);
  const declaredModelId = `opaque/model-declared-${Date.now()}`;
  const undeclaredModelId = `opaque/model-not-declared-${Date.now()}`;
  const agentProfileId = await createAgentProfile(request, `E2E unsupported profile ${Date.now()}`);
  const { runnerId } = await enrollRunner(request, `unsupported-model-runner-${Date.now()}`, declaredModelId);
  await setProjectDefaultModel(request, projectId, 'openai', undeclaredModelId);

  const itemId = await createFreshItem(request, projectId, `Scheduler E2E unsupported model ${Date.now()}`);
  const { modal } = await openRunModal(page, projectId, itemId);
  await fillExactRunnerTarget(modal, runnerId, agentProfileId);

  // The project's default model is auto-selected ("Project default — …")
  // whenever one is configured, ahead of Auto — no explicit mode switch is
  // needed here, only the target/profile/repository `fillExactRunnerTarget`
  // already filled in.
  await expect(modal.getByLabel(/^Project default —/)).toBeChecked();

  // The live capability gate (real `GET /api/runners` data — the runner
  // enrolled above declared a *different* model) must name this
  // unsupported, not merely leave the control in an ambiguous "unverified"
  // state, and must disable submission entirely.
  await expect(modal.getByText('Unsupported', { exact: true })).toBeVisible();
  await expect(modal.getByRole('button', { name: 'Run' })).toBeDisabled();
});
