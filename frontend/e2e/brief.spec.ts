import { test, expect } from '@playwright/test';
import { getOrCreateProject, createFreshItem, waitForApp } from './helpers';

// One journey: write a two-criterion brief on an item, reload, both persist.
test('a brief with two criteria survives a reload', async ({ page, request }) => {
  const projectId = await getOrCreateProject(request);
  const itemId = await createFreshItem(request, projectId, `Brief journey ${Date.now()}`);

  await page.goto(`/projects/${projectId}/board?item=${itemId}`);
  await waitForApp(page);
  const drawer = page.getByRole('dialog', { name: 'Item details' });
  await drawer.getByRole('tab', { name: 'Brief' }).click();

  await drawer.getByRole('button', { name: 'Add criterion' }).click();
  await drawer.getByLabel('Title').first().fill('Build passes');
  await drawer.getByLabel('Command').fill('cargo build');

  await drawer.getByRole('button', { name: 'Add criterion' }).click();
  const second = drawer.getByTestId('criterion').nth(1);
  await second.getByLabel('Kind').selectOption('manual');
  await second.getByLabel('Title').fill('Reads well');
  await second.getByLabel('What a person must check').fill('The copy is clear');

  await drawer.getByRole('button', { name: 'Save brief' }).click();
  await expect(page.getByText('Brief saved')).toBeVisible();

  await page.reload();
  await waitForApp(page);
  await drawer.getByRole('tab', { name: 'Brief' }).click();
  await expect(drawer.getByTestId('criterion')).toHaveCount(2);
  await expect(drawer.getByLabel('Command')).toHaveValue('cargo build');
  await expect(drawer.getByTestId('criterion').nth(1).getByLabel('What a person must check')).toHaveValue(
    'The copy is clear',
  );
});
