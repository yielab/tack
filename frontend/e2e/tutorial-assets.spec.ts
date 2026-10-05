import { test, expect, type Page } from '@playwright/test';
import path from 'path';
import fs from 'fs';
import os from 'os';
import { execSync } from 'child_process';
import { fileURLToPath } from 'url';
import { waitForApp } from './helpers';

// Captures the step-by-step tutorial screenshots (docs/screenshots/tutorial/)
// that docs/book/src/user-guide/tutorial.md embeds, every step done through
// the UI: first open, project creation, adding tasks, a task's brief, turning
// agent execution on (the harnesses it finds), a default model, an agent
// profile, the "Run with agent" dialog, and one REAL Claude Code execution
// tracked from the board chip through the Execution tab to its artifacts.
//
// Run against an ALREADY-RUNNING release build of `tack serve` (built with
// `--features embed-spa`), started with a FRESH database so the first
// screenshot really is a first open, with a real, signed-in `claude` on PATH
// — see playwright.tutorial-assets.config.ts for the exact recipe.
// The one execution this file creates is a real, live, billed model call.

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
const MODEL_PROVIDER = 'anthropic';
const MODEL_ID = 'claude-sonnet-5-5';
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
let repoRev: string;
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
  repoRev = execSync('git rev-parse HEAD', { cwd: repoDir }).toString().trim();

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

  test('06: write the task a brief', async ({ page }) => {
    await ensureIds();
    await page.setViewportSize({ width: 1440, height: 1300 });
    await page.goto(`${BASE}/projects/${projectId}/board?item=${runItemId}`);
    await waitForApp(page);
    const drawer = page.getByRole('dialog');
    await drawer.getByRole('tab', { name: 'Brief' }).click();
    const panel = drawer.getByRole('tabpanel');

    const criteria: Array<[string, string]> = [
      ['About 400 words', 'Count the words of the post; between 350 and 450 is fine.'],
      ['Ends with a call to action', 'The last paragraph tells the reader what to do next.'],
    ];
    for (const [i, [title, check]] of criteria.entries()) {
      await panel.getByRole('button', { name: 'Add criterion' }).click();
      await panel.getByRole('combobox', { name: 'Kind' }).nth(i).selectOption({ label: 'Manual (a person checks)' });
      await panel.getByRole('textbox', { name: 'Title', exact: true }).nth(i).fill(title);
      await panel.getByLabel('What a person must check').nth(i).fill(check);
    }
    await panel
      .getByLabel('Definition of done')
      .fill('A post the team can publish on launch day without rewriting it.');
    await panel.getByRole('button', { name: 'Save brief' }).click();
    await page.waitForTimeout(4_500);
    await drawer.getByRole('tablist').evaluate((t) => t.scrollIntoView({ block: 'start' }));
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

  test('09: save a default model for the project', async ({ page }) => {
    await ensureIds();
    await page.goto(`${BASE}/agents`);
    await waitForApp(page);
    const modelDefault = stepSection(page, 'Default model');
    await expect(modelDefault).toBeVisible();

    // The project picker only renders once more than one project exists —
    // on this tutorial's fresh database there is exactly one, so the section
    // already targets it (same conditional e2e/agents-page.spec.ts uses).
    const picker = modelDefault.getByLabel('Project', { exact: true });
    if (await picker.isVisible().catch(() => false)) {
      await picker.selectOption({ label: PROJECT_NAME });
    }
    await page.waitForTimeout(300);
    await modelDefault.getByRole('radio', { name: 'Type a model id' }).check();
    await page.waitForTimeout(300);
    await modelDefault.getByLabel('Provider', { exact: true }).fill(MODEL_PROVIDER);
    await modelDefault.getByLabel('Model ID', { exact: true }).fill(MODEL_ID);
    await page.waitForTimeout(300);
    await modelDefault.getByRole('button', { name: 'Save' }).click();
    await page.waitForTimeout(800);
    await modelDefault.scrollIntoViewIfNeeded();
    await page.waitForTimeout(300);
    await modelDefault.screenshot({
      path: path.join(OUT_DIR, '09-default-model.png'),
    });
  });

  test('10: create the agent profile', async ({ page }) => {
    await page.goto(`${BASE}/agents`);
    await waitForApp(page);
    await page.getByRole('button', { name: /^Advanced/ }).click();
    await page.getByRole('tab', { name: 'Agent profiles' }).click();
    await page.getByRole('button', { name: '+ Create agent profile' }).click();
    const form = page.locator('form').filter({ has: page.getByLabel('Instructions') });
    await form.getByLabel('Name').fill(PROFILE_NAME);
    await form.getByLabel('Instructions').fill(PROFILE_INSTRUCTIONS);
    const advanced = page.locator('section#advanced');
    await advanced.scrollIntoViewIfNeeded();
    await page.waitForTimeout(300);
    await advanced.screenshot({ path: path.join(OUT_DIR, '10-agent-profile.png') });
    await form.getByRole('button', { name: 'Create' }).click();
    await expect(page.getByText(PROFILE_NAME).first()).toBeVisible();
  });

  test('11–15: run the task with Claude Code, track it, see the result', async ({
    page,
  }) => {
    await ensureIds();
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
    await dialog.getByLabel('Harness').selectOption('claude-code');
    await page.waitForTimeout(400);
    await dialog.getByRole('button', { name: 'Change for this run' }).click();
    await page.waitForTimeout(300);
    await dialog.getByLabel('Remote').fill(repoDir);
    await dialog.getByLabel('Base revision').fill(repoRev);
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
    // us to Succeeded anyway, the capture honestly shows that instead.
    await page.goto(
      `${BASE}/projects/${projectId}/board?item=${runItemId}&tab=execution`,
    );
    await waitForApp(page);
    await expect(
      page
        .getByText('Running', { exact: true })
        .or(page.getByText('Succeeded', { exact: true }))
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
      page.getByText('Succeeded', { exact: true }).first(),
    ).toBeVisible({ timeout: 15_000 });
    // Bring the attempt card fully into the drawer's own scroll viewport —
    // its cost tiles sit below the fold otherwise.
    await page
      .getByRole('button', { name: 'Show events, decisions & artifacts' })
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
      .getByRole('button', { name: 'Show events, decisions & artifacts' })
      .first()
      .click();
    await page.waitForTimeout(1200);
    const artifactsHeading = page.getByRole('heading', {
      name: 'Artifacts',
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
