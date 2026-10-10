import { defineConfig, devices } from '@playwright/test';

// Config for e2e/docket-prill.spec.ts: the whole workflow from a new project to
// prill's merge-readiness pack, with docket as the agent. Same posture as
// playwright.agent-assets.config.ts, for the same reason: NO webServer block,
// because the target is an already-running `tack serve` built with
// `--features embed-spa` against a fresh database, started together with a
// runner configuration by scripts/docket-prill-e2e.sh — never the dev
// `cargo run`/`npm run dev` pair the default config starts, and never the fake
// harness shims. Every run the spec makes is a real model call against the
// endpoint that script names.
//
//   scripts/docket-prill-e2e.sh
//
// is the whole recipe; see that script's header for the knobs.
export default defineConfig({
  testDir: './e2e',
  testMatch: ['**/docket-prill.spec.ts'],
  fullyParallel: false,
  retries: 0,
  workers: 1,
  reporter: [['list'], ['html', { open: 'never', outputFolder: 'playwright-report/docket-prill' }]],
  outputDir: 'test-results/docket-prill-run',
  timeout: 900_000,
  expect: { timeout: 20_000 },
  use: {
    trace: 'retain-on-failure',
    screenshot: 'only-on-failure',
    actionTimeout: 20_000,
  },
  projects: [{ name: 'chromium', use: { ...devices['Desktop Chrome'] } }],
});
