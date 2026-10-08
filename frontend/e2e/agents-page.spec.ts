import { test, expect, waitForApp } from './helpers';

// The Agents page — this computer only. `playwright.config.ts` prepends
// `e2e/fixtures/harness-shims/` to the API webServer's own PATH, so the
// embedded runner's real probe (a real subprocess exec, not a mock) always
// finds these two fake `claude`/`codex` binaries instead of whatever is or
// isn't installed on the machine running the suite — see that file's own
// header comment. Their reported versions (9.9.1/9.9.2) are the load-bearing
// proof that step 1-2 assertions below observe the real probe, not
// something the frontend fabricated.
//
// The first test below flips the same server-wide "agent execution on/off"
// switch `execution-toggle.spec.ts` and `provider-key-panel.spec.ts` each
// drive — it holds `executionToggleLock` (`./helpers.ts`) for that reason;
// see that fixture's own doc comment before touching either.

test.beforeEach(async ({ page }) => {
  page.on('pageerror', (err) => {
    throw new Error(`Uncaught page error: ${err.message}`);
  });
});

test('turning agent execution on reveals both agents installed at the shim\'s own version, not signed in, and enables the test run', async ({
  page,
  executionToggleLock,
}) => {
  await page.goto('/agents');
  await waitForApp(page);

  const toggleButton = page.getByRole('button', { name: /Turn (on|off)/ });
  await expect(toggleButton).toBeVisible();

  // Leave a clean, known "off" state if a previous run left it on.
  if ((await toggleButton.textContent())?.includes('Turn off')) {
    await toggleButton.click();
    await expect(page.getByText('Stopped', { exact: true })).toBeVisible();
  }

  await expect(page.getByText("Turn on agent execution above to see what's installed here.")).toBeVisible();

  await toggleButton.click();
  await expect(page.getByText('Running', { exact: true })).toBeVisible({ timeout: 15_000 });

  // Both fake harnesses report at the shim's own version — the real
  // embedded runner's own probe reading a real subprocess, not anything
  // this page fabricated.
  await expect(page.getByText('Installed v9.9.1')).toBeVisible({ timeout: 10_000 });
  await expect(page.getByText('Installed v9.9.2')).toBeVisible();

  // The fake shims never ran a test, so neither agent is signed in.
  await expect(page.getByText('Not signed in').first()).toBeVisible();
  await expect(page.getByText('Not signed in')).toHaveCount(2);

  // The click flow (POST /api/local-runner/test-run) is asserted after S5 lands.
  await expect(page.getByRole('button', { name: 'Run test' })).toBeEnabled();

  // Leave the machine as this test found it, for every other spec in this
  // suite that assumes agent execution starts off — but only when it is
  // still this test's own to turn off. `execution-toggle.spec.ts` and
  // `provider-key-panel.spec.ts` drive this identical, single, server-wide
  // switch too; if one of them already turned it off while this test was
  // busy reloading and re-reading the saved default above, the goal this
  // step exists for is already met, and clicking a button that no longer
  // reads "Turn off" would itself be the flaky assertion.
  const offAlready = (await page.getByRole('button', { name: /Turn (on|off)/ }).textContent())?.includes('Turn on');
  if (!offAlready) {
    await page.getByRole('button', { name: 'Turn off' }).click();
    await expect(page.getByText('Stopped', { exact: true })).toBeVisible({ timeout: 15_000 });
  }
});
