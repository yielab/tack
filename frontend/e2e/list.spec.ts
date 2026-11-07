import { test, expect } from '@playwright/test';
import { API, getOrCreateProject, waitForApp } from './helpers';

// User journey for the List's filter and in-place edit. Text-based selectors keep the
// test resilient to styling refactors — only a real regression breaks it.

test('list view lists items and filters them', async ({ page, request }) => {
  const projectId = await getOrCreateProject(request);
  const title = `List item ${Date.now()}`;
  const res = await request.post(`${API}/projects/${projectId}/items`, {
    data: { title, item_type: 'task' },
  });
  expect(res.ok()).toBeTruthy();

  await page.goto(`/projects/${projectId}/list`);
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

test('list inline title edit persists', async ({ page, request }) => {
  const projectId = await getOrCreateProject(request);
  const title = `Editable ${Date.now()}`;
  const res = await request.post(`${API}/projects/${projectId}/items`, {
    data: { title, item_type: 'task' },
  });
  expect(res.ok()).toBeTruthy();

  await page.goto(`/projects/${projectId}/list`);
  await waitForApp(page);

  // Press the title's edit button (the title itself opens the item), change it, and commit with Enter.
  // Name the editor by its row so we don't grab the top-bar search box.
  await page.getByRole('button', { name: `Edit title of ${title}` }).click();
  const editor = page.getByRole('textbox', { name: `Title of ${title}` });
  const newTitle = `${title} (edited)`;
  await editor.fill(newTitle);
  await editor.press('Enter');

  // The new title must persist (it round-trips through the API + refetch).
  await expect(page.getByText(newTitle, { exact: false })).toBeVisible();
});
