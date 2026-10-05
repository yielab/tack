import { test, expect, type APIRequestContext } from '@playwright/test';
import path from 'path';
import fs from 'fs';
import { createHash } from 'crypto';
import { fileURLToPath } from 'url';
import { API, acceptAndStartAttempt, claimOnceWithLease, setProjectDefaultModel, waitForApp } from './helpers';

// Screenshot capture for the README and the book. Run with `make screenshots`.
// Outputs PNG files to docs/screenshots/ at repo root.
//
// The agent screens (run dialog, decision inbox, merge-readiness panel,
// factory metrics) need a runner and attempts. No model is called: a runner
// is enrolled and its attempts driven through the real runner protocol, as
// the e2e suite does, so every panel renders from the production router.
// The pack is the mrp-v1 contract's own `ready.json` fixture.

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const OUT_DIR = path.join(__dirname, '../../docs/screenshots');

test.use({
  viewport: { width: 1440, height: 900 },
  colorScheme: 'light',
});

test.setTimeout(90_000);

// Offset from today rather than a fixed date, so the timeline bars stay inside
// the default viewport window no matter when this capture is re-run.
function daysFromNow(days: number): string {
  const d = new Date();
  d.setUTCDate(d.getUTCDate() + days);
  d.setUTCHours(0, 0, 0, 0);
  return d.toISOString();
}

const MRP_PACK = path.join(__dirname, '../../docs/contracts/mrp-v1/fixtures/ready.json');

const BRIEF = {
  acceptance: [
    { kind: 'manual', id: 'login_providers', title: 'Sign-in works with Google and GitHub', text: 'Both providers complete the OAuth2 round trip on staging.' },
    { kind: 'test', id: 'session_tests', title: 'Session tests pass', name: 'cargo test -p auth session', runner: null },
    { kind: 'command', id: 'build', title: 'The release build succeeds', run: 'cargo build --release', expect_exit: 0, cwd: null },
    { kind: 'absent', id: 'no_plain_tokens', title: 'No tokens written to the log file', path: 'logs/tokens.txt' },
  ],
  constraints: [
    { kind: 'forbidden_path', glob: 'migrations/**' },
    { kind: 'max_changed_files', n: 15 },
    { kind: 'note', text: 'Sessions stay server-side; no JWT in local storage.' },
  ],
  definition_of_done: 'A user signs in with either provider, stays signed in across restarts, and signs out everywhere from settings.',
  risk: 'medium',
};

const RUNNER_MODEL = 'claude-sonnet-4-5';

/** The drawer captures share one height, so they sit side by side. */
const DRAWER_HEIGHT = 1300;

/** A runner whose config has a verifier and pushes branches, as
 *  `tack-runner` reports them at enrollment. */
async function enrollConfiguredRunner(request: APIRequestContext) {
  const pending = await (await request.post(`${API}/runners/enrollment`, {
    data: { name: 'build-box', total_capacity: 2, available_capacity: 2 },
  })).json();
  const now = new Date().toISOString();
  const enrolled = await request.post(`${API}/runner/v1/enroll`, {
    data: {
      protocol_version: 1,
      enrollment_token: pending.enrollment_token,
      runner_name: 'build-box',
      runner_version: '0.1.0',
      capabilities: {
        reported_at: now,
        labels: {},
        concurrency: { total: 2, available: 2 },
        harnesses: [{
          harness_kind: 'claude-code',
          installed_version: '2.0.0',
          probe_error: null,
          probed_at: now,
          model_combinations: [{ model_provider: 'anthropic', model_ids: [RUNNER_MODEL], discovery: 'reported' }],
        }],
        features: {},
        limits: { event_payload_bytes_max: 65536, artifact_content_bytes_max: 52428800 },
        verify_configured: true,
        push_configured: true,
      },
    },
  });
  expect(enrolled.ok(), `enroll: ${enrolled.status()}`).toBeTruthy();
  return { runnerId: pending.runner_id as string, credential: (await enrolled.json()).runner_credential as string };
}

/** Requests a run of `itemId` on the runner and drives it to `running`. */
async function startRun(
  request: APIRequestContext,
  runner: { runnerId: string; credential: string },
  itemId: string,
  profileId: string,
) {
  const res = await request.post(`${API}/executions`, {
    data: {
      item_id: itemId,
      idempotency_key: `screenshots-${itemId}`,
      selector_kind: 'exact_runner',
      selector_id: runner.runnerId,
      agent_profile_id: profileId,
      requested_harness_kind: 'claude-code',
      requested_model_provider: 'anthropic',
      requested_model_id: RUNNER_MODEL,
      agent_profile_snapshot: { name: 'Implementer', instructions: 'Implement the item to its brief.', tool_policy: {}, timeout_seconds: 3600, budgets: {} },
      repository_snapshot: { kind: 'git', remote: 'git@github.com:acme/launch.git', base_revision: 'main', subdirectory: null },
      permission_policy: { tools: [], network: false },
      budgets: {},
      environment: {},
      metadata: {},
      timeout_seconds: 3600,
      status_map_policy_id: null,
    },
  });
  expect(res.ok(), `create execution: ${res.status()}`).toBeTruthy();
  const lease = await claimOnceWithLease(request, runner.runnerId, runner.credential, `screenshots-claim-${itemId}`);
  expect(lease, 'the runner claims the request it was named for').not.toBeNull();
  await acceptAndStartAttempt(request, runner.runnerId, runner.credential, lease!.attemptId, lease!.fencingToken);
  return lease!;
}

const seeded = {
  briefItemTitle: 'Auth system — OAuth2 + session',
  briefItemId: '',
  decisionItemId: '',
  mrpItemId: '',
  mrpRequestId: '',
};
const itemIds: Record<string, string> = {};

/** The brief, a pending decision with a recommendation, and a succeeded
 *  attempt carrying a merge-readiness pack. */
async function seedAgentWork(request: APIRequestContext, projectId: string) {
  seeded.briefItemId = itemIds[seeded.briefItemTitle];
  seeded.decisionItemId = itemIds['Dashboard charts & burndown'];
  seeded.mrpItemId = itemIds['REST API documentation'];

  const brief = await request.put(`${API}/items/${seeded.briefItemId}/brief`, { data: BRIEF });
  expect(brief.ok(), `put brief: ${brief.status()}`).toBeTruthy();

  const profile = await request.post(`${API}/agent-profiles`, {
    data: { name: 'Implementer', instructions: 'Implement the item to its brief, then stop.', tool_policy: {} },
  });
  expect(profile.ok(), `create profile: ${profile.status()}`).toBeTruthy();
  const profileId = (await profile.json()).agent_profile_id as string;
  await setProjectDefaultModel(request, projectId, 'anthropic', RUNNER_MODEL);

  const runner = await enrollConfiguredRunner(request);
  const auth = { authorization: `Bearer ${runner.credential}` };

  // A decision the agent raised and is waiting on.
  const asking = await startRun(request, runner, seeded.decisionItemId, profileId);
  const decision = await request.post(`${API}/runner/v1/attempts/${asking.attemptId}/decisions`, {
    headers: auth,
    data: {
      protocol_version: 1,
      runner_id: runner.runnerId,
      attempt_id: asking.attemptId,
      fencing_token: asking.fencingToken,
      decision_id: 'chart-library',
      kind: 'question',
      prompt: 'The burndown needs a chart library. Which one should I add?',
      options: [
        { option_id: 'uplot', label: 'uPlot', description: 'Small canvas charts; covers line and area.', risks: ['No built-in tooltips; about 40 lines to add them.'], estimated_tokens: 18000 },
        { option_id: 'echarts', label: 'Apache ECharts', description: 'Every chart type, themable.', risks: ['Adds about 330 KB gzipped to the bundle.'], estimated_tokens: 9000 },
        { option_id: 'svg', label: 'Hand-written SVG', description: 'No dependency.', risks: ['Axis and resize handling are ours to maintain.'], estimated_tokens: 30000 },
      ],
      recommendation: {
        option_id: 'uplot',
        rationale: 'The dashboard only draws line and area charts, and the bundle budget has 60 KB left.',
        evidence_refs: ['frontend/package.json', 'docs/perf-budget.md'],
      },
    },
  });
  expect(decision.ok(), `create decision: ${decision.status()}`).toBeTruthy();

  // A finished attempt whose verifier produced a pack.
  const done = await startRun(request, runner, seeded.mrpItemId, profileId);
  seeded.mrpRequestId = done.requestId;
  const pack = JSON.parse(fs.readFileSync(MRP_PACK, 'utf-8'));
  pack.attempt_id = done.attemptId;
  const bytes = Buffer.from(JSON.stringify(pack, null, 2));
  const manifest = await request.post(`${API}/runner/v1/attempts/${done.attemptId}/artifacts`, {
    headers: auth,
    data: {
      protocol_version: 1,
      runner_id: runner.runnerId,
      attempt_id: done.attemptId,
      fencing_token: done.fencingToken,
      artifacts: [{
        artifact_id: 'mrp',
        kind: 'mrp',
        name: 'mrp.json',
        media_type: 'application/vnd.tack.mrp+json',
        size_bytes: bytes.length,
        sha256: createHash('sha256').update(bytes).digest('hex'),
        content_disposition: 'inline_upload',
      }],
    },
  });
  expect(manifest.ok(), `pack manifest: ${manifest.status()}`).toBeTruthy();
  const upload = await request.put(`${API}/runner/v1/attempts/${done.attemptId}/artifacts/mrp/content`, {
    headers: { ...auth, 'x-tack-fencing-token': String(done.fencingToken), 'content-type': 'application/vnd.tack.mrp+json' },
    data: bytes,
  });
  expect(upload.ok(), `pack upload: ${upload.status()} ${await upload.text()}`).toBeTruthy();
  const started = new Date(Date.now() - 6 * 60_000).toISOString();
  const completion = await request.post(`${API}/runner/v1/attempts/${done.attemptId}/completion`, {
    headers: auth,
    data: {
      protocol_version: 1,
      runner_id: runner.runnerId,
      attempt_id: done.attemptId,
      fencing_token: done.fencingToken,
      completion_id: 'screenshots-completion',
      terminal_state: 'succeeded',
      terminal_reason: { code: 'completed', message: 'Harness exited successfully' },
      actual_execution: {
        harness_kind: 'claude-code',
        harness_version: '2.0.0',
        model_provider: 'anthropic',
        model_id: RUNNER_MODEL,
        model_observation_source: 'harness_reported',
        capability_snapshot: {
          cancel: { support: 'supported', reason: null },
          resume: { support: 'unsupported', reason: 'no resumable session contract' },
          decisions: { support: 'supported', reason: null },
          artifacts: { support: 'supported', reason: null },
          usage: { support: 'advisory', reason: 'usage may be absent' },
        },
        workspace_id: `ws-${done.attemptId}`,
        base_revision: '0'.repeat(40),
        started_at: started,
        ended_at: new Date().toISOString(),
      },
      usage: {
        tokens_in: { value: 48210, source: 'measured' },
        tokens_out: { value: 6120, source: 'measured' },
        duration_ms: { value: 352000, source: 'measured' },
        cost_usd: { value: null, source: 'not_measured' },
      },
      final_event_checkpoint: null,
    },
  });
  expect(completion.ok(), `completion: ${completion.status()}`).toBeTruthy();
}

// Serial: all tests share the project seeded in beforeAll.
test.describe.serial('README screenshots', () => {
  let projectId: string;

  test.beforeAll(async ({ request }) => {
    fs.mkdirSync(OUT_DIR, { recursive: true });

    const res = await request.post(`${API}/projects`, {
      data: {
        name: 'Product Launch',
        project_type: 'software',
        description: 'Q3 feature sprint — API, dashboard, and launch prep',
      },
    });
    expect(res.ok(), `create project: ${res.status()}`).toBeTruthy();
    const project = await res.json();
    projectId = project.id;

    // Items are always created at the initial status (Backlog).
    // We PATCH each one to its target status after creation.
    // due_date must be a full ISO-8601 datetime.
    const items: Array<{
      title: string;
      priority: string;
      targetStatus: string;
      due_date?: string;
    }> = [
      // Backlog
      { title: 'User research & interviews', priority: 'medium', targetStatus: 'Backlog' },
      { title: 'Analytics event schema', priority: 'low', targetStatus: 'Backlog' },
      { title: 'Onboarding flow wireframes', priority: 'high', targetStatus: 'Backlog' },
      // To Do
      { title: 'Landing page copywriting', priority: 'medium', targetStatus: 'To Do' },
      { title: 'Pricing page design', priority: 'medium', targetStatus: 'To Do' },
      // In Progress — with due dates so timeline bars show up
      { title: 'Auth system — OAuth2 + session', priority: 'high', targetStatus: 'In Progress', due_date: daysFromNow(10) },
      { title: 'Dashboard charts & burndown', priority: 'high', targetStatus: 'In Progress', due_date: daysFromNow(15) },
      { title: 'REST API documentation', priority: 'medium', targetStatus: 'In Progress', due_date: daysFromNow(20) },
      // In Review
      { title: 'Board drag-and-drop polish', priority: 'high', targetStatus: 'In Review' },
      { title: 'Dark mode contrast fixes', priority: 'medium', targetStatus: 'In Review' },
      // Done
      { title: 'Repo & CI setup', priority: 'high', targetStatus: 'Done' },
      { title: 'Tech stack decision', priority: 'medium', targetStatus: 'Done' },
      { title: 'Design system tokens', priority: 'medium', targetStatus: 'Done' },
    ];

    for (const item of items) {
      const { targetStatus, ...createData } = item;
      const r = await request.post(`${API}/projects/${projectId}/items`, {
        data: { ...createData, item_type: 'task' },
      });
      expect(r.ok(), `create "${item.title}": ${r.status()}`).toBeTruthy();

      const created = await r.json();
      itemIds[item.title] = created.id;
      if (targetStatus !== 'Backlog') {
        const patch = await request.patch(`${API}/items/${created.id}`, {
          data: { status: targetStatus },
        });
        expect(patch.ok(), `move "${item.title}" → ${targetStatus}: ${patch.status()}`).toBeTruthy();
      }
    }
    await seedAgentWork(request, projectId);
  });

  test('board', async ({ page }) => {
    await page.goto(`/projects/${projectId}/board`);
    await waitForApp(page);
    await expect(page.getByText('Backlog').first()).toBeVisible();
    await page.waitForTimeout(400);
    await page.screenshot({ path: path.join(OUT_DIR, 'board.png') });
  });

  test('list', async ({ page }) => {
    await page.goto(`/projects/${projectId}/list`);
    await waitForApp(page);
    // List view uses div rows, not a <table>; wait for an item title to confirm render.
    await expect(page.getByText('User research & interviews')).toBeVisible();
    await page.waitForTimeout(300);
    await page.screenshot({ path: path.join(OUT_DIR, 'list.png') });
  });

  test('timeline', async ({ page }) => {
    await page.goto(`/projects/${projectId}/timeline`);
    await waitForApp(page);
    await page.waitForTimeout(800);
    await page.screenshot({ path: path.join(OUT_DIR, 'timeline.png') });
  });

  test('dashboard', async ({ page }) => {
    await page.goto(`/projects/${projectId}/overview`);
    await waitForApp(page);
    await page.waitForTimeout(800);
    await page.screenshot({ path: path.join(OUT_DIR, 'dashboard.png') });
  });

  test('brief', async ({ page }) => {
    await page.setViewportSize({ width: 1440, height: DRAWER_HEIGHT });
    await page.goto(`/projects/${projectId}/board?item=${seeded.briefItemId}`);
    await waitForApp(page);
    const drawer = page.getByRole('dialog');
    await drawer.getByRole('tab', { name: 'Brief' }).click();
    const first = drawer.getByRole('tabpanel').getByRole('textbox', { name: 'Title', exact: true }).first();
    await expect(first).toHaveValue('Sign-in works with Google and GitHub');
    await drawer.getByRole('tablist').scrollIntoViewIfNeeded();
    await page.waitForTimeout(300);
    await drawer.screenshot({ path: path.join(OUT_DIR, 'brief.png') });
  });

  test('run-with-agent', async ({ page }) => {
    // Tall enough that the dialog (max 90vh) shows the whole flow unscrolled.
    await page.setViewportSize({ width: 1440, height: 2000 });
    await page.goto(`/projects/${projectId}/board`);
    await waitForApp(page);
    await page.getByRole('button', { name: `Run with agent: ${seeded.briefItemTitle}` }).click();
    const dialog = page.getByRole('dialog', { name: /^Run with agent:/ });
    await expect(dialog).toBeVisible();
    await expect(dialog.getByText('Verify the result')).toBeVisible();
    await page.waitForTimeout(400);
    await dialog.screenshot({ path: path.join(OUT_DIR, 'run-with-agent.png') });
  });

  test('decision-inbox', async ({ page }) => {
    await page.setViewportSize({ width: 1440, height: DRAWER_HEIGHT });
    await page.goto(`/projects/${projectId}/board?item=${seeded.decisionItemId}`);
    await waitForApp(page);
    const drawer = page.getByRole('dialog');
    await drawer.getByRole('tab', { name: 'Execution' }).click();
    await drawer.getByRole('button', { name: 'Show events, decisions & artifacts' }).first().click();
    await expect(drawer.getByText('Recommended', { exact: true }).first()).toBeVisible({ timeout: 10_000 });
    await drawer.getByRole('heading', { name: 'Decisions' }).evaluate((h) => h.scrollIntoView({ block: 'start' }));
    await page.waitForTimeout(400);
    await drawer.screenshot({ path: path.join(OUT_DIR, 'decision-inbox.png') });
  });

  test('merge-readiness', async ({ page }) => {
    await page.setViewportSize({ width: 1440, height: DRAWER_HEIGHT });
    await page.goto(`/projects/${projectId}/board?item=${seeded.mrpItemId}`);
    await waitForApp(page);
    const drawer = page.getByRole('dialog');
    await drawer.getByRole('tab', { name: 'Execution' }).click();
    await drawer.getByRole('button', { name: 'Show events, decisions & artifacts' }).first().click();
    await expect(drawer.getByText(/^Recommendation:/).first()).toBeVisible({ timeout: 10_000 });
    await drawer.getByText('Merge-readiness pack', { exact: true }).evaluate((h) => h.scrollIntoView({ block: 'start' }));
    await page.waitForTimeout(400);
    await drawer.screenshot({ path: path.join(OUT_DIR, 'merge-readiness.png') });
  });

  test('factory-metrics', async ({ page, request }) => {
    // The pack was opened in the previous capture; record its verdict.
    const review = await request.post(`${API}/executions/${seeded.mrpRequestId}/attempts/1/mrp/review`, {
      data: { verdict: 'accept', reason: 'Every criterion passed; the coverage report matches the diff.' },
    });
    expect(review.ok(), `review: ${review.status()}`).toBeTruthy();
    await page.goto(`/projects/${projectId}/factory`);
    await waitForApp(page);
    await expect(page.getByRole('heading', { name: /factory metrics/i })).toBeVisible();
    await page.waitForTimeout(600);
    await page.screenshot({ path: path.join(OUT_DIR, 'factory-metrics.png') });
  });

  test('settings-vocabulary', async ({ page }) => {
    await page.goto(`/projects/${projectId}/settings`);
    await waitForApp(page);
    // Click the Vocabulary tab in the settings panel
    const vocabTab = page
      .getByRole('tab', { name: /vocabulary/i })
      .or(page.getByRole('button', { name: /vocabulary/i }))
      .or(page.getByText('Vocabulary').first());
    await expect(vocabTab).toBeVisible({ timeout: 5_000 });
    await vocabTab.click();
    await page.waitForTimeout(300);
    await page.screenshot({ path: path.join(OUT_DIR, 'settings-vocabulary.png') });
  });
});
