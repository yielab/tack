import { defineConfig, devices } from '@playwright/test';

// Config for capturing the step-by-step tutorial screenshots
// (docs/screenshots/tutorial/*.png — the images docs/book/src/user-guide/
// tutorial.md embeds). Same posture as playwright.agent-assets.config.ts,
// for the same reason: NO webServer block, because the target is an
// already-running RELEASE build of `tack serve` (built with
// `--features embed-spa`), started against a FRESH database so the captures
// show a first run, with a real, signed-in `claude` on PATH — never the
// dev pair the default config starts, and never the fake harness shims.
// Not `--with-runner`: that turns execution on at start, and the tutorial
// photographs turning it on. The one execution this suite drives is a real,
// live, billed Claude Code call.
//
//   S=$(mktemp -d); TACK_PORT=3311 TACK_DATABASE_URL="sqlite:$S/tack.db?mode=rwc" \
//     TACK_STORAGE_DIR=$S/storage ./target/release/tack serve
//   npx playwright test e2e/tutorial-assets.spec.ts \
//     --config playwright.tutorial-assets.config.ts --project=chromium --workers=1
export default defineConfig({
  testDir: './e2e',
  testMatch: ['**/tutorial-assets.spec.ts'],
  fullyParallel: false,
  retries: 0,
  workers: 1,
  reporter: 'list',
  timeout: 300_000,
  expect: { timeout: 15_000 },
  use: {
    trace: 'off',
    screenshot: 'only-on-failure',
    actionTimeout: 15_000,
  },
  projects: [{ name: 'chromium', use: { ...devices['Desktop Chrome'] } }],
});
