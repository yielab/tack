import { test, expect } from '@playwright/test';
import { API, getOrCreateProject, waitForApp } from './helpers';

// User journey for the Table view (editable grid). Text-based selectors keep the
// test resilient to styling refactors — only a real regression breaks it.

test('table view lists items and filters them', async ({ page, request }) => {
  const projectId = await getOrCreateProject(request);
  const title = `Table item ${Date.now()}`;
  const res = await request.post(`${API}/projects/${projectId}/items`, {
    data: { title, item_type: 'task' },
  });
  expect(res.ok()).toBeTruthy();

  await page.goto(`/projects/${projectId}/table`);
  await waitForApp(page);

  // The created item surfaces as a row.
  await expect(page.getByText(title, { exact: false })).toBeVisible();

  // Filtering to a non-matching string hides it; clearing brings it back.
  const filter = page.getByRole('searchbox', { name: /filter items/i });
  await filter.fill('zzz-no-match-zzz');
  await expect(page.getByText(title, { exact: false })).toHaveCount(0);
  await filter.fill('');
  await expect(page.getByText(title, { exact: false })).toBeVisible();
});

test('table inline title edit persists', async ({ page, request }) => {
  const projectId = await getOrCreateProject(request);
  const title = `Editable ${Date.now()}`;
  const res = await request.post(`${API}/projects/${projectId}/items`, {
    data: { title, item_type: 'task' },
  });
  expect(res.ok()).toBeTruthy();

  await page.goto(`/projects/${projectId}/table`);
  await waitForApp(page);

  // Click the title cell to enter edit mode, change it, and commit with Enter.
  // Scope the editor to the table so we don't grab the top-bar search box.
  await page.getByRole('button', { name: title, exact: false }).first().click();
  const editor = page.locator('table').getByRole('textbox').first();
  const newTitle = `${title} (edited)`;
  await editor.fill(newTitle);
  await editor.press('Enter');

  // The new title must persist (it round-trips through the API + refetch).
  await expect(page.getByText(newTitle, { exact: false })).toBeVisible();
});
