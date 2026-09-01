import { test, expect } from '@playwright/test';
import path from 'path';
import fs from 'fs';
import { execSync } from 'child_process';
import { fileURLToPath } from 'url';
import { API, waitForApp } from './helpers';

// Hero GIF capture for the README. Run with:
//   npx playwright test hero-gif --project=chromium --workers=1
//
// Records the browser session as a webm, then converts to docs/screenshots/hero.gif.

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const OUT_DIR = path.join(__dirname, '../../docs/screenshots');
const FRAMES_DIR = path.join(OUT_DIR, '_frames');

// Offset from today rather than a fixed date, so the timeline bars stay inside
// the default viewport window no matter when this capture is re-run.
function daysFromNow(days: number): string {
  const d = new Date();
  d.setUTCDate(d.getUTCDate() + days);
  d.setUTCHours(0, 0, 0, 0);
  return d.toISOString();
}

test.use({
  viewport: { width: 1280, height: 720 },
  colorScheme: 'light',
  video: { mode: 'on', size: { width: 1280, height: 720 } },
});

test.setTimeout(120_000);

// Guard: skip immediately if ffmpeg is not available (e.g. CI without ffmpeg).
// This file is excluded from the default testIgnore list in playwright.config.ts
// and should only run via `make gif`.
test.beforeEach(async ({}, testInfo) => {
  try {
    execSync('ffmpeg -version', { stdio: 'pipe' });
  } catch {
    testInfo.skip(true, 'ffmpeg not found — run `make gif` locally');
  }
});

// Smooth pointer drag — works with solid-dnd (pointer events, not native HTML5 drag)
async function drag(
  page: import('@playwright/test').Page,
  from: { x: number; y: number },
  to: { x: number; y: number },
  steps = 30,
  dwellMs = 150,
) {
  await page.mouse.move(from.x, from.y);
  await page.mouse.down();
  await page.waitForTimeout(dwellMs);
  await page.mouse.move(to.x, to.y, { steps });
  await page.waitForTimeout(dwellMs);
  await page.mouse.up();
  await page.waitForTimeout(300);
}

test('hero GIF', async ({ page }, testInfo) => {
  fs.mkdirSync(OUT_DIR, { recursive: true });
  fs.mkdirSync(FRAMES_DIR, { recursive: true });

  // ── Seed data ────────────────────────────────────────────────────────────────
  const apiReq = testInfo.project.use as never;
  void apiReq; // unused — we call the API with fetch below
  const baseApi = process.env.E2E_API_ORIGIN
    ? `${process.env.E2E_API_ORIGIN}/api`
    : 'http://127.0.0.1:3399/api';

  const projRes = await fetch(`${baseApi}/projects`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ name: 'Product Launch', project_type: 'software' }),
  });
  const project = await projRes.json() as { id: string };
  const pid = project.id;

  const seedItems = [
    { title: 'User research & interviews', priority: 'medium', targetStatus: 'Backlog' },
    { title: 'Analytics event schema', priority: 'low', targetStatus: 'Backlog' },
    { title: 'Onboarding flow wireframes', priority: 'high', targetStatus: 'Backlog' },
    { title: 'Landing page copywriting', priority: 'medium', targetStatus: 'To Do' },
    { title: 'Pricing page design', priority: 'medium', targetStatus: 'To Do' },
    { title: 'Auth system — OAuth2 + session', priority: 'high', targetStatus: 'In Progress', due_date: daysFromNow(10) },
    { title: 'Dashboard charts & burndown', priority: 'high', targetStatus: 'In Progress', due_date: daysFromNow(15) },
    { title: 'REST API documentation', priority: 'medium', targetStatus: 'In Progress', due_date: daysFromNow(20) },
    { title: 'Board drag-and-drop polish', priority: 'high', targetStatus: 'In Review' },
    { title: 'Dark mode contrast fixes', priority: 'medium', targetStatus: 'In Review' },
    { title: 'Repo & CI setup', priority: 'high', targetStatus: 'Done' },
    { title: 'Tech stack decision', priority: 'medium', targetStatus: 'Done' },
    { title: 'Design system tokens', priority: 'medium', targetStatus: 'Done' },
  ];

  for (const item of seedItems) {
    const { targetStatus, ...body } = item;
    const r = await fetch(`${baseApi}/projects/${pid}/items`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ ...body, item_type: 'task' }),
    });
    const created = await r.json() as { id: string };
    if (targetStatus !== 'Backlog') {
      await fetch(`${baseApi}/items/${created.id}`, {
        method: 'PATCH',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ status: targetStatus }),
      });
    }
  }

  // ── Scene 1: Board view (pause so viewer can read it) ────────────────────────
  await page.goto(`/projects/${pid}/board`);
  await waitForApp(page);
  await expect(page.getByText('Backlog').first()).toBeVisible();
  await page.waitForTimeout(2000);

  // ── Scene 2: Drag "Pricing page design" from To Do → In Progress ─────────────
  const cardText = page.getByText('Pricing page design', { exact: false }).first();
  await expect(cardText).toBeVisible();
  const cardBB = await cardText.boundingBox();

  // Target: centre of the In Progress column header area
  const inProgressHeader = page.getByText('In Progress', { exact: true }).first();
  const inProgressBB = await inProgressHeader.boundingBox();

  if (cardBB && inProgressBB) {
    await drag(
      page,
      { x: cardBB.x + cardBB.width / 2, y: cardBB.y + cardBB.height / 2 },
      // Drop in the column body, below the header
      { x: inProgressBB.x + inProgressBB.width / 2, y: inProgressBB.y + 180 },
      40,   // steps — slow drag for visibility
      200,  // dwell ms at start/end
    );
  }
  await page.waitForTimeout(1000);

  // ── Scene 3: Switch to Timeline ───────────────────────────────────────────────
  // The sidebar now also has a per-lens Timeline link, so scope to the first match.
  await page.getByRole('link', { name: /timeline/i }).first().click();
  await waitForApp(page);
  await page.waitForTimeout(1800);

  // ── Scene 4: Open command palette ─────────────────────────────────────────────
  await page.keyboard.press('Control+k');
  await page.waitForTimeout(400);
  // Type a search term
  await page.keyboard.type('board', { delay: 80 });
  await page.waitForTimeout(1200);
  await page.keyboard.press('Escape');
  await page.waitForTimeout(600);

  // ── Scene 5: Settings → Vocabulary editor ─────────────────────────────────────
  await page.goto(`/projects/${pid}/settings`);
  await waitForApp(page);
  await page.waitForTimeout(500);

  const vocabTab = page
    .getByRole('tab', { name: /vocabulary/i })
    .or(page.getByRole('button', { name: /vocabulary/i }))
    .or(page.getByText('Vocabulary').first());
  await expect(vocabTab).toBeVisible();
  await vocabTab.click();
  await page.waitForTimeout(600);

  // Type a custom label into the "Task" field to show live editing
  const taskInput = page.getByRole('textbox').filter({ hasText: /task/i }).first()
    .or(page.locator('input[value="Task"]').first())
    .or(page.locator('input').nth(2));
  if (await taskInput.isVisible({ timeout: 2000 }).catch(() => false)) {
    await taskInput.click({ clickCount: 3 }); // select all
    await page.keyboard.type('Work Order', { delay: 60 });
    await page.waitForTimeout(1000);
  } else {
    await page.waitForTimeout(1500);
  }

  // Final pause before the recording ends
  await page.waitForTimeout(1000);

  // ── Flush the video ────────────────────────────────────────────────────────────
  await page.close();
  const videoPath = await page.video()!.path();

  // ── Convert webm → GIF ─────────────────────────────────────────────────────────
  const palettePath = path.join(FRAMES_DIR, 'palette.png');
  const gifPath = path.join(OUT_DIR, 'hero.gif');
  // Skip the first 1.5 s of blank loading frames; [0:v] is required in
  // filter_complex so ffmpeg knows which input the scale filter applies to.
  const filter = 'fps=8,scale=1000:-2:flags=lanczos';

  execSync(
    `ffmpeg -y -ss 1.5 -i "${videoPath}" -vf "${filter},palettegen=stats_mode=diff" "${palettePath}"`,
    { stdio: 'pipe' },
  );
  execSync(
    `ffmpeg -y -ss 1.5 -i "${videoPath}" -i "${palettePath}" -filter_complex "[0:v] ${filter} [x]; [x][1:v] paletteuse=dither=bayer:bayer_scale=5:diff_mode=rectangle" "${gifPath}"`,
    { stdio: 'pipe' },
  );

  // Clean up temporary palette
  fs.rmSync(FRAMES_DIR, { recursive: true, force: true });

  const sizeMB = (fs.statSync(gifPath).size / 1_048_576).toFixed(1);
  console.log(`\n✓ hero.gif saved (${sizeMB} MB) → ${gifPath}\n`);
});
