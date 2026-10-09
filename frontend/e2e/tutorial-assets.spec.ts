import { test, expect, type Page } from '@playwright/test';
import path from 'path';
import fs from 'fs';
import os from 'os';
import { execSync } from 'child_process';
import { fileURLToPath } from 'url';
import { waitForApp } from './helpers';

// Captures the step-by-step tutorial screenshots (docs/screenshots/tutorial/)
// that docs/book/src/user-guide/tutorial.md embeds, every step done through
// the UI: first open, project creation, adding tasks, a task's acceptance
// criteria, turning agent execution on (the agents it finds), the project's
// agent settings, an agent profile, the "Run with agent" dialog, and one REAL
// Claude Code execution tracked from the board chip through the Execution tab
// to its files. Then records docs/screenshots/workflow.gif: the whole loop on
// one page, from a new project to an accepted run.
//
// Run against an ALREADY-RUNNING release build of `tack serve` (built with
// `--features embed-spa`), started with a FRESH database so the first
// screenshot really is a first open, with a real, signed-in `claude` on PATH
// — see playwright.tutorial-assets.config.ts for the exact recipe.
// The two executions this file creates — the tutorial's and workflow.gif's —
// are real, live, billed model calls.

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const OUT_DIR = path.join(__dirname, '../../docs/screenshots/tutorial');

const BASE = process.env.E2E_API_ORIGIN || 'http://127.0.0.1:3311';
const API = `${BASE}/api`;

const PROJECT_NAME = 'Website Relaunch';
const RUN_ITEM_TITLE = 'Draft the launch announcement';
const OTHER_ITEMS = [
  'Redesign the pricing page',
  'Migrate DNS to the new host',
  'Refresh the screenshots in the docs',
];
const PROFILE_NAME = 'Announcement writer';
const PROFILE_INSTRUCTIONS =
  'Write the launch announcement for the relaunched marketing site as a blog post of about 400 words: ' +
  'a headline, an intro paragraph, three feature sections of two sentences each, and a closing call to action. ' +
  'Reply with the announcement text only; do not call any tool.';

test.use({
  viewport: { width: 1440, height: 900 },
  colorScheme: 'light',
});

test.setTimeout(240_000);

// Same single-writer-race retry as agent-assets.spec.ts, for the same
// measured reason: the embedded runner's poll loop can make an operator
// write lose the SQLite single-writer race (`500 database is locked`,
// marked retryable by the API itself).
async function apiFetch(p: string, init?: RequestInit) {
  let lastErr: unknown;
  for (let attempt = 0; attempt < 6; attempt++) {
    const res = await fetch(`${API}${p}`, init);
    if (res.ok) {
      if (res.status === 204) return null;
      return res.json();
    }
    const text = await res.text();
    if (res.status === 500 && /database is locked/i.test(text) && attempt < 5) {
      lastErr = new Error(
        `${init?.method ?? 'GET'} ${p} -> ${res.status}: ${text}`,
      );
      await new Promise((r) => setTimeout(r, 300 * (attempt + 1)));
      continue;
    }
    throw new Error(`${init?.method ?? 'GET'} ${p} -> ${res.status}: ${text}`);
  }
  throw lastErr;
}

async function waitForRequestState(
  requestId: string,
  wantStates: readonly string[],
  timeoutMs = 120_000,
): Promise<string> {
  const start = Date.now();
  let last = '';
  while (Date.now() - start < timeoutMs) {
    const body = (await apiFetch(`/executions/${requestId}`)) as {
      state?: string;
    };
    last = body?.state ?? last;
    if (last && wantStates.includes(last)) return last;
    await new Promise((r) => setTimeout(r, 400));
  }
  throw new Error(
    `timed out waiting for request ${requestId} to reach one of [${wantStates.join(', ')}] (last seen: ${last || 'none'})`,
  );
}

// Same internal-scroll-aware capture as agent-assets.spec.ts: the app's main
// content area scrolls inside `Layout.tsx`'s overflow container, so a plain
// screenshot only shows one viewport's worth.
async function screenshotFullContent(
  page: Page,
  outPath: string,
): Promise<void> {
  const contentHeight = await page.evaluate(() => {
    let max = 0;
    for (const el of Array.from(document.querySelectorAll('*'))) {
      const style = getComputedStyle(el);
      if (
        (style.overflowY === 'auto' || style.overflowY === 'scroll') &&
        el.scrollHeight > el.clientHeight
      ) {
        max = Math.max(max, el.scrollHeight);
      }
    }
    return max;
  });
  const viewport = page.viewportSize();
  if (viewport && contentHeight > viewport.height) {
    await page.setViewportSize({
      width: viewport.width,
      height: contentHeight + 40,
    });
    await page.waitForTimeout(300);
  }
  await page.screenshot({ path: outPath });
  if (viewport) await page.setViewportSize(viewport);
}

// The Agents page's numbered step sections, addressed by their own headings —
// the same locator strategy e2e/agents-page.spec.ts uses.
function stepSection(page: Page, headingName: string) {
  return page.locator('section').filter({
    has: page.getByRole('heading', { name: headingName, exact: true }),
  });
}

let projectId: string;
let runItemId: string;

// Re-resolve ids from the server when a rerun starts past the test that set
// them (e.g. `--grep` on one failed step), so each test stays runnable alone
// against the same live server.
async function ensureProject(): Promise<void> {
  if (!projectId) {
    const projects = (await apiFetch('/projects')) as Array<{
      id: string;
      name: string;
    }>;
    projectId = projects.find((p) => p.name === PROJECT_NAME)?.id ?? '';
    if (!projectId)
      throw new Error(
        `project "${PROJECT_NAME}" not found — run the earlier tests first`,
      );
  }
}

async function ensureIds(): Promise<void> {
  await ensureProject();
  if (!runItemId) {
    const items = (await apiFetch(`/projects/${projectId}/items`)) as {
      data: Array<{ id: string; title: string }>;
    };
    runItemId = items.data.find((it) => it.title === RUN_ITEM_TITLE)?.id ?? '';
    if (!runItemId)
      throw new Error(
        `item "${RUN_ITEM_TITLE}" not found — run the earlier tests first`,
      );
  }
}
let repoDir: string;
let scratchRoot: string;

test.beforeAll(async () => {
  fs.mkdirSync(OUT_DIR, { recursive: true });
  scratchRoot = fs.mkdtempSync(path.join(os.tmpdir(), 'tutorial-assets-'));

  // A real, disposable git fixture for the repository snapshot — the same
  // shape agent-assets.spec.ts and scripts/smoke.sh use.
  repoDir = path.join(scratchRoot, 'repo');
  fs.mkdirSync(repoDir, { recursive: true });
  execSync('git init -q -b main', { cwd: repoDir });
  fs.writeFileSync(
    path.join(repoDir, 'README.md'),
    '# Demo repository for the Tack tutorial screenshots\n',
  );
  execSync('git add README.md', { cwd: repoDir });
  execSync(
    'git -c user.email=demo@invalid -c user.name=demo commit -q -m seed',
    { cwd: repoDir },
  );

});

test.afterAll(() => {
  if (scratchRoot) fs.rmSync(scratchRoot, { recursive: true, force: true });
});

test.describe.serial('tutorial screenshots', () => {
  test('01–03: first open, create a project', async ({ page }) => {
    await page.goto(`${BASE}/`);
    await waitForApp(page);
    await expect(
      page.getByRole('button', { name: 'New Project' }).first(),
    ).toBeVisible();
    await page.waitForTimeout(600);
    await page.screenshot({ path: path.join(OUT_DIR, '01-first-open.png') });

    await page.getByRole('button', { name: 'New Project' }).first().click();
    await page.waitForTimeout(400);
    await page.getByPlaceholder('My Awesome Project').fill(PROJECT_NAME);
    await page
      .getByPlaceholder('A brief description of your project...')
      .fill(
        'Relaunch the marketing site: new pricing page, new host, public announcement.',
      );
    await page.waitForTimeout(400);
    await page.screenshot({ path: path.join(OUT_DIR, '02-new-project.png') });

    await page.getByRole('button', { name: 'Create Project' }).click();
    await page.waitForTimeout(800);

    const projects = (await apiFetch('/projects')) as Array<{
      id: string;
      name: string;
    }>;
    const project = projects.find((p) => p.name === PROJECT_NAME);
    if (!project)
      throw new Error(
        'the created project did not come back from GET /projects',
      );
    projectId = project.id;

    await page.goto(`${BASE}/projects/${projectId}/board`);
    await waitForApp(page);
    await expect(page.getByText('Backlog').first()).toBeVisible();
    await page.waitForTimeout(400);
    await page.screenshot({ path: path.join(OUT_DIR, '03-board-empty.png') });
  });

  test('04–05: add tasks on the board', async ({ page }) => {
    await ensureProject();
    await page.goto(`${BASE}/projects/${projectId}/board`);
    await waitForApp(page);
    await expect(page.getByText('Backlog').first()).toBeVisible();

    // Idempotent on rerun: only create the titles the board doesn't have yet.
    const existing = (await apiFetch(`/projects/${projectId}/items`)) as {
      data: Array<{ title: string }>;
    };
    const have = new Set(existing.data.map((it) => it.title));
    const titles = [RUN_ITEM_TITLE, ...OTHER_ITEMS].filter((t) => !have.has(t));
    for (const [i, title] of titles.entries()) {
      await page.getByTitle('Add item').first().click();
      await page.waitForTimeout(400);
      await page.getByPlaceholder('What needs to be done?').fill(title);
      if (i === 0 && title === RUN_ITEM_TITLE) {
        // The description field is a rich-text editor (contenteditable), not
        // an <input> — no placeholder attribute to target, so type into it.
        await page.locator('[contenteditable="true"]').first().click();
        await page.keyboard.type(
          'About 400 words for the blog: a headline, an intro, three feature sections and a call to action.',
        );
        await page.waitForTimeout(300);
        await page.screenshot({ path: path.join(OUT_DIR, '04-new-item.png') });
      }
      await page.getByRole('button', { name: 'Create Item' }).click();
      await expect(
        page.getByPlaceholder('What needs to be done?'),
      ).toBeHidden();
      await page.waitForTimeout(300);
    }

    await expect(page.getByText(RUN_ITEM_TITLE).first()).toBeVisible();
    // Let the "Item created successfully" toasts auto-dismiss (4s duration,
    // shared/ui/toast.ts) rather than capturing the board under a stack of
    // transient overlays.
    await page.waitForTimeout(4_500);
    await page.screenshot({ path: path.join(OUT_DIR, '05-board-tasks.png') });
  });

  test('06: write the task a checklist', async ({ page }) => {
    await ensureIds();
    await page.setViewportSize({ width: 1440, height: 1300 });
    await page.goto(`${BASE}/projects/${projectId}/board?item=${runItemId}`);
    await waitForApp(page);
    const drawer = page.getByRole('dialog', { name: 'Item details' });

    // The Brief tab is gone: acceptance criteria are a checklist under the
    // description, one line per thing a person checks, saved on blur.
    const criteria = [
      'About 400 words',
      'Ends with a call to action',
      'A post the team can publish on launch day without rewriting it',
    ];
    for (const [i, text] of criteria.entries()) {
      await drawer.getByRole('button', { name: 'Add check' }).click();
      const line = drawer.getByLabel('Acceptance criterion').nth(i);
      await line.fill(text);
      await line.blur();
      await page.waitForTimeout(500);
    }
    await page.waitForTimeout(4_500);
    await drawer.getByRole('heading', { name: 'Acceptance criteria' }).evaluate((h) => h.scrollIntoView({ block: 'center' }));
    await page.waitForTimeout(300);
    await drawer.screenshot({ path: path.join(OUT_DIR, '06-brief.png') });
  });

  test('07–08: turn agent execution on; the harnesses it finds', async ({ page }) => {
    await page.goto(`${BASE}/agents`);
    await waitForApp(page);
    await expect(
      page.getByText('Agent execution on this machine'),
    ).toBeVisible();
    await expect(page.getByRole('button', { name: 'Turn on' })).toBeVisible();
    await page.waitForTimeout(400);
    await page.screenshot({ path: path.join(OUT_DIR, '07-agents-off.png') });

    await page.getByRole('button', { name: 'Turn on' }).click();
    await expect(
      page.getByText('Running', { exact: true }).first(),
    ).toBeVisible({ timeout: 20_000 });
    // Give the harness-detection section time to probe the installed
    // binaries and their vendor logins before capturing it.
    await page.waitForTimeout(4_000);
    await screenshotFullContent(page, path.join(OUT_DIR, '08-agents-on.png'));
  });

  test('09: the project\'s agent settings', async ({ page }) => {
    await ensureIds();
    // The project's agent settings live in its Automation tab. Choosing a
    // specific default model there is not offered (the model row is disabled),
    // so the run below uses the agent's own default model.
    await page.goto(`${BASE}/projects/${projectId}/settings?tab=automation`);
    await waitForApp(page);
    const agentSettings = page.locator('div').filter({
      has: page.getByRole('heading', { name: 'Agent', exact: true }),
    }).last();
    await expect(page.getByRole('heading', { name: 'Agent', exact: true })).toBeVisible();
    await page.waitForTimeout(600);
    await page.getByRole('heading', { name: 'Agent', exact: true }).scrollIntoViewIfNeeded();
    await page.waitForTimeout(300);
    await agentSettings.screenshot({
      path: path.join(OUT_DIR, '09-default-model.png'),
    });
  });

  test('10: create the agent profile', async ({ page }) => {
    await page.goto(`${BASE}/agents`);
    await waitForApp(page);
    await page.getByRole('button', { name: /^Advanced/ }).click();
    await page.getByRole('tab', { name: 'Profiles', exact: true }).click();
    await page.getByRole('button', { name: '+ Create agent profile' }).click();
    const form = page.locator('form').filter({ has: page.getByLabel('Instructions') });
    await form.getByLabel('Name').fill(PROFILE_NAME);
    await form.getByLabel('Instructions').fill(PROFILE_INSTRUCTIONS);
    // The form alone, in a viewport tall enough that it never scrolls under
    // the sticky page header (an element screenshot of the whole Advanced
    // section did, and captured the header across it).
    await page.setViewportSize({ width: 1440, height: 2200 });
    await form.scrollIntoViewIfNeeded();
    await page.waitForTimeout(400);
    await form.screenshot({ path: path.join(OUT_DIR, '10-agent-profile.png') });
    await page.setViewportSize({ width: 1440, height: 900 });
    await form.getByRole('button', { name: 'Create' }).click();
    await expect(page.getByText(PROFILE_NAME).first()).toBeVisible();
  });

  test('11–15: run the task with Claude Code, track it, see the result', async ({
    page,
  }) => {
    await ensureIds();
    // The dialog has no repository fields: the server fills the repository
    // from the project, so name the disposable git fixture as its folder.
    await apiFetch(`/projects/${projectId}`, {
      method: 'PATCH',
      headers: { 'content-type': 'application/json' },
      body: JSON.stringify({ code_origin: 'folder', repository: repoDir }),
    });
    // Tall enough that the dialog (max 90vh) shows the whole flow unscrolled.
    await page.setViewportSize({ width: 1440, height: 2000 });
    await page.goto(`${BASE}/projects/${projectId}/board`);
    await waitForApp(page);
    await expect(page.getByText(RUN_ITEM_TITLE).first()).toBeVisible();

    // ── The "Run with agent" dialog, filled ────────────────────────────────
    await page
      .getByRole('button', { name: `Run with agent: ${RUN_ITEM_TITLE}` })
      .click();
    const dialog = page.getByRole('dialog', { name: /^Run with agent:/ });
    await expect(dialog).toBeVisible();
    await page.waitForTimeout(600);
    // The profile is a pill; the agent is picked under "Advanced for this run".
    await dialog.getByRole('button', { name: PROFILE_NAME, exact: true }).click();
    await dialog.getByText('Advanced for this run').click();
    await dialog.getByLabel('Agent', { exact: true }).selectOption('claude-code');
    await page.waitForTimeout(500);
    await expect(dialog.getByRole('button', { name: 'Run', exact: true })).toBeEnabled();
    await dialog.screenshot({
      path: path.join(OUT_DIR, '11-run-with-agent.png'),
    });

    // ── Submit — from here on everything is a real, billed attempt ────────
    await dialog.getByRole('button', { name: 'Run', exact: true }).click();
    await page.waitForTimeout(1200);
    await page.setViewportSize({ width: 1440, height: 900 });

    const listed = (await apiFetch(`/executions?item_id=${runItemId}`)) as {
      data: Array<{ request_id: string; requested_harness_kind?: string }>;
    };
    const requestId = listed.data[0]?.request_id;
    if (!requestId)
      throw new Error(
        'no execution request found for this item after clicking Run',
      );

    // ── Track it from the board: the card's own execution chip ────────────
    await page.goto(`${BASE}/projects/${projectId}/board`);
    await waitForApp(page);
    await expect(
      page.getByRole('button', {
        name: `Open the Execution tab for ${RUN_ITEM_TITLE}`,
      }),
    ).toBeVisible();
    await page.waitForTimeout(600);
    await page.screenshot({
      path: path.join(OUT_DIR, '12-board-tracking.png'),
    });

    // ── Up close: the Execution tab while the attempt runs ────────────────
    // Grab the in-flight state the moment its badge shows; if the run beats
    // us to Finished anyway, the capture honestly shows that instead.
    await page.goto(
      `${BASE}/projects/${projectId}/board?item=${runItemId}&tab=execution`,
    );
    await waitForApp(page);
    await expect(
      page
        .getByText('Running', { exact: true })
        .or(page.getByText('Finished', { exact: true }))
        .first(),
    ).toBeVisible({ timeout: 60_000 });
    await page.waitForTimeout(400);
    await page.screenshot({
      path: path.join(OUT_DIR, '13-execution-running.png'),
    });

    // ── The finished attempt, whole card in frame ──────────────────────────
    const finalState = await waitForRequestState(requestId, ['succeeded', 'failed']);
    expect(finalState, 'the tutorial shows a run that succeeded').toBe('succeeded');
    await page.goto(
      `${BASE}/projects/${projectId}/board?item=${runItemId}&tab=execution`,
    );
    await waitForApp(page);
    await expect(
      page.getByText(/^Finished/).first(),
    ).toBeVisible({ timeout: 15_000 });
    // Bring the attempt card fully into the drawer's own scroll viewport —
    // its cost tiles sit below the fold otherwise.
    await page
      .getByRole('button', { name: 'Show timeline, questions & files' })
      .first()
      .scrollIntoViewIfNeeded();
    await page.waitForTimeout(600);
    await page.screenshot({
      path: path.join(OUT_DIR, '14-execution-succeeded.png'),
    });

    // ── The result: the attempt's artifacts, cropped to that section ──────
    // Deliberately NOT a full-page capture of the expanded panel: the raw
    // event payloads EventTimeline falls back to rendering include this
    // machine's real staged paths (agent-assets.spec.ts documents the same
    // trade for attempt.png). The Artifacts section alone shows the result
    // without them.
    await page
      .getByRole('button', { name: 'Show timeline, questions & files' })
      .first()
      .click();
    await page.waitForTimeout(1200);
    const artifactsHeading = page.getByRole('heading', {
      name: 'Files from this run',
      exact: true,
    });
    await expect(artifactsHeading).toBeVisible();
    const artifactsSection = artifactsHeading.locator('xpath=..');
    await artifactsSection.scrollIntoViewIfNeeded();
    await page.waitForTimeout(400);
    await artifactsSection.screenshot({
      path: path.join(OUT_DIR, '15-artifacts.png'),
    });

    console.log(
      `\n✓ tutorial screenshots saved -> ${OUT_DIR} (request ${requestId})\n`,
    );
  });
});

// ── workflow.gif ─────────────────────────────────────────────────────────
// The whole loop on one page and one recording: a new project pointed at a
// folder, a task with its acceptance criteria, the run with Claude Code, the
// card's chip, the finished run, Accept, and the files it changed. Runs after
// the screenshots above, so agent execution is already on. The wait for the
// agent is cut down to a few seconds in the GIF; everything else plays at
// real speed. A second real, billed Claude Code call.
const GIF_PROJECT = 'Docs Site';
const GIF_ITEM = 'Add a changelog page';
const GIF_CRITERIA = [
  'CHANGELOG.md exists at the repository root',
  'It has an Unreleased section',
];

test.describe('workflow gif', () => {
  test('workflow gif', async ({ browser }) => {
    try {
      execSync('ffmpeg -version', { stdio: 'pipe' });
    } catch {
      test.skip(true, 'ffmpeg not found — required to convert the recording to a GIF');
    }
    const framesDir = fs.mkdtempSync(path.join(os.tmpdir(), 'workflow-gif-'));
    const context = await browser.newContext({
      viewport: { width: 1440, height: 900 },
      colorScheme: 'light',
      recordVideo: { dir: framesDir, size: { width: 1440, height: 900 } },
    });
    const page = await context.newPage();
    const started = Date.now();
    const at = () => (Date.now() - started) / 1000;
    const gifRepo = path.join(scratchRoot, 'docs-site');
    fs.mkdirSync(gifRepo, { recursive: true });
    execSync('git init -q -b main', { cwd: gifRepo });
    fs.writeFileSync(path.join(gifRepo, 'README.md'), '# Docs site\n');
    execSync('git add README.md', { cwd: gifRepo });
    execSync('git -c user.email=demo@invalid -c user.name=demo commit -q -m seed', { cwd: gifRepo });

    // ── A project whose code is a folder on this computer ─────────────────
    await page.goto(`${BASE}/`);
    await waitForApp(page);
    await page.waitForTimeout(1200);
    await page.getByRole('button', { name: 'New Project' }).first().click();
    await page.waitForTimeout(500);
    await page.getByPlaceholder('My Awesome Project').pressSequentially(GIF_PROJECT, { delay: 40 });
    await page.getByLabel('An existing folder on this computer').check();
    await page.getByLabel('Code folder').fill(gifRepo);
    await page.getByLabel('Code folder').blur();
    await expect(page.getByText(/^A git repository/)).toBeVisible();
    await page.waitForTimeout(1200);
    await page.getByRole('button', { name: 'Create Project' }).click();
    await expect(page.getByText('Backlog').first()).toBeVisible();
    await page.waitForTimeout(4_500);

    // ── A task, and what "done" means for it ───────────────────────────────
    await page.getByTitle('Add item').first().click();
    await page.waitForTimeout(400);
    await page.getByPlaceholder('What needs to be done?').pressSequentially(GIF_ITEM, { delay: 40 });
    await page.waitForTimeout(500);
    await page.getByRole('button', { name: 'Create Item' }).click();
    await expect(page.getByPlaceholder('What needs to be done?')).toBeHidden();
    await page.waitForTimeout(1200);
    await page.getByText(GIF_ITEM, { exact: true }).first().click();
    const drawer = page.getByRole('dialog', { name: 'Item details' });
    await expect(drawer).toBeVisible();
    await page.waitForTimeout(800);
    for (const [i, text] of GIF_CRITERIA.entries()) {
      await drawer.getByRole('button', { name: 'Add check' }).click();
      const line = drawer.getByLabel('Acceptance criterion').nth(i);
      await line.pressSequentially(text, { delay: 30 });
      await line.blur();
      await page.waitForTimeout(600);
    }
    await page.waitForTimeout(1500);
    await page.keyboard.press('Escape');
    await expect(drawer).toBeHidden();
    await page.waitForTimeout(800);

    // ── Run it with Claude Code ────────────────────────────────────────────
    await page.getByRole('button', { name: `Run with agent: ${GIF_ITEM}` }).click();
    const dialog = page.getByRole('dialog', { name: /^Run with agent:/ });
    await expect(dialog).toBeVisible();
    await page.waitForTimeout(1500);
    await dialog.getByText('Advanced for this run').click();
    await dialog.getByLabel('Agent', { exact: true }).selectOption('claude-code');
    await page.waitForTimeout(1200);
    await dialog.getByRole('button', { name: 'Run', exact: true }).click();
    await expect(dialog).toBeHidden();
    const chip = page.getByRole('button', { name: `Open the Execution tab for ${GIF_ITEM}` });
    await expect(chip).toBeVisible({ timeout: 30_000 });
    await page.waitForTimeout(2500);
    await chip.click();
    await page.waitForTimeout(2000);

    const projects = (await apiFetch('/projects')) as Array<{ id: string; name: string }>;
    const gifProjectId = projects.find((p) => p.name === GIF_PROJECT)?.id;
    if (!gifProjectId) throw new Error(`project "${GIF_PROJECT}" not found`);
    const items = (await apiFetch(`/projects/${gifProjectId}/items`)) as {
      data: Array<{ id: string; title: string }>;
    };
    const gifItemId = items.data.find((it) => it.title === GIF_ITEM)?.id;
    if (!gifItemId) throw new Error(`item "${GIF_ITEM}" not found`);
    const listed = (await apiFetch(`/executions?item_id=${gifItemId}`)) as {
      data: Array<{ request_id: string }>;
    };
    const requestId = listed.data[0]?.request_id;
    if (!requestId) throw new Error('no execution request found after clicking Run');

    // ── The wait, cut in the GIF ───────────────────────────────────────────
    const waitFrom = at();
    const finalState = await waitForRequestState(requestId, ['succeeded', 'failed'], 240_000);
    expect(finalState, 'the GIF shows a run that succeeded').toBe('succeeded');
    await expect(
      page.getByText(/^Finished/).first(),
    ).toBeVisible({ timeout: 15_000 });
    const waitTo = at();
    await page.waitForTimeout(2500);

    // ── Review it: Accept, then what it changed ───────────────────────────
    await page.getByRole('button', { name: 'Accept', exact: true }).first().click();
    await expect(page.getByText(/^Accepted/).first()).toBeVisible();
    await page.waitForTimeout(2000);
    await page.getByRole('button', { name: 'Show timeline, questions & files' }).first().click();
    const files = page.getByRole('heading', { name: 'Files from this run', exact: true });
    await expect(files).toBeVisible();
    await files.scrollIntoViewIfNeeded();
    await page.waitForTimeout(3500);
    await page.keyboard.press('Escape');
    await page.waitForTimeout(3000);

    // ── Flush the video; cut the wait; convert to GIF ─────────────────────
    await page.close();
    await context.close();
    const videoPath = await page.video()!.path();
    const palettePath = path.join(framesDir, 'palette.png');
    const gifPath = path.join(OUT_DIR, '..', 'workflow.gif');
    // Keep 2s of the wait at real speed, then play the rest in 3s.
    const cut = (waitFrom + 2).toFixed(2);
    const resume = waitTo.toFixed(2);
    const squeeze = (3 / Math.max(waitTo - waitFrom - 2, 3)).toFixed(4);
    const cutGraph =
      `[0:v]trim=0.6:${cut},setpts=PTS-STARTPTS[a];` +
      `[0:v]trim=${cut}:${resume},setpts=(PTS-STARTPTS)*${squeeze}[b];` +
      `[0:v]trim=${resume},setpts=PTS-STARTPTS[c];` +
      `[a][b][c]concat=n=3:v=1:a=0,fps=6,scale=960:-2:flags=lanczos`;
    execSync(
      `ffmpeg -y -i "${videoPath}" -filter_complex "${cutGraph},palettegen=stats_mode=diff" -update 1 "${palettePath}"`,
      { stdio: 'pipe' },
    );
    execSync(
      `ffmpeg -y -i "${videoPath}" -i "${palettePath}" -filter_complex "${cutGraph}[x];[x][1:v]paletteuse=dither=bayer:bayer_scale=5:diff_mode=rectangle" "${gifPath}"`,
      { stdio: 'pipe' },
    );
    fs.rmSync(framesDir, { recursive: true, force: true });
    const sizeMB = (fs.statSync(gifPath).size / 1_048_576).toFixed(2);
    console.log(`\n✓ workflow.gif saved (${sizeMB} MB) -> ${gifPath} (request ${requestId})\n`);
  });
});
