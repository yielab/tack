import { test, expect, type Page, type Locator } from '@playwright/test';
import path from 'path';
import fs from 'fs';
import os from 'os';
import { execFileSync, spawn, type ChildProcess } from 'child_process';
import { fileURLToPath } from 'url';
import { waitForApp } from './helpers';

// The whole workflow, through the UI, with docket as the agent and prill as the
// verifier: a new project pointed at a folder, the runner enrolled on the Agents
// page, the project's agent settings, three items of three kinds (a bug, a
// feature, a task) each run with docket on a branch of the folder, every
// finished attempt verified by prill into a merge-readiness pack the review
// panel renders, the pack and the attempt accepted, the branch merged, and one
// run that asks before each action so the decision inbox is exercised too.
// Every screen is captured along the way.
//
// Run through `scripts/docket-prill-e2e.sh`, which builds tack from this
// checkout, starts it on a fresh database, starts `docket start --dispatch` (API 5) in a scratch
// HOME and DOCKET_HOME (a local OpenAI-compatible endpoint as docket's provider and default
// model), writes the runner configuration (prill as `[verify]`) and passes
// everything this spec needs in `DOCKET_PRILL_*` variables. Every run here is a
// real model call on docket's default model; nothing is staged, and no run names
// a model.
//
// What is asserted, beyond "it finished": the pack came from prill (the
// artifact of kind `mrp` exists, no `attempt.verify_failed` event), the pack's
// criteria carry the brief's ids with prill's verdicts, its decision is one
// prill can reach (`review` here, because a local image cannot be digest-pinned
// and a manual criterion always needs a person), and the agent's change really
// lands on a branch of the fixture repository.

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const BASE = process.env.E2E_API_ORIGIN || 'http://127.0.0.1:3377';
const API = `${BASE}/api`;
const SHOTS = process.env.DOCKET_PRILL_SHOTS || path.join(__dirname, '../test-results/docket-prill');
const WORK = process.env.DOCKET_PRILL_WORK || fs.mkdtempSync(path.join(os.tmpdir(), 'docket-prill-'));
const TACK_BIN = process.env.DOCKET_PRILL_TACK_BIN || path.join(__dirname, '../../target/debug/tack');
const RUNNER_CONFIG = process.env.DOCKET_PRILL_RUNNER_CONFIG || '';
const RUNNER_STATE = process.env.DOCKET_PRILL_RUNNER_STATE || path.join(WORK, 'runner-state');
const RUNNER_PATH = process.env.DOCKET_PRILL_RUNNER_PATH || process.env.PATH || '';
const AGENT_HOME = process.env.DOCKET_PRILL_HOME || '';
// The server's API token: the browser is signed in by the page's own session
// cookie, and this spec's direct calls carry the token.
const API_TOKEN = process.env.DOCKET_PRILL_API_TOKEN || '';
const DECISION_TOKEN = process.env.DOCKET_PRILL_DECISION_TOKEN || 'docket-prill-decision';
const NO_KEYCHAIN = process.env.DOCKET_PRILL_NO_KEYCHAIN || 'unix:path=/nonexistent/tack-docket-prill-no-keychain';

const PROJECT_NAME = 'Calculator';
const RUNNER_NAME = 'workstation-docket-prill';

/** The three kinds of work, each with what prill can check by itself. */
const ITEMS = {
  bug: {
    type: 'Bug',
    title: 'divide() returns the product instead of the quotient',
    description:
      'tests/test_calc.py::test_divide fails because divide(a, b) in calc.py returns a * b. ' +
      'Fix divide so it returns a / b and raises ZeroDivisionError on b == 0 (the test expects that). ' +
      'Use your file tools to edit calc.py; run the tests with: python3 -m pytest -q. Change nothing else.',
    checklist: ['divide(10, 2) returns 5', 'The whole suite passes'],
    criteria: [
      { kind: 'command', id: 'suite', title: 'The test suite passes', run: 'python3 -m pytest -q', expect_exit: 0, cwd: null },
      { kind: 'absent', id: 'no_debug_log', title: 'No debug log was left behind', path: 'debug.log' },
    ],
  },
  feature: {
    type: 'Feature',
    title: 'Add multiply(a, b) to the calculator',
    description:
      'Add a multiply(a, b) function to calc.py that returns a * b, and a test named test_multiply in tests/test_calc.py ' +
      'asserting multiply(3, 4) == 12. Use your file tools to edit the files; run the tests with: python3 -m pytest -q.',
    checklist: ['multiply(3, 4) returns 12', 'A test covers multiply'],
    criteria: [
      { kind: 'command', id: 'suite', title: 'The test suite passes', run: 'python3 -m pytest -q', expect_exit: 0, cwd: null },
      { kind: 'command', id: 'multiply_tested', title: 'A test named test_multiply exists', run: 'grep -q "def test_multiply" tests/test_calc.py', expect_exit: 0, cwd: null },
    ],
  },
  task: {
    type: 'Task',
    title: 'Write the changelog',
    description:
      'Create CHANGELOG.md at the repository root with a top heading "# Changelog" and a section "## Unreleased" that lists, ' +
      'as bullet points, the functions calc.py exports (read calc.py to find them). Use your file tools to write the file; do not run shell commands.',
    checklist: ['CHANGELOG.md has an Unreleased section'],
    criteria: [
      { kind: 'file', id: 'changelog', title: 'CHANGELOG.md exists', path: 'CHANGELOG.md' },
      { kind: 'command', id: 'unreleased', title: 'It has an Unreleased section', run: 'grep -q "^## Unreleased" CHANGELOG.md', expect_exit: 0, cwd: null },
      { kind: 'command', id: 'suite', title: 'The test suite still passes', run: 'python3 -m pytest -q', expect_exit: 0, cwd: null },
    ],
  },
  ask: {
    type: 'Task',
    title: 'Record completion with a shell command',
    description:
      "Using the bash tool, run exactly this command and nothing else: sh -c 'echo done > DONE.txt' " +
      'Then stop. Do not create the file any other way.',
    checklist: ['DONE.txt contains the word done'],
    criteria: [{ kind: 'file', id: 'done_marker', title: 'DONE.txt exists', path: 'DONE.txt' }],
  },
} as const;

type ItemKey = keyof typeof ITEMS;

test.use({ viewport: { width: 1440, height: 900 }, colorScheme: 'light' });

// ── Capture ──────────────────────────────────────────────────────────────────
let shotIndex = 0;
const summary: Record<string, unknown> = { base: BASE, work: WORK, shots: [] as string[], runs: {} as Record<string, unknown>, allowed: {} as Record<string, unknown> };

async function shot(target: Page | Locator, name: string): Promise<void> {
  fs.mkdirSync(SHOTS, { recursive: true });
  shotIndex += 1;
  const file = path.join(SHOTS, `${String(shotIndex).padStart(2, '0')}-${name}.png`);
  await target.screenshot({ path: file });
  (summary.shots as string[]).push(path.basename(file));
}

// The app's main content area scrolls inside `Layout.tsx`'s overflow container,
// so a plain screenshot shows one viewport's worth: grow the viewport to the
// tallest scrolled container first (agent-assets.spec.ts does the same).
async function shotFullContent(page: Page, name: string): Promise<void> {
  const contentHeight = await page.evaluate(() => {
    let max = 0;
    for (const el of Array.from(document.querySelectorAll('*'))) {
      const style = getComputedStyle(el);
      if ((style.overflowY === 'auto' || style.overflowY === 'scroll') && el.scrollHeight > el.clientHeight) {
        max = Math.max(max, el.scrollHeight);
      }
    }
    return max;
  });
  const viewport = page.viewportSize();
  if (viewport && contentHeight > viewport.height) {
    await page.setViewportSize({ width: viewport.width, height: Math.min(contentHeight + 40, 4000) });
    await page.waitForTimeout(300);
  }
  await shot(page, name);
  if (viewport) await page.setViewportSize(viewport);
}

// ── API ──────────────────────────────────────────────────────────────────────
// The runner's poll loop writes to the same SQLite file often enough that an
// operator write can lose the single-writer race (`500 database is locked`,
// marked retryable by the API itself); retried like a real client would.
function withToken(init?: RequestInit): RequestInit {
  const headers = new Headers(init?.headers);
  if (API_TOKEN) headers.set('Authorization', `Bearer ${API_TOKEN}`);
  return { ...init, headers };
}
async function apiFetch(p: string, init?: RequestInit): Promise<unknown> {
  let lastErr: unknown;
  for (let attempt = 0; attempt < 6; attempt++) {
    const res = await fetch(`${API}${p}`, withToken(init));
    if (res.ok) return res.status === 204 ? null : res.json();
    const text = await res.text();
    if (res.status === 500 && /database is locked/i.test(text) && attempt < 5) {
      lastErr = new Error(`${init?.method ?? 'GET'} ${p} -> ${res.status}: ${text}`);
      await new Promise((r) => setTimeout(r, 300 * (attempt + 1)));
      continue;
    }
    throw new Error(`${init?.method ?? 'GET'} ${p} -> ${res.status}: ${text}`);
  }
  throw lastErr;
}
const json = (method: string, body: unknown): RequestInit => ({
  method,
  headers: { 'content-type': 'application/json' },
  body: JSON.stringify(body),
});

/**
 * Waits for a run to end, answering what docket asks along the way as the operator would: a
 * gated command is allowed once, a consultation is pointed back at the item. A local model sometimes writes a gated command (a `2>/dev/null` redirect, a
 * binary off docket's list) on its own; that is docket's policy working, not this run's subject.
 * Each answer is recorded in the summary.
 */
async function waitAllowingApprovals(requestId: string, label: string, timeoutMs: number): Promise<string> {
  const start = Date.now();
  const allowed: string[] = [];
  let last = '';
  while (Date.now() - start < timeoutMs) {
    last = ((await apiFetch(`/executions/${requestId}`)) as { state?: string }).state ?? last;
    if (['succeeded', 'failed', 'cancelled'].includes(last)) break;
    if (last === 'waiting_decision') {
      const attempts = await attemptsOf(requestId);
      const n = attempts[attempts.length - 1]?.attempt_number;
      const { data } = (await apiFetch(`/executions/${requestId}/attempts/${n}/decisions`)) as {
        data: Array<{
          decision_id: string; attempt_id: string; kind: string; prompt: string; state: string;
          options: Array<{ option_id: string }>; recommendation?: { option_id?: string } | null;
        }>;
      };
      for (const d of data.filter((d) => d.state === 'pending')) {
        // A gated command is allowed once. A consultation (a member asking how to go on) takes
        // docket's recommended option, else its first that is not a decline; one with no options
        // of its own is answered by pointing back at the item, which already says what to do.
        const own = d.options.filter((o) => !/decline/i.test(o.option_id) && o.option_id !== 'accept');
        const answer =
          d.kind === 'approval'
            ? { option_id: (d.options.find((o) => /once|allow/i.test(o.option_id)) ?? d.options[0]).option_id }
            : own.length
              ? { option_id: d.recommendation?.option_id ?? own[0].option_id }
              : { option_id: 'accept', text: 'Do exactly what the item says, with the tools you have, and finish.' };
        await apiFetch(
          `/attempts/${d.attempt_id}/decisions/${d.decision_id}/resolve`,
          { ...json('POST', { answer }), headers: { 'content-type': 'application/json', 'x-tack-decision-token': DECISION_TOKEN } },
        );
        allowed.push(`${d.kind}: ${d.prompt}`);
      }
    }
    await new Promise((r) => setTimeout(r, 1000));
  }
  if (allowed.length) (summary.allowed as Record<string, unknown>)[label] = allowed;
  if (!['succeeded', 'failed', 'cancelled'].includes(last)) {
    throw new Error(`timed out after ${timeoutMs}ms waiting for request ${requestId} to end (last seen: ${last || 'none'})`);
  }
  return last;
}

async function waitForRequestState(requestId: string, wantStates: readonly string[], timeoutMs: number): Promise<string> {
  const start = Date.now();
  let last = '';
  while (Date.now() - start < timeoutMs) {
    const body = (await apiFetch(`/executions/${requestId}`)) as { state?: string };
    last = body?.state ?? last;
    if (last && wantStates.includes(last)) return last;
    await new Promise((r) => setTimeout(r, 1000));
  }
  throw new Error(`timed out after ${timeoutMs}ms waiting for request ${requestId} to reach one of [${wantStates.join(', ')}] (last seen: ${last || 'none'})`);
}

interface RunnerRow {
  runner_id: string;
  name: string;
  state: string;
  capability_snapshot?: {
    harnesses?: Array<{
      harness_kind: string;
      default_model?: string | null;
      models?: { ids?: string[] } | null;
      sign_in?: { state: string; detail: string } | null;
    }>;
  };
}
/** A model docket's team members run on, and the two teams the script's daemon is asked for, as our runner reports them. */
async function docketDefaultModel(): Promise<string> {
  const mine = (await runners()).find((r) => r.runner_id === runnerId);
  const docket = mine?.capability_snapshot?.harnesses?.find((h) => h.harness_kind === 'docket');
  expect(docket?.sign_in?.state, `docket's sign-in: ${docket?.sign_in?.detail}`).toBe('signed_in');
  expect(docket?.models?.ids?.length, 'docket reports the models its team members run on').toBeGreaterThan(0);
  return docket!.models!.ids![0];
}
async function runners(): Promise<RunnerRow[]> {
  return ((await apiFetch('/runners')) as { data: RunnerRow[] }).data;
}

// ── Fixture repository ───────────────────────────────────────────────────────
function git(cwd: string, ...args: string[]): string {
  return execFileSync('git', ['-c', 'user.email=fixture@invalid', '-c', 'user.name=fixture', ...args], { cwd, encoding: 'utf8' }).trim();
}

/** A small Python package with one seeded bug and a prill configuration that
 *  names the suite, so prill's suite and anti-tautology stages have something
 *  to run inside its sandbox image. */
function makeFixtureRepo(dir: string): string {
  fs.mkdirSync(path.join(dir, 'tests'), { recursive: true });
  fs.writeFileSync(path.join(dir, 'README.md'), '# calc\n\nA tiny calculator used by Tack\'s docket + prill end-to-end test.\n');
  fs.writeFileSync(
    path.join(dir, 'calc.py'),
    '"""A tiny calculator."""\n\n\ndef add(a, b):\n    return a + b\n\n\ndef subtract(a, b):\n    return a - b\n\n\ndef divide(a, b):\n    return a * b\n',
  );
  fs.writeFileSync(
    path.join(dir, 'tests', 'test_calc.py'),
    'import pytest\n\nfrom calc import add, divide, subtract\n\n\ndef test_add():\n    assert add(2, 3) == 5\n\n\ndef test_subtract():\n    assert subtract(5, 3) == 2\n\n\ndef test_divide():\n    assert divide(10, 2) == 5\n    with pytest.raises(ZeroDivisionError):\n        divide(1, 0)\n',
  );
  fs.writeFileSync(
    path.join(dir, 'prill.yaml'),
    'version: 1\ntests:\n  command: "python3 -m pytest -q --junitxml={report}"\n  suites:\n    - name: python\n      globs: ["tests/**/test_*.py"]\n      file_command: "python3 -m pytest -q {file} --junitxml={report}"\n',
  );
  fs.writeFileSync(path.join(dir, '.gitignore'), '__pycache__/\n.pytest_cache/\n');
  git(dir, 'init', '-q', '-b', 'main');
  git(dir, 'add', '-A');
  git(dir, 'commit', '-q', '-m', 'seed: calculator with a failing divide test');
  return git(dir, 'rev-parse', 'HEAD');
}

// ── Shared state ─────────────────────────────────────────────────────────────
let repoDir: string;
let projectId: string;
let runnerId: string;
let runnerProcess: ChildProcess | null = null;
let modelId: string;
const itemIds: Partial<Record<ItemKey, string>> = {};

async function ensureProject(): Promise<void> {
  if (projectId) return;
  const projects = (await apiFetch('/projects')) as Array<{ id: string; name: string }>;
  projectId = projects.find((p) => p.name === PROJECT_NAME)?.id ?? '';
  if (!projectId) throw new Error(`project "${PROJECT_NAME}" not found — run the earlier tests first`);
}
async function ensureItem(key: ItemKey): Promise<string> {
  await ensureProject();
  if (itemIds[key]) return itemIds[key]!;
  const items = (await apiFetch(`/projects/${projectId}/items`)) as { data: Array<{ id: string; title: string }> };
  const id = items.data.find((it) => it.title === ITEMS[key].title)?.id;
  if (!id) throw new Error(`item "${ITEMS[key].title}" not found — run the earlier tests first`);
  itemIds[key] = id;
  return id;
}
async function ensureRunner(): Promise<void> {
  if (runnerId) return;
  const mine = (await runners()).find((r) => r.name === RUNNER_NAME && r.state === 'active');
  if (!mine) throw new Error(`runner "${RUNNER_NAME}" is not active — run the earlier tests first`);
  runnerId = mine.runner_id;
}

test.beforeAll(() => {
  fs.mkdirSync(SHOTS, { recursive: true });
  repoDir = path.join(WORK, 'calc');
  if (fs.existsSync(repoDir)) fs.rmSync(repoDir, { recursive: true, force: true });
  fs.mkdirSync(repoDir, { recursive: true });
  summary.fixtureRepo = repoDir;
  summary.seedCommit = makeFixtureRepo(repoDir);
  const versions = path.join(WORK, 'versions.txt');
  if (fs.existsSync(versions)) summary.versions = fs.readFileSync(versions, 'utf8').trim().split('\n');
});

test.afterAll(() => {
  if (runnerProcess && runnerProcess.exitCode === null) runnerProcess.kill('SIGTERM');
  fs.writeFileSync(path.join(SHOTS, 'summary.json'), JSON.stringify(summary, null, 2) + '\n');
});

// ── Dialog helpers ───────────────────────────────────────────────────────────
/** Two targets exist (the embedded runner and ours), so the picker shows;
 *  select ours by name, never by a typed id. */
async function selectRunner(modal: Locator): Promise<void> {
  const picker = modal.getByRole('combobox', { name: 'Machine or group' });
  const appeared = await picker.waitFor({ state: 'visible', timeout: 5000 }).then(() => true).catch(() => false);
  if (!appeared) return;
  await expect(picker.getByRole('option', { name: RUNNER_NAME })).toBeAttached({ timeout: 10_000 });
  await picker.selectOption({ label: RUNNER_NAME });
}

interface AttemptRow {
  attempt_number: number;
  state: string;
  terminal_reason?: unknown;
}
async function attemptsOf(requestId: string): Promise<AttemptRow[]> {
  return ((await apiFetch(`/executions/${requestId}/attempts`)) as { data: AttemptRow[] }).data;
}
interface ArtifactRow { artifact_id: string; kind: string; name: string }
async function artifactsOf(requestId: string, n: number): Promise<ArtifactRow[]> {
  return ((await apiFetch(`/executions/${requestId}/attempts/${n}/artifacts`)) as { data: ArtifactRow[] }).data;
}
async function artifactJson(requestId: string, n: number, artifactId: string): Promise<unknown> {
  const res = await fetch(`${API}/executions/${requestId}/attempts/${n}/artifacts/${artifactId}/content`, withToken());
  if (!res.ok) throw new Error(`artifact ${artifactId} -> ${res.status}`);
  return res.json();
}
async function eventsOf(requestId: string, n: number): Promise<Array<{ kind: string; sequence: number; payload?: unknown }>> {
  return ((await apiFetch(`/executions/${requestId}/attempts/${n}/events`)) as { data: Array<{ kind: string; sequence: number; payload?: unknown }> }).data;
}

interface Pack {
  recommendation: { decision: string; rationale: string };
  risk: { tier: string; reasons: string[] };
  criteria: Array<{ id: string; kind: string; status: string; note: string | null }>;
  verify: { command: string; exit_code: number } | null;
  produced_by: { program: string; version: string };
}

/** Writes the machine-checkable criteria behind the checklist the UI typed,
 *  keeping those manual lines: the brief is one document on the server. */
async function addCriteria(itemId: string, key: ItemKey): Promise<void> {
  const current = (await apiFetch(`/items/${itemId}/brief`).catch(() => null)) as
    | { acceptance: unknown[]; constraints: unknown[]; definition_of_done: string | null; risk: string | null }
    | null;
  await apiFetch(`/items/${itemId}/brief`, json('PUT', {
    acceptance: [...(current?.acceptance ?? []), ...ITEMS[key].criteria],
    constraints: [...(current?.constraints ?? []), { kind: 'forbidden_path', glob: 'prill.yaml' }],
    definition_of_done: current?.definition_of_done ?? null,
    risk: current?.risk ?? null,
  }));
}

/** Opens the run dialog for an item from the board, fills it the way a person
 *  would (our runner, docket, docket's own default model, verify on) and
 *  submits. Returns the request id. */
async function runFromBoard(page: Page, key: ItemKey, options: { ask?: boolean; capture?: boolean } = {}): Promise<string> {
  const itemId = await ensureItem(key);
  await page.setViewportSize({ width: 1440, height: 1800 });
  await page.goto(`${BASE}/projects/${projectId}/board`);
  await waitForApp(page);
  await page.getByRole('button', { name: `Run with agent: ${ITEMS[key].title}` }).click();
  const modal = page.getByRole('dialog', { name: /^Run with agent:/ });
  await expect(modal).toBeVisible();
  await selectRunner(modal);
  await expect(modal.getByRole('button', { name: 'Implementer', pressed: true })).toBeVisible();
  await modal.getByText('Advanced for this run').click();
  await modal.getByLabel('Agent', { exact: true }).selectOption('docket');
  // docket names no default model (each team member runs on its own), so the run leaves it to
  // docket; the model its members run on is listed to choose.
  const model = modal.getByLabel('Model', { exact: true });
  await expect(model.locator('option:checked')).toHaveText("Agent's default");
  await expect(model.locator('option', { hasText: modelId })).toHaveCount(1);
  // prill is this runner's verifier: the switch is live and on, not deferred.
  const verify = modal.getByLabel('Verify the result');
  await expect(verify).toBeEnabled();
  await expect(verify).toBeChecked();
  await expect(modal.getByLabel('Push the branch')).toBeDisabled();
  if (options.ask) {
    // The help hint beside it carries the same label; the control is the switch.
    const ask = modal.getByRole('switch', { name: 'Ask before each action' });
    await expect(ask).toBeEnabled();
    await ask.check();
  }
  const run = modal.getByRole('button', { name: 'Run', exact: true });
  await expect(run).toBeEnabled();
  if (options.capture) await shot(modal, `run-dialog-${key}`);
  await run.click();
  await expect(modal).toBeHidden();
  await page.setViewportSize({ width: 1440, height: 900 });
  const listed = (await apiFetch(`/executions?item_id=${itemId}`)) as { data: Array<{ request_id: string }> };
  const requestId = listed.data[0]?.request_id;
  if (!requestId) throw new Error('no execution request found for this item after clicking Run');
  return requestId;
}

/** The finished attempt on the Execution tab, the pack prill wrote, and the
 *  checks that it is prill's. Returns the pack and the branch the change is on. */
async function inspectFinishedRun(page: Page, key: ItemKey, requestId: string, captureDrawer: boolean): Promise<{ pack: Pack; branch: string | null; attempt: number }> {
  const itemId = await ensureItem(key);
  const attempts = await attemptsOf(requestId);
  const attempt = attempts.find((a) => a.state === 'succeeded') ?? attempts[attempts.length - 1];
  expect(attempt, 'the request has an attempt').toBeTruthy();
  const n = attempt.attempt_number;

  const artifacts = await artifactsOf(requestId, n);
  const kinds = artifacts.map((a) => a.kind);
  const events = await eventsOf(requestId, n);
  const verifyFailed = events.find((e) => e.kind === 'attempt.verify_failed');
  expect(verifyFailed, `prill did not fail: ${JSON.stringify(verifyFailed?.payload ?? null)}`).toBeUndefined();
  // The steps docket reported while it ran come first, in order, and the run's end after them.
  const kindsInOrder = events.map((e) => e.kind);
  // docket's team reports each step (hop) it runs, not each model call or tool inside one.
  expect(kindsInOrder, 'a step of the team ran').toContain('agent.hop_started');
  expect(kindsInOrder, 'a step of the team finished').toContain('agent.hop_finished');
  expect(kindsInOrder.indexOf('agent.hop_finished')).toBeLessThan(kindsInOrder.indexOf('attempt.terminal'));
  const sequences = events.map((e) => e.sequence);
  expect(sequences, 'one event per sequence, oldest first').toEqual([...sequences].sort((a, b) => a - b));
  expect(new Set(sequences).size).toBe(sequences.length);
  expect(kinds, 'evidence, files and the pack were uploaded').toEqual(expect.arrayContaining(['evidence', 'files', 'patch', 'mrp']));

  const mrp = artifacts.find((a) => a.kind === 'mrp')!;
  const pack = (await artifactJson(requestId, n, mrp.artifact_id)) as Pack;
  expect(pack.produced_by.program).toBe('prill');
  expect(['merge', 'review', 'reject']).toContain(pack.recommendation.decision);
  expect(pack.recommendation.rationale).toMatch(/^prill: /);
  for (const criterion of ITEMS[key].criteria) {
    const found = pack.criteria.find((c) => c.id === criterion.id);
    expect(found, `criterion ${criterion.id} is in the pack`).toBeTruthy();
    expect(found!.kind).toBe(criterion.kind);
  }
  const evidenceArtifact = artifacts.find((a) => a.kind === 'evidence')!;
  const evidence = (await artifactJson(requestId, n, evidenceArtifact.artifact_id)) as { branch?: unknown; files?: Array<{ path: string }> };
  const b = evidence.branch;
  const branch = typeof b === 'string' ? b : ((b as { branch?: unknown } | null)?.branch as string | undefined) ?? null;

  await page.goto(`${BASE}/projects/${projectId}/board?item=${itemId}&tab=execution`);
  await waitForApp(page);
  await expect(page.getByText(/^Finished/).first()).toBeVisible({ timeout: 20_000 });
  await page.getByRole('button', { name: 'Show timeline, questions & files' }).first().click();
  await expect(page.getByRole('heading', { name: 'Merge-readiness pack' })).toBeVisible();
  await expect(page.getByText(`Recommendation: ${pack.recommendation.decision}`)).toBeVisible();
  await expect(page.getByTestId('mrp-criterion')).toHaveCount(pack.criteria.length);
  if (captureDrawer) {
    await page.setViewportSize({ width: 1440, height: 2400 });
    await page.waitForTimeout(400);
    await shot(page.getByRole('dialog', { name: 'Item details' }), `execution-${key}-pack`);
    await page.setViewportSize({ width: 1440, height: 900 });
  }
  (summary.runs as Record<string, unknown>)[key] = {
    ...((summary.runs as Record<string, Record<string, unknown>>)[key] ?? {}),
    requestId,
    attempt: n,
    decision: pack.recommendation.decision,
    risk: pack.risk.tier,
    criteria: pack.criteria.map((c) => `${c.id}: ${c.status}`),
    verify: pack.verify,
    branch,
    files: evidence.files?.map((f) => f.path),
    prill: pack.produced_by.version,
    steps: events.filter((e) => e.kind.startsWith('agent.')).length,
  };
  return { pack, branch, attempt: n };
}

/** Review the pack and the attempt as a person would, then merge the branch
 *  into main in the fixture repository — Tack recommends, it never merges. */
async function acceptAndMerge(page: Page, key: ItemKey, branch: string | null, captureBoard: boolean): Promise<void> {
  const itemId = await ensureItem(key);
  if (captureBoard) {
    await page.goto(`${BASE}/projects/${projectId}/board`);
    await waitForApp(page);
    await expect(page.getByText('Needs your review').first()).toBeVisible();
    await shot(page, `board-needs-review-${key}`);
  }
  await page.goto(`${BASE}/projects/${projectId}/board?item=${itemId}&tab=execution`);
  await waitForApp(page);
  await page.getByRole('button', { name: 'Show timeline, questions & files' }).first().click();
  const panel = page.getByRole('heading', { name: 'Merge-readiness pack' }).locator('xpath=..');
  await panel.getByLabel('Reason').fill('Checked the pack: the suite ran in the sandbox and every automatic criterion passed.');
  await panel.getByRole('button', { name: 'Accept', exact: true }).click();
  await expect(page.getByText('Pack accepted.')).toBeVisible();
  const drawer = page.getByRole('dialog', { name: 'Item details' });
  await drawer.getByRole('button', { name: 'Accept', exact: true }).first().click();
  await expect(page.getByText(/^Accepted/).first()).toBeVisible();
  if (branch) {
    // Fast-forward only: the branch was cut from main's tip and main has not moved.
    git(repoDir, 'merge', '--ff-only', branch);
  }
}

// ── The workflow ─────────────────────────────────────────────────────────────
/** Opens one of Settings → Automation's folded sections by its heading, as a person clicks it. */
async function openFold(page: Page, title: string) {
  const fold = page.locator('details').filter({ has: page.getByRole('heading', { name: title, exact: true }) });
  if (!(await fold.evaluate((details) => (details as HTMLDetailsElement).open))) {
    await fold.locator(':scope > summary').click();
  }
}

test.describe.serial('docket runs, prill verifies', () => {
  test('01 a new project pointed at the folder', async ({ page }) => {
    await page.goto(`${BASE}/`);
    await waitForApp(page);
    await expect(page.getByRole('button', { name: 'New Project' }).first()).toBeVisible();
    await shot(page, 'first-open');
    await page.getByRole('button', { name: 'New Project' }).first().click();
    await page.getByPlaceholder('My Awesome Project').fill(PROJECT_NAME);
    await page.getByPlaceholder('A brief description of your project...').fill('A tiny Python calculator: docket makes the changes, prill verifies them.');
    await page.getByLabel('An existing folder on this computer').check();
    await page.getByLabel('Code folder').fill(repoDir);
    await page.getByLabel('Code folder').blur();
    await expect(page.getByText(/^A git repository/)).toBeVisible();
    await shot(page, 'new-project');
    await page.getByRole('button', { name: 'Create Project' }).click();
    await expect(page.getByText('Backlog').first()).toBeVisible();
    await ensureProject();
    // The modal creates the project, then writes the folder onto it in a
    // second request, so the folder can land a moment after the board shows.
    await expect
      .poll(async () => {
        const project = (await apiFetch(`/projects/${projectId}`)) as { repository: string | null; code_origin: string };
        return `${project.code_origin}:${project.repository ?? ''}`;
      }, { timeout: 10_000 })
      .toBe(`folder:${repoDir}`);
    await shot(page, 'board-empty');
  });

  test('02 agent execution on, and our runner enrolled from the Agents page', async ({ page }) => {
    test.skip(!RUNNER_CONFIG, 'DOCKET_PRILL_RUNNER_CONFIG is unset — run this through scripts/docket-prill-e2e.sh');
    await page.goto(`${BASE}/agents`);
    await waitForApp(page);
    await expect(page.getByText('Agent execution on this machine')).toBeVisible();
    const toggle = page.getByRole('button', { name: /Turn (on|off)/ });
    if ((await toggle.textContent())?.includes('Turn on')) {
      await toggle.click();
      await expect(page.getByText('Running', { exact: true }).first()).toBeVisible({ timeout: 30_000 });
    }
    await page.waitForTimeout(3000);
    await shotFullContent(page, 'agents-on');

    // The runner with prill and the model endpoint is enrolled here, by hand,
    // the way a second machine would be — and started with the token shown once.
    await page.getByRole('button', { name: /^Advanced/ }).click();
    await page.getByRole('tab', { name: 'Runners', exact: true }).click();
    await page.getByLabel('Name').fill(RUNNER_NAME);
    await page.getByLabel('Labels (JSON object, optional)').fill(JSON.stringify({ verifier: 'prill', harness: 'docket' }));
    await page.getByRole('button', { name: 'Enroll' }).click();
    const id = (await page.locator('xpath=//p[normalize-space(text())="Runner ID"]/following::p[1]').first().textContent())?.trim();
    const token = (await page.locator('xpath=//p[normalize-space(text())="Enrollment token"]/following::p[1]').first().textContent())?.trim();
    if (!id || !token) throw new Error('could not read the runner id and enrollment token from the modal');
    runnerId = id;
    await shot(page.getByRole('dialog'), 'runner-enrolled');

    const log = fs.openSync(path.join(WORK, 'runner.log'), 'a');
    runnerProcess = spawn(
      TACK_BIN,
      ['runner', 'start', '--config', RUNNER_CONFIG, '--api-url', BASE, '--runner-id', runnerId, '--state-dir', RUNNER_STATE],
      {
        env: {
          PATH: RUNNER_PATH,
          HOME: AGENT_HOME || os.homedir(),
          TACK_RUNNER_ENROLLMENT_TOKEN: token,
          DBUS_SESSION_BUS_ADDRESS: NO_KEYCHAIN,
        },
        stdio: ['ignore', log, log],
      },
    );
    fs.writeFileSync(path.join(WORK, 'runner.pid'), String(runnerProcess.pid));
    await page.getByRole('button', { name: "I've copied it — close" }).click();

    await expect.poll(async () => (await runners()).find((r) => r.runner_id === runnerId)?.state, { timeout: 60_000 }).toBe('active');
    const mine = (await runners()).find((r) => r.runner_id === runnerId)!;
    const docket = mine.capability_snapshot?.harnesses?.find((h) => h.harness_kind === 'docket');
    expect(docket, 'the runner reports docket').toBeTruthy();
    modelId = await docketDefaultModel();
    summary.runner = { runnerId, name: RUNNER_NAME, modelId };
    await page.waitForTimeout(4500);
    await shot(page.getByText('Runners enrolled or revoked this session', { exact: true }).locator('xpath=..'), 'runners-this-session');
  });

  test('03 the project\'s agent settings: a branch of the folder, docket, the model', async ({ page }) => {
    await ensureProject();
    await ensureRunner();
    if (!modelId) modelId = await docketDefaultModel();
    await page.goto(`${BASE}/projects/${projectId}/settings?tab=automation`);
    await waitForApp(page);
    await expect(page.getByRole('heading', { name: 'Where the code is' })).toBeVisible();
    await expect(page.getByLabel('An existing folder on this computer')).toBeChecked();
    await openFold(page, 'How the agent works on it');
    await page.getByLabel('On a new branch of your repo (recommended)').check();
    await openFold(page, 'Agent');
    await page.getByRole('button', { name: /^docket$/i }).click();
    await expect.poll(async () => ((await apiFetch(`/projects/${projectId}`)) as { default_harness: string | null }).default_harness, { timeout: 10_000 }).toBe('docket');
    await page.reload();
    await waitForApp(page);
    await openFold(page, 'Agent');
    await expect(page.getByRole('button', { name: /^docket$/i })).toHaveAttribute('aria-pressed', 'true');
    // docket listed the models its team members run on; the project names none.
    const model = page.getByLabel('Model', { exact: true });
    await expect(model.locator('option', { hasText: modelId }).last()).toHaveText(modelId);
    await expect(page.getByText(/From docket on this computer\./i)).toBeVisible();
    const project = (await apiFetch(`/projects/${projectId}`)) as { workspace_mode: string; default_harness: string | null; default_model: string | null };
    expect(project.workspace_mode).toBe('local_branch');
    expect(project.default_harness).toBe('docket');
    expect(project.default_model).toBeNull();
    await shotFullContent(page, 'automation-settings');
  });

  test('04 three kinds of work on the board, each with a checklist', async ({ page }) => {
    await ensureProject();
    await page.goto(`${BASE}/projects/${projectId}/board`);
    await waitForApp(page);
    await expect(page.getByText('Backlog').first()).toBeVisible();
    for (const key of ['bug', 'feature', 'task', 'ask'] as const) {
      const item = ITEMS[key];
      await page.getByTitle('Add item').first().click();
      const form = page.getByRole('dialog');
      await form.getByPlaceholder('What needs to be done?').fill(item.title);
      // The chip reads `<emoji>Label`, emoji and label with nothing between.
      await form.getByRole('button', { name: new RegExp(`^\\P{L}*${item.type}$`, 'u') }).click();
      await form.locator('[contenteditable="true"]').first().click();
      await page.keyboard.type(item.description);
      if (key === 'bug') await shot(form, 'new-item-bug');
      await form.getByRole('button', { name: 'Create Item' }).click();
      await expect(page.getByPlaceholder('What needs to be done?')).toBeHidden();
      await expect(page.getByText(item.title, { exact: true }).first()).toBeVisible();
    }
    await page.waitForTimeout(4500);
    await shot(page, 'board-items');

    for (const key of ['bug', 'feature', 'task', 'ask'] as const) {
      const itemId = await ensureItem(key);
      await page.goto(`${BASE}/projects/${projectId}/board?item=${itemId}`);
      await waitForApp(page);
      const drawer = page.getByRole('dialog', { name: 'Item details' });
      for (const [i, text] of ITEMS[key].checklist.entries()) {
        await drawer.getByRole('button', { name: 'Add check' }).click();
        const line = drawer.getByLabel('Acceptance criterion').nth(i);
        await line.fill(text);
        await line.blur();
        await page.waitForTimeout(400);
      }
      await expect.poll(
        async () => ((await apiFetch(`/items/${itemId}/brief`).catch(() => ({ acceptance: [] }))) as { acceptance: unknown[] }).acceptance.length,
        { timeout: 10_000 },
      ).toBe(ITEMS[key].checklist.length);
      await addCriteria(itemId, key);
      if (key === 'bug') {
        await page.reload();
        await waitForApp(page);
        await page.setViewportSize({ width: 1440, height: 1400 });
        await drawer.getByRole('heading', { name: 'Acceptance criteria' }).evaluate((h) => h.scrollIntoView({ block: 'center' }));
        await page.waitForTimeout(300);
        await shot(drawer, 'item-checklist-bug');
        await page.setViewportSize({ width: 1440, height: 900 });
      }
    }
  });

  test('05 the bug: docket fixes it on a branch, prill verifies, the board asks for review', async ({ page }) => {
    await ensureRunner();
    const requestId = await runFromBoard(page, 'bug', { capture: true });
    await page.goto(`${BASE}/projects/${projectId}/board`);
    await waitForApp(page);
    await expect(page.getByRole('button', { name: `Open the Execution tab for ${ITEMS.bug.title}` })).toBeVisible();
    await page.waitForTimeout(800);
    await shot(page, 'board-tracking-bug');
    await page.goto(`${BASE}/projects/${projectId}/board?item=${itemIds.bug}&tab=execution`);
    await waitForApp(page);
    await expect(page.getByText('Running', { exact: true }).or(page.getByText('Finished', { exact: true })).first()).toBeVisible({ timeout: 120_000 });
    await shot(page, 'execution-running-bug');

    const state = await waitAllowingApprovals(requestId, `run-5`, 600_000);
    expect(state, 'the bug-fix run succeeded').toBe('succeeded');
    const { pack, branch } = await inspectFinishedRun(page, 'bug', requestId, true);
    expect(pack.criteria.find((c) => c.id === 'suite')?.status, 'prill ran the suite in its sandbox and it passed').toBe('passed');
    expect(pack.verify?.exit_code, 'the suite stage exited 0').toBe(0);
    expect(branch, 'the change is on a branch of the folder').toMatch(/^tack\//);
    expect(git(repoDir, 'branch', '--list', branch!)).toContain(branch!);
    await acceptAndMerge(page, 'bug', branch, true);
    expect(git(repoDir, 'show', 'HEAD:calc.py')).not.toContain('return a * b');
  });

  test('06 the feature: a new function with its test', async ({ page }) => {
    await ensureRunner();
    const requestId = await runFromBoard(page, 'feature');
    const state = await waitAllowingApprovals(requestId, `run-6`, 600_000);
    expect(state, 'the feature run succeeded').toBe('succeeded');
    const { pack, branch } = await inspectFinishedRun(page, 'feature', requestId, true);
    expect(pack.criteria.find((c) => c.id === 'multiply_tested')?.status).toBe('passed');
    expect(pack.criteria.find((c) => c.id === 'suite')?.status).toBe('passed');
    await acceptAndMerge(page, 'feature', branch, false);
    expect(git(repoDir, 'show', 'HEAD:calc.py')).toContain('def multiply');
  });

  test('07 the task: a document, written with a profile made for it', async ({ page }) => {
    await ensureRunner();
    await page.goto(`${BASE}/agents`);
    await waitForApp(page);
    await page.getByRole('button', { name: /^Advanced/ }).click();
    await page.getByRole('tab', { name: 'Profiles', exact: true }).click();
    await page.getByRole('button', { name: '+ Create agent profile' }).click();
    const form = page.locator('form').filter({ has: page.getByLabel('Instructions') });
    await form.getByLabel('Name').fill('Changelog writer');
    await form.getByLabel('Instructions').fill(
      'You write project documentation. Read the code first, then write exactly the file the item asks for, in Markdown, and stop. Do not change code or tests.',
    );
    await page.setViewportSize({ width: 1440, height: 2200 });
    await form.scrollIntoViewIfNeeded();
    await shot(form, 'agent-profile');
    await page.setViewportSize({ width: 1440, height: 900 });
    await form.getByRole('button', { name: 'Create' }).click();
    await expect(page.getByText('Changelog writer').first()).toBeVisible();

    const itemId = await ensureItem('task');
    await page.setViewportSize({ width: 1440, height: 1800 });
    await page.goto(`${BASE}/projects/${projectId}/board`);
    await waitForApp(page);
    await page.getByRole('button', { name: `Run with agent: ${ITEMS.task.title}` }).click();
    const modal = page.getByRole('dialog', { name: /^Run with agent:/ });
    await selectRunner(modal);
    await modal.getByRole('group', { name: 'Profile' }).getByRole('button', { name: 'Changelog writer', exact: true }).click();
    await modal.getByText('Advanced for this run').click();
    await modal.getByLabel('Agent', { exact: true }).selectOption('docket');
    await expect(modal.getByRole('button', { name: 'Run', exact: true })).toBeEnabled();
    await shot(modal, 'run-dialog-task');
    await modal.getByRole('button', { name: 'Run', exact: true }).click();
    await expect(modal).toBeHidden();
    await page.setViewportSize({ width: 1440, height: 900 });
    const listed = (await apiFetch(`/executions?item_id=${itemId}`)) as { data: Array<{ request_id: string }> };
    const requestId = listed.data[0]!.request_id;
    const state = await waitAllowingApprovals(requestId, `run-7`, 600_000);
    expect(state, 'the documentation run succeeded').toBe('succeeded');
    const { pack, branch } = await inspectFinishedRun(page, 'task', requestId, true);
    expect(pack.criteria.find((c) => c.id === 'changelog')?.status).toBe('passed');
    expect(pack.criteria.find((c) => c.id === 'unreleased')?.status).toBe('passed');
    await acceptAndMerge(page, 'task', branch, false);
    expect(git(repoDir, 'show', 'HEAD:CHANGELOG.md')).toContain('## Unreleased');
  });

  test('08 ask before each action: docket pauses on a gated command and the inbox answers it', async ({ page }, testInfo) => {
    await ensureRunner();
    const itemId = await ensureItem('ask');
    const requestId = await runFromBoard(page, 'ask', { ask: true, capture: true });
    // `sh` is off docket's list of safe binaries, so a model that follows the
    // instruction makes docket ask (a bare `echo … > file` inside the workspace
    // runs unattended). A model that writes the file some other way never asks;
    // that is the model's choice, recorded here rather than failed, and the
    // run's end state is still checked.
    const start = Date.now();
    let state = '';
    let asked = false;
    while (Date.now() - start < 600_000) {
      state = ((await apiFetch(`/executions/${requestId}`)) as { state: string }).state;
      if (['succeeded', 'failed', 'cancelled'].includes(state)) break;
      const attempts = await attemptsOf(requestId);
      const n = attempts[attempts.length - 1]?.attempt_number;
      if (n) {
        const decisions = ((await apiFetch(`/executions/${requestId}/attempts/${n}/decisions`)) as { data: Array<{ state: string }> }).data;
        if (decisions.some((d) => d.state === 'pending')) { asked = true; break; }
      }
      await new Promise((r) => setTimeout(r, 1000));
    }
    if (!asked) {
      testInfo.annotations.push({ type: 'note', description: `the model never asked; the run ended ${state}` });
      (summary.runs as Record<string, unknown>).ask = { requestId, asked: false, state };
      await page.goto(`${BASE}/projects/${projectId}/board?item=${itemId}&tab=execution`);
      await waitForApp(page);
      await shot(page, 'execution-ask-no-question');
      return;
    }
    await expect
      .poll(async () => ((await apiFetch(`/executions/${requestId}`)) as { state: string }).state, { timeout: 20_000 })
      .toBe('waiting_decision');
    await page.goto(`${BASE}/projects/${projectId}/board?item=${itemId}&tab=execution`);
    await waitForApp(page);
    await page.getByRole('button', { name: 'Show timeline, questions & files' }).first().click();
    const tokenField = page.getByLabel('Your decision token');
    await tokenField.fill(DECISION_TOKEN);
    await tokenField.locator('xpath=ancestor::div[.//button][1]').getByRole('button', { name: 'Save', exact: true }).click();
    await expect(page.getByText('Decision token saved for this browser session.')).toBeVisible();
    // docket asks in one of two ways, and the inbox answers each as a person would: a gated
    // command (approval) names the tool and docket's policy and is allowed; a consultation from a
    // member that cannot do the step itself (the lead has no shell) takes its first option.
    const kinds: string[] = [];
    for (let answered = 0; answered < 3; answered++) {
      const attempts = await attemptsOf(requestId);
      const n = attempts[attempts.length - 1]?.attempt_number;
      const decisions = ((await apiFetch(`/executions/${requestId}/attempts/${n}/decisions`)) as {
        data: Array<{ state: string; kind?: string; prompt?: string }>;
      }).data;
      const pending = decisions.find((d) => d.state === 'pending');
      if (!pending) break;
      kinds.push(pending.kind ?? 'unknown');
      await expect(page.getByText('Pending', { exact: true }).first()).toBeVisible({ timeout: 20_000 });
      if (pending.kind === 'approval') {
        const question = page.getByText(/^Allow bash\b/).first();
        await expect(question).toBeVisible();
        await expect(question).toContainText("docket's policy:");
      } else {
        await expect(page.getByText(pending.prompt!.slice(0, 40), { exact: false }).first()).toBeVisible();
      }
      await page.setViewportSize({ width: 1440, height: 2000 });
      await page.waitForTimeout(300);
      await shot(page.getByRole('dialog', { name: 'Item details' }), `decision-pending-${answered + 1}`);
      // The option's input is visually hidden behind a styled span (`RadioRow`), so a
      // plain click lands on the span; set the input directly.
      await page.getByRole('radio').first().check({ force: true });
      await page.getByRole('button', { name: 'Resolve', exact: true }).first().click();
      await expect(page.getByText('Decision resolved.')).toBeVisible();
      await page.setViewportSize({ width: 1440, height: 900 });
      // Either the run goes on to its end or docket asks again.
      const next = Date.now();
      while (Date.now() - next < 300_000) {
        const st = ((await apiFetch(`/executions/${requestId}`)) as { state: string }).state;
        if (st !== 'waiting_decision' && st !== 'running' && st !== 'claimed' && st !== 'queued') break;
        const again = ((await apiFetch(`/executions/${requestId}/attempts/${n}/decisions`)) as { data: Array<{ state: string }> }).data;
        if (st === 'waiting_decision' && again.some((d) => d.state === 'pending')) break;
        await new Promise((r) => setTimeout(r, 1000));
      }
      await page.reload();
      await waitForApp(page);
      await page.getByRole('button', { name: 'Show timeline, questions & files' }).first().click();
    }
    testInfo.annotations.push({ type: 'note', description: `docket asked: ${kinds.join(', ')}` });
    const end = await waitForRequestState(requestId, ['succeeded', 'failed', 'cancelled'], 600_000);
    (summary.runs as Record<string, unknown>).ask = { requestId, asked: true, state: end };
    expect(end, 'the run went on after the answer').toBe('succeeded');
    const { pack } = await inspectFinishedRun(page, 'ask', requestId, true);
    expect(pack.criteria.find((c) => c.id === 'done_marker')?.status).toBe('passed');
  });

  test('09 where it ends: the board, the list and the fixture repository', async ({ page }) => {
    await ensureProject();
    await page.goto(`${BASE}/projects/${projectId}/board`);
    await waitForApp(page);
    await expect(page.getByText(ITEMS.bug.title, { exact: true }).first()).toBeVisible();
    await shot(page, 'board-final');
    await page.goto(`${BASE}/projects/${projectId}/list`);
    await waitForApp(page);
    await expect(page.getByText(ITEMS.feature.title, { exact: true }).first()).toBeVisible();
    await shot(page, 'list-final');
    const log = git(repoDir, 'log', '--oneline', 'main');
    summary.fixtureLog = log.split('\n');
    expect(log.split('\n').length, 'main carries the seed plus one merged attempt per accepted run').toBeGreaterThanOrEqual(4);
    expect(git(repoDir, 'status', '--porcelain'), 'the user\'s checkout was never touched').toBe('');
    // The runner snapshots with its own git plumbing and keeps no ref, so the
    // user's repository holds the attempt branches and nothing else.
    expect(git(repoDir, 'for-each-ref', 'refs/prill')).toBe('');
    expect(git(repoDir, 'branch', '--list', 'tack/*').split('\n').filter(Boolean).length).toBeGreaterThanOrEqual(4);
  });
});
