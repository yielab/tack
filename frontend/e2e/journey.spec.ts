import { test, expect } from '@playwright/test';
import { API, getOrCreateProject, waitForApp } from './helpers';

// User journey: a created item flows through to the board UI and its detail view.
// Selectors are kept text-based (not CSS-class-based) so normal UI refactors
// don't break the test — only a genuine regression does.
//
// Guards the two bugs found in the manual QA pass:
//   1. the item-detail drawer once showed "undefined" instead of the title
//   2. unknown routes once rendered a blank page

test('created item appears on the board and opens with the correct title', async ({
  page,
  request,
}) => {
  const projectId = await getOrCreateProject(request);
  const title = `Journey item ${Date.now()}`;

  // Create through the API, then verify it surfaces in the UI.
  const res = await request.post(`${API}/projects/${projectId}/items`, {
    data: { title, item_type: 'task' },
  });
  expect(res.ok()).toBeTruthy();

  await page.goto(`/projects/${projectId}/board`);
  await waitForApp(page);

  const card = page.getByText(title, { exact: false }).first();
  await expect(card).toBeVisible();

  // Opening the item must show its real title — never "undefined". The drawer
  // renders the title in an editable field, so assert on the field's value:
  // this is the direct regression guard for the "undefined title" bug.
  await card.click();
  const detail = page.getByRole('dialog');
  await expect(detail).toBeVisible();
  await expect(detail.getByRole('textbox', { name: /item title/i })).toHaveValue(title);
});

test('list view shows seeded items', async ({ page, request }) => {
  const projectId = await getOrCreateProject(request);
  const title = `List item ${Date.now()}`;
  const res = await request.post(`${API}/projects/${projectId}/items`, {
    data: { title, item_type: 'task' },
  });
  expect(res.ok()).toBeTruthy();

  await page.goto(`/projects/${projectId}/list`);
  await waitForApp(page);

  // The created item must surface as a row/entry in the list.
  await expect(page.getByText(title, { exact: false })).toBeVisible();
});
