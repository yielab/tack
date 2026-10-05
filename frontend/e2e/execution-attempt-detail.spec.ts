import { readFileSync } from 'node:fs';
import { createHash } from 'node:crypto';
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

// The attempts/events/decisions/artifacts UI on the Execution tab
// (`shared/runWithAgent/{AttemptList,EventTimeline,DecisionInbox,
// ArtifactDownloadPanel}.tsx`), proven through the real production router —
// not a mock. Both panels discover decisions/artifacts for real
// (`GET .../attempts/{n}/decisions`, `GET .../attempts/{n}/artifacts`)
// rather than taking a manually typed id — every assertion below finds a
// decision/artifact through its listed row, never by typing an id.

test.describe('Execution tab — real attempts/decisions/artifacts against the production router', () => {
  test('a claimed attempt renders honestly, decisions/artifacts are discovered as empty before anything is raised, and a real raised decision refuses to resolve without a token', async ({
    page,
    request,
  }) => {
    const projectId = await getOrCreateProject(request);
    const itemId = await createFreshItem(request, projectId, `F4 attempt detail ${Date.now()}`);
    const profileId = await createAgentProfile(request, `F4 Profile ${Date.now()}`);
    const modelId = 'opaque/model-alpha';

    // Enroll the target runner BEFORE creating the request — `createExecution`
    // uses an `exact_runner` selector naming it (see that helper's own doc
    // comment for why).
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
    const attemptId = lease!.attemptId;
    const fencingToken = lease!.fencingToken;

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
    // Discovered honestly through the real list routes — nothing raised
    // yet, never a fake empty state conflated with a typed id that simply
    // hasn't been entered.
    await expect(drawer2.getByText('No decisions raised yet')).toBeVisible();
    await expect(drawer2.getByText('No artifacts yet')).toBeVisible();

    await acceptAndStartAttempt(request, runnerId, credential, attemptId, fencingToken);
    const decisionId = `dec-${Date.now()}`;
    await createRunnerDecision(request, runnerId, credential, attemptId, fencingToken, decisionId, [
      { option_id: 'allow_once', label: 'Allow once' },
      { option_id: 'deny', label: 'Deny' },
    ]);

    await page.reload();
    await waitForApp(page);
    const drawer3 = page.getByRole('dialog');
    await drawer3.getByRole('tab', { name: 'Execution' }).click();
    await drawer3.getByRole('button', { name: /Show events, decisions & artifacts/ }).click();

    // The real decision the runner just raised — found through the list,
    // no id typed anywhere.
    await expect(drawer3.getByText('e2e: allow this action?')).toBeVisible();
    await expect(drawer3.getByText('Pending')).toBeVisible();

    // Resolve with NO decision token entered — the real, fail-closed
    // default: "decisions cannot be resolved on this deployment" is a real,
    // expected operator-facing state, not an error. Toasts render via a
    // `<Portal>` to `document.body`, outside the dialog subtree — asserted
    // page-wide, not `drawer3`-scoped.
    await drawer3.getByText('Allow once', { exact: true }).click();
    await drawer3.getByRole('button', { name: 'Resolve' }).click();
    await expect(
      page.getByText(/not configured decision resolution|token entered above is wrong/),
    ).toBeVisible();
  });

  test('a real pending decision resolves through the UI against the production router (token configured), and a real artifact downloads — both discovered, never typed', async ({
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

    // Both discovered through their real list routes — no id typed
    // anywhere, never a manual-entry fallback.
    await expect(drawer.getByText('e2e: allow this action?')).toBeVisible();
    await expect(drawer.getByText(`${artifactId}.txt`)).toBeVisible();

    // Enter the deployment's real decision token (this file's own
    // `playwright.config.ts` addition configures `TACK_EXECUTION_DECISION_TOKEN`
    // for exactly this test).
    await drawer.getByLabel('Your decision token').fill('e2e-decision-token');
    await drawer.getByRole('button', { name: 'Save' }).click();

    // Resolve the REAL decision from its listed row — a genuine POST to the
    // real, mounted resolve route.
    await drawer.getByText('Allow once', { exact: true }).click();
    await drawer.getByRole('button', { name: 'Resolve' }).click();
    // Toast — Portal-rendered outside the dialog subtree, page-wide assert.
    await expect(page.getByText('Decision resolved.')).toBeVisible();
    // The list refetches after a successful resolve — the row's own badge
    // flips from Pending to Resolved without a page reload, proving the
    // resolve genuinely landed server-side (the strongest UI-observable
    // proof) ahead of the idempotent-replay check below.
    await expect(drawer.getByText('Resolved')).toBeVisible();

    // Idempotent replay proves the resolve genuinely landed server-side.
    // Must match the UI's submitted answer byte-for-byte (including the
    // explicit `text: null` `DecisionInbox.tsx#DecisionRow` always sends for
    // an empty "Details" field) — a structurally different answer shape is
    // a genuine `idempotency_conflict`, not a replay.
    const replay = await request.post(`${API}/attempts/${attemptId}/decisions/${decisionId}/resolve`, {
      headers: { 'x-tack-decision-token': 'e2e-decision-token' },
      data: { answer: { option_id: 'allow_once', text: null } },
    });
    expect(replay.ok(), `replay resolve failed: ${replay.status()}`).toBeTruthy();
    const replayBody = await replay.json();
    expect(replayBody.replayed).toBe(true);

    // Download the REAL artifact from its listed row — a genuine browser
    // download event, verified byte-for-byte against what the runner
    // uploaded.
    const [download] = await Promise.all([
      page.waitForEvent('download'),
      drawer.getByRole('button', { name: 'Download' }).click(),
    ]);
    // Chromium appends a MIME-inferred extension to a `download` attribute
    // value that has no extension of its own — a browser download-manager
    // quirk, not a claim this app makes; `.download = artifact.name` is set
    // verbatim in `ArtifactDownloadPanel.tsx`, and `name` already carries
    // `.txt` here, so containment (not exact-match) stays the honest
    // assertion.
    expect(download.suggestedFilename()).toContain(`${artifactId}.txt`);
    const stream = await download.createReadStream();
    const chunks: Buffer[] = [];
    for await (const chunk of stream!) chunks.push(chunk as Buffer);
    expect(Buffer.concat(chunks).toString('utf-8')).toBe(artifactContent);
    await expect(drawer.getByText('Downloaded.')).toBeVisible();
  });

  test('a poll tick during a pending decision never unmounts the attempt panel — a typed-but-unsaved token and a chosen option both survive, and the Resolve control stays the same DOM node', async ({
    page,
    request,
  }) => {
    const projectId = await getOrCreateProject(request);
    const itemId = await createFreshItem(request, projectId, `F4 poll survives ${Date.now()}`);
    const profileId = await createAgentProfile(request, `F4 Profile PS ${Date.now()}`);
    const modelId = 'opaque/model-alpha';

    const { runnerId, credential } = await enrollRunner(request, `F4 Runner PS ${Date.now()}`, modelId);
    const requestId = await createExecution(request, itemId, runnerId, profileId, modelId);
    const lease = await claimOnceWithLease(request, runnerId, credential, `f4-ps-claim-${Date.now()}`);
    expect(lease?.requestId).toBe(requestId);
    const attemptId = lease!.attemptId;
    const fencingToken = lease!.fencingToken;

    await acceptAndStartAttempt(request, runnerId, credential, attemptId, fencingToken);
    const decisionId = `dec-${Date.now()}`;
    await createRunnerDecision(request, runnerId, credential, attemptId, fencingToken, decisionId, [
      { option_id: 'allow_once', label: 'Allow once' },
      { option_id: 'deny', label: 'Deny' },
    ]);

    await page.goto(`/projects/${projectId}/board?item=${itemId}`);
    await waitForApp(page);
    const drawer = page.getByRole('dialog');
    await drawer.getByRole('tab', { name: 'Execution' }).click();
    await expect(drawer.getByText('Attempt #1')).toBeVisible();
    await drawer.getByRole('button', { name: /Show events, decisions & artifacts/ }).click();
    await expect(drawer.getByText('e2e: allow this action?')).toBeVisible();

    // Typed but never saved (no click on "Save") — this is the token
    // FIELD's own local input state, distinct from the persisted
    // preference `decisionTokenStore` already covers; only a live,
    // continuously-mounted `DecisionInbox` keeps it.
    const tokenField = drawer.getByLabel('Your decision token');
    await tokenField.fill('typed-not-saved-token');
    await drawer.getByText('Allow once', { exact: true }).click();

    const resolveButton = drawer.getByRole('button', { name: 'Resolve' });
    const resolveHandle = await resolveButton.elementHandle();
    expect(resolveHandle).not.toBeNull();

    // The next poll tick's refetch of the executions lands, and the frames
    // after it give the panel time to re-render from the new data.
    await page.waitForResponse(
      (r) => r.request().method() === 'GET' && /\/api\/executions(\?|\/|$)/.test(r.url()),
      { timeout: 15_000 },
    );
    await page.evaluate(
      () => new Promise((done) => requestAnimationFrame(() => requestAnimationFrame(() => done(null)))),
    );

    // The exact node captured before the wait is still the one in the DOM
    // — never detached and replaced by a fresh mount.
    expect(await resolveHandle!.evaluate((el) => el.isConnected)).toBe(true);
    expect(await resolveHandle!.isVisible()).toBe(true);
    await expect(tokenField).toHaveValue('typed-not-saved-token');
    await expect(drawer.getByRole('radio', { name: 'Allow once' })).toBeChecked();
  });

  test('a merge-readiness pack shows one row per criterion, and accepting it with a reason shows the review', async ({
    page,
    request,
  }) => {
    const projectId = await getOrCreateProject(request);
    const itemId = await createFreshItem(request, projectId, `E3 pack ${Date.now()}`);
    const profileId = await createAgentProfile(request, `E3 Profile ${Date.now()}`);
    const modelId = 'opaque/model-alpha';

    const { runnerId, credential } = await enrollRunner(request, `E3 Runner ${Date.now()}`, modelId);
    const requestId = await createExecution(request, itemId, runnerId, profileId, modelId);
    const lease = await claimOnceWithLease(request, runnerId, credential, `e3-claim-${Date.now()}`);
    expect(lease?.requestId).toBe(requestId);
    const { attemptId, fencingToken } = lease!;
    await acceptAndStartAttempt(request, runnerId, credential, attemptId, fencingToken);

    // Upload the `ready` fixture as the attempt's pack artifact.
    const raw = readFileSync(new URL('../../docs/contracts/mrp-v1/fixtures/ready.json', import.meta.url));
    const pack = JSON.parse(raw.toString('utf-8'));
    const mediaType = 'application/vnd.tack.mrp+json';
    const artifactId = `mrp-${Date.now()}`;
    const manifest = await request.post(`${API}/runner/v1/attempts/${attemptId}/artifacts`, {
      headers: { authorization: `Bearer ${credential}` },
      data: {
        protocol_version: 1,
        runner_id: runnerId,
        attempt_id: attemptId,
        fencing_token: fencingToken,
        artifacts: [
          {
            artifact_id: artifactId,
            kind: 'mrp',
            name: 'mrp.json',
            media_type: mediaType,
            size_bytes: raw.length,
            sha256: createHash('sha256').update(raw).digest('hex'),
            content_disposition: 'inline_upload',
          },
        ],
      },
    });
    expect(manifest.ok(), `submit_artifacts failed: ${manifest.status()}`).toBeTruthy();
    const upload = await request.put(`${API}/runner/v1/attempts/${attemptId}/artifacts/${artifactId}/content`, {
      headers: {
        authorization: `Bearer ${credential}`,
        'x-tack-fencing-token': String(fencingToken),
        'content-type': mediaType,
      },
      data: raw,
    });
    expect(upload.ok(), `pack upload failed: ${upload.status()}`).toBeTruthy();

    await page.goto(`/projects/${projectId}/board?item=${itemId}`);
    await waitForApp(page);
    const drawer = page.getByRole('dialog');
    await drawer.getByRole('tab', { name: 'Execution' }).click();
    await drawer.getByRole('button', { name: /Show events, decisions & artifacts/ }).click();

    await expect(drawer.getByTestId('mrp-criterion')).toHaveCount(pack.criteria.length);
    await drawer.getByLabel('Reason').fill('Every criterion passed.');
    await drawer.getByRole('button', { name: 'Accept' }).click();
    await expect(drawer.getByTestId('mrp-review')).toContainText('Accepted');
    await expect(drawer.getByTestId('mrp-review')).toContainText('Every criterion passed.');
  });
});
