import { test, expect } from '@playwright/test';
import {
  API,
  acceptAndStartAttempt,
  claimOnceWithLease,
  createAgentProfile,
  createExecution,
  createFreshItem,
  createRunnerDecision,
  getOrCreateProject,
  enrollRunner,
  submitRunnerArtifact,
  waitForApp,
} from './helpers';

// III-F4: the attempts/events/decisions/artifacts UI added to the Execution
// tab (`shared/runWithAgent/{AttemptList,EventTimeline,DecisionInbox,
// ArtifactDownloadPanel}.tsx`), proven through the real production router —
// not a mock.

test.describe('Execution tab — real attempts/decisions/artifacts against the production router', () => {
  test('a claimed attempt renders honestly, and decision/artifact actions each fail with a distinct, visible reason', async ({
    page,
    request,
  }) => {
    const projectId = await getOrCreateProject(request);
    const itemId = await createFreshItem(request, projectId, `F4 attempt detail ${Date.now()}`);
    const profileId = await createAgentProfile(request, `F4 Profile ${Date.now()}`);
    const modelId = 'opaque/model-alpha';

    // Enroll the target runner BEFORE creating the request — `createExecution`
    // uses an `exact_runner` selector naming it (see that helper's own doc
    // comment for why: no `agent_fleet_members` write route exists yet).
    const { runnerId, credential } = await enrollRunner(request, `F4 Runner ${Date.now()}`, modelId);
    const requestId = await createExecution(request, itemId, runnerId, profileId, modelId);

    await page.goto(`/projects/${projectId}/board?item=${itemId}`);
    await waitForApp(page);
    const drawer = page.getByRole('dialog');
    await drawer.getByRole('tab', { name: 'Execution' }).click();
    await expect(drawer.getByText('Queued')).toBeVisible();
    // Nothing has claimed it yet — an honest "no attempts", never a fake
    // empty timeline conflated with "still loading".
    await expect(drawer.getByText('No attempts yet.')).toBeVisible();

    const lease = await claimOnceWithLease(request, runnerId, credential, `f4-claim-${Date.now()}`);
    expect(lease?.requestId).toBe(requestId);

    // Force a fresh mount so `store.ts#loadAttempts` runs against the
    // now-claimed request (realtime refresh is proven separately at the
    // unit level; this test's subject is the real HTTP wiring, not timing).
    await page.reload();
    await waitForApp(page);
    const drawer2 = page.getByRole('dialog');
    await drawer2.getByRole('tab', { name: 'Execution' }).click();
    await expect(drawer2.getByText('Attempt #1')).toBeVisible();
    await expect(drawer2.getByText(runnerId)).toBeVisible();
    // Both the request's own state and the attempt's are "Leased" right
    // after a claim — two badges, hence `.first()`.
    await expect(drawer2.getByText('Leased').first()).toBeVisible();
    // model_provenance is null until the attempt reports actual_execution —
    // "Not yet reported", never a fabricated match.
    await expect(drawer2.getByText('Not yet reported')).toBeVisible();
    // usage_economics is honestly "Not measured" — never $0.00 — before any
    // completion has been reported.
    await expect(drawer2.getByText('Not measured').first()).toBeVisible();

    await drawer2.getByRole('button', { name: /Show events, decisions & artifacts/ }).click();
    await expect(drawer2.getByText('No events reported yet')).toBeVisible();

    // 1) Resolve with NO decision token entered — the real, fail-closed
    // default this card's brief names explicitly ("decisions cannot be
    // resolved on this deployment" is a real, expected operator-facing
    // state). Toasts render via a `<Portal>` to `document.body`, outside
    // the dialog subtree — asserted page-wide, not `drawer2`-scoped.
    await drawer2.getByLabel('Decision id').fill('dec_does_not_exist');
    await drawer2.getByLabel('Answer (option id)').fill('allow_once');
    await drawer2.getByRole('button', { name: 'Resolve decision' }).click();
    await expect(
      page.getByText(/not configured decision resolution|token entered above is wrong/),
    ).toBeVisible();

    // 2) Enter the real decision token (configured server-side by this
    // spec's own `playwright.config.ts` addition) and retry against a
    // decision id that genuinely does not exist — a DIFFERENT, distinct
    // 404 from the real, mounted resolve endpoint.
    await drawer2.getByLabel('Your decision token').fill('e2e-decision-token');
    await drawer2.getByRole('button', { name: 'Save' }).click();
    await drawer2.getByRole('button', { name: 'Resolve decision' }).click();
    await expect(page.getByText('No decision with that id exists for this attempt.')).toBeVisible();

    // 3) Artifact download against an id that genuinely does not exist — a
    // real 404 from the real, mounted download endpoint (no token needed —
    // this route has no separate credential).
    await drawer2.getByLabel('Artifact id').fill('art_does_not_exist');
    await drawer2.getByRole('button', { name: 'Download artifact' }).click();
    await expect(drawer2.getByText('No artifact with that id exists for this attempt.')).toBeVisible();
  });

  test('a real pending decision resolves through the UI against the production router (token configured), and a real artifact downloads', async ({
    page,
    request,
  }) => {
    const projectId = await getOrCreateProject(request);
    const itemId = await createFreshItem(request, projectId, `F4 happy path ${Date.now()}`);
    const profileId = await createAgentProfile(request, `F4 Profile HP ${Date.now()}`);
    const modelId = 'opaque/model-alpha';

    const { runnerId, credential } = await enrollRunner(request, `F4 Runner HP ${Date.now()}`, modelId);
    const requestId = await createExecution(request, itemId, runnerId, profileId, modelId);
    const lease = await claimOnceWithLease(request, runnerId, credential, `f4-hp-claim-${Date.now()}`);
    expect(lease?.requestId).toBe(requestId);
    const attemptId = lease!.attemptId;
    const fencingToken = lease!.fencingToken;

    await acceptAndStartAttempt(request, runnerId, credential, attemptId, fencingToken);
    const decisionId = `dec-${Date.now()}`;
    await createRunnerDecision(request, runnerId, credential, attemptId, fencingToken, decisionId, [
      { option_id: 'allow_once', label: 'Allow once' },
      { option_id: 'deny', label: 'Deny' },
    ]);
    const artifactContent = `hello from e2e ${Date.now()}`;
    const artifactId = `art-${Date.now()}`;
    await submitRunnerArtifact(request, runnerId, credential, attemptId, fencingToken, artifactId, artifactContent);

    await page.goto(`/projects/${projectId}/board?item=${itemId}`);
    await waitForApp(page);
    const drawer = page.getByRole('dialog');
    await drawer.getByRole('tab', { name: 'Execution' }).click();
    await expect(drawer.getByText('Attempt #1')).toBeVisible();
    await drawer.getByRole('button', { name: /Show events, decisions & artifacts/ }).click();

    // Enter the deployment's real decision token (this file's own
    // `playwright.config.ts` addition configures `TACK_EXECUTION_DECISION_TOKEN`
    // for exactly this test) — mirrors `features/approvals/ApprovalsPage.tsx`'s
    // identical token-entry flow.
    await drawer.getByLabel('Your decision token').fill('e2e-decision-token');
    await drawer.getByRole('button', { name: 'Save' }).click();

    // Resolve the REAL decision via the manual quick action (no
    // discovery/list endpoint exists — see `shared/execution/decisions.ts`'s
    // header comment) — a genuine POST to the real, mounted resolve route.
    await drawer.getByLabel('Decision id').fill(decisionId);
    const allowRadio = drawer.getByRole('radio', { name: 'Allow once' });
    // No radios exist for the manual quick action (it's freeform-by-id, see
    // DecisionInbox.tsx) — this decision is unknown to the list either way,
    // so the manual form's own "Answer (option id)" text field is used.
    await expect(allowRadio).toHaveCount(0);
    await drawer.getByLabel('Answer (option id)').fill('allow_once');
    await drawer.getByRole('button', { name: 'Resolve decision' }).click();
    // Toast — Portal-rendered outside the dialog subtree, page-wide assert.
    await expect(page.getByText('Decision resolved.')).toBeVisible();

    // Idempotent replay proves the resolve genuinely landed server-side —
    // the strongest available proof given no read/list endpoint exists.
    // Must match the UI's submitted answer byte-for-byte (including the
    // explicit `text: null` `DecisionInbox.tsx#ManualDecisionResolve` always
    // sends for an empty "Details" field) — a structurally different answer
    // shape is a genuine `idempotency_conflict`, not a replay.
    const replay = await request.post(`${API}/attempts/${attemptId}/decisions/${decisionId}/resolve`, {
      headers: { 'x-tack-decision-token': 'e2e-decision-token' },
      data: { answer: { option_id: 'allow_once', text: null } },
    });
    expect(replay.ok(), `replay resolve failed: ${replay.status()}`).toBeTruthy();
    const replayBody = await replay.json();
    expect(replayBody.replayed).toBe(true);

    // Download the REAL artifact — a genuine browser download event,
    // verified byte-for-byte against what the runner uploaded.
    const [download] = await Promise.all([
      page.waitForEvent('download'),
      (async () => {
        await drawer.getByLabel('Artifact id').fill(artifactId);
        await drawer.getByRole('button', { name: 'Download artifact' }).click();
      })(),
    ]);
    // Chromium appends a MIME-inferred extension (`.txt`, since the fetched
    // Blob's type is `text/plain`) to a `download` attribute value that has
    // no extension of its own — a browser download-manager quirk, not a
    // claim this app makes; `.download = artifactId` is set verbatim in
    // `ArtifactDownloadPanel.tsx`. Assert containment, not byte-exact
    // equality, for exactly that reason.
    expect(download.suggestedFilename()).toContain(artifactId);
    const stream = await download.createReadStream();
    const chunks: Buffer[] = [];
    for await (const chunk of stream!) chunks.push(chunk as Buffer);
    expect(Buffer.concat(chunks).toString('utf-8')).toBe(artifactContent);
    await expect(drawer.getByText('Downloaded.')).toBeVisible();
  });
});
