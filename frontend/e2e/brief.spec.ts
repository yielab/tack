import { test, expect } from '@playwright/test';
import { getOrCreateProject, createFreshItem, waitForApp } from './helpers';

// One journey: a checklist line and a machine check on an item, reload, both persist.
test('a checklist line and a check for the agent survive a reload', async ({ page, request }) => {
  const projectId = await getOrCreateProject(request);
  const itemId = await createFreshItem(request, projectId, `Brief journey ${Date.now()}`);

  await page.goto(`/projects/${projectId}/board?item=${itemId}`);
  await waitForApp(page);
  const drawer = page.getByRole('dialog', { name: 'Item details' });

  // The manual line is saved on blur.
  await drawer.getByRole('button', { name: 'Add check' }).click();
  await drawer.getByLabel('Acceptance criterion').fill('The copy is clear');
  const savedLine = page.waitForResponse(
    (r) => r.url().endsWith(`/api/items/${itemId}/brief`) && r.request().method() === 'PUT' && r.ok(),
  );
  await drawer.getByLabel('Acceptance criterion').blur();
  await savedLine;

  // The machine check lives in the collapsed "For the agent" block.
  await drawer.locator('summary').click();
  await drawer.getByRole('button', { name: 'Add criterion' }).click();
  await drawer.getByTestId('criterion').getByLabel('Title').fill('Build passes');
  await drawer.getByLabel('Command').fill('cargo build');
  const savedAgent = page.waitForResponse(
    (r) => r.url().endsWith(`/api/items/${itemId}/brief`) && r.request().method() === 'PUT' && r.ok(),
  );
  await drawer.getByRole('button', { name: 'Save for the agent' }).click();
  await savedAgent;

  await page.reload();
  await waitForApp(page);
  await expect(drawer.getByLabel('Acceptance criterion')).toHaveValue('The copy is clear');
  await drawer.locator('summary').click();
  await expect(drawer.getByTestId('criterion')).toHaveCount(1);
  await expect(drawer.getByLabel('Command')).toHaveValue('cargo build');
});
