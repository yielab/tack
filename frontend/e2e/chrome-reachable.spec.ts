import { test, expect, createRunnableProject, createFreshItem, enrollRunner, waitForApp } from './helpers';

// The chrome around a project at a laptop height: the sidebar reaches its last links and the run
// dialog's footer is the last thing in the dialog, with nothing of the form showing beneath it.

test.use({ viewport: { width: 1440, height: 900 } });

test('Sidebar: Agents and Settings are reachable at 1440x900 inside a project, and the nav shows it can scroll', async ({ page, request }) => {
  const projectId = await createRunnableProject(request, `Chrome sidebar ${Date.now()}`);
  await page.goto(`/projects/${projectId}/board`);
  await waitForApp(page);

  const nav = page.getByRole('navigation', { name: 'Main' });
  const more = page.getByTestId('nav-more');
  // The links below the fold are cut by the nav's edge, and the nav says there is more under it.
  await expect(more).toBeVisible();

  await nav.hover();
  await page.mouse.wheel(0, 600);
  await expect(more).toBeHidden();
  await page.screenshot({ path: 'test-results/chrome-sidebar-1440x900.png' });

  const navBox = (await nav.boundingBox())!;
  for (const name of ['Agents', 'Settings']) {
    const link = nav.getByRole('link', { name, exact: true });
    await expect(link).toBeInViewport({ ratio: 1 });
    const box = (await link.boundingBox())!;
    expect(box.y + box.height, `${name} is cut by the nav's lower edge`).toBeLessThanOrEqual(navBox.y + navBox.height + 0.5);
    // Whatever is drawn at the link's centre is the link, not the footer or the container.
    const covered = await link.evaluate((el) => {
      const r = el.getBoundingClientRect();
      const hit = document.elementFromPoint(r.left + r.width / 2, r.top + r.height / 2);
      return !(hit && el.contains(hit));
    });
    expect(covered, `${name} is covered by something else`).toBe(false);
  }
  // Nothing of the sidebar sticks out under the window's edge.
  const sidebar = await page.locator('#sidebar').boundingBox();
  expect(sidebar!.y + sidebar!.height).toBeLessThanOrEqual(900);

  // The way back up: the cue returns when the nav is scrolled away from its end.
  await page.mouse.wheel(0, -600);
  await expect(more).toBeVisible();
});

test('Run dialog: the footer is the last row and no field shows beneath it', async ({ page, request }) => {
  const projectId = await createRunnableProject(request, `Chrome dialog ${Date.now()}`);
  await createFreshItem(request, projectId, `Chrome dialog item ${Date.now()}`);
  await enrollRunner(request, `Chrome dialog runner ${Date.now()}`, 'opaque/model-alpha');
  await page.goto(`/projects/${projectId}/board`);
  await waitForApp(page);

  await page.getByRole('button', { name: /^Run with agent:/ }).first().click();
  const dialog = page.getByRole('dialog', { name: /^Run with agent:/ });
  await expect(dialog).toBeVisible();
  // Open every section so the form is taller than the window and has to scroll.
  for (const summary of await dialog.locator('details > summary').all()) {
    const open = await summary.evaluate((el) => (el.parentElement as HTMLDetailsElement).open);
    if (!open) await summary.click();
  }
  const footer = dialog.getByTestId('run-footer');
  await expect(footer).toBeVisible();

  const probe = async () =>
    footer.evaluate((el) => {
      const dialogBox = el.closest('[role="dialog"]')!.getBoundingClientRect();
      const box = el.getBoundingClientRect();
      // A point just above the dialog's lower edge, away from its rounded corners.
      const hit = document.elementFromPoint(dialogBox.left + dialogBox.width / 2, dialogBox.bottom - 4);
      return {
        gap: Math.round(dialogBox.bottom - box.bottom),
        footerIsLast: !!hit && !!hit.closest('[data-testid="run-footer"]'),
      };
    });

  const scroller = dialog.locator('.overflow-y-auto').first();
  for (const top of [0, 200, 100000]) {
    await scroller.evaluate((el, t) => { el.scrollTop = t; }, top);
    const { gap, footerIsLast } = await probe();
    expect(gap, `footer sits ${gap}px above the dialog's lower edge at scrollTop ${top}`).toBeLessThanOrEqual(1);
    expect(footerIsLast, `a form control shows beneath the footer at scrollTop ${top}`).toBe(true);
  }
  await scroller.evaluate((el) => { el.scrollTop = 300; });
  await page.screenshot({ path: 'test-results/chrome-run-dialog-1440x900.png' });
});

test.describe('Toasts', () => {
  const LONG = 'Decision token given for this browser session. It stays in this tab only and is forgotten when the tab closes.';
  for (const width of [375, 1440]) {
    test(`a long toast stays inside the window at ${width}px wide`, async ({ page, request }) => {
      await page.setViewportSize({ width, height: 900 });
      const projectId = await createRunnableProject(request, `Chrome toast ${Date.now()}`);
      await page.goto(`/projects/${projectId}/board`);
      await waitForApp(page);
      await page.evaluate(async (message) => {
        const { toast } = await import('/src/shared/ui/toast.ts');
        toast.error(message);
      }, LONG);
      const toast = page.getByRole('alert').filter({ hasText: 'Decision token given' });
      await expect(toast).toBeVisible();
      const box = (await toast.boundingBox())!;
      expect(box.x, 'toast pokes off the left edge').toBeGreaterThanOrEqual(0);
      expect(box.x + box.width, 'toast pokes off the right edge').toBeLessThanOrEqual(width);
      expect(box.y + box.height, 'toast pokes off the bottom edge').toBeLessThanOrEqual(900);
      // The dismiss button is reachable inside the window too.
      const dismiss = (await toast.getByRole('button', { name: 'Dismiss notification' }).boundingBox())!;
      expect(dismiss.x).toBeGreaterThanOrEqual(0);
      expect(dismiss.x + dismiss.width).toBeLessThanOrEqual(width);
      await page.screenshot({ path: `test-results/chrome-toast-${width}.png` });
    });
  }
});
