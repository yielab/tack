import { test, expect } from '@playwright/test';
import path from 'path';
import fs from 'fs';
import { fileURLToPath } from 'url';
import { API, waitForApp } from './helpers';

// Screenshot capture for the README. Run with:
//   npx playwright test screenshots --project=chromium --workers=1
//
// Outputs PNG files to docs/screenshots/ at repo root.

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const OUT_DIR = path.join(__dirname, '../../docs/screenshots');

test.use({
  viewport: { width: 1440, height: 900 },
  colorScheme: 'light',
});

test.setTimeout(90_000);

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
      { title: 'Auth system — OAuth2 + session', priority: 'high', targetStatus: 'In Progress', due_date: '2026-07-10T00:00:00Z' },
      { title: 'Dashboard charts & burndown', priority: 'high', targetStatus: 'In Progress', due_date: '2026-07-15T00:00:00Z' },
      { title: 'REST API documentation', priority: 'medium', targetStatus: 'In Progress', due_date: '2026-07-20T00:00:00Z' },
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

      if (targetStatus !== 'Backlog') {
        const created = await r.json();
        const patch = await request.patch(`${API}/items/${created.id}`, {
          data: { status: targetStatus },
        });
        expect(patch.ok(), `move "${item.title}" → ${targetStatus}: ${patch.status()}`).toBeTruthy();
      }
    }
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
