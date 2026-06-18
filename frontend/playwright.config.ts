import { defineConfig, devices } from '@playwright/test';

// End-to-end tests drive the real app: the flexpm-api server + the Vite-served
// SPA, in a real browser. Playwright owns the lifecycle of both servers via the
// `webServer` block below, so `npm run test:e2e` is the only command needed.
//
// The API runs against a throwaway SQLite file (e2e.db) and storage dir so a
// run never touches your working database.

// Dedicated e2e ports so a dev server already running on the standard ports
// (3210 API / 5173 SPA) is never reused in place of an isolated test instance.
// The API runs against a throwaway e2e.db; the SPA proxies to it via
// VITE_PROXY_TARGET. This makes local and CI runs identical and hermetic.
const API_PORT = 3399;
const WEB_PORT = 5199;
const isCI = !!process.env.CI;

// The test runner (and helpers/api.spec) talk to the API directly on this origin.
process.env.E2E_API_ORIGIN = process.env.E2E_API_ORIGIN || `http://127.0.0.1:${API_PORT}`;

export default defineConfig({
  testDir: './e2e',
  // Capture specs (screenshots / hero GIF) are local-only tools — they require
  // ffmpeg and a running dev environment. Exclude them from the default CI suite;
  // run them explicitly with `make screenshots` or `make gif`.
  testIgnore: ['**/screenshots.spec.ts', '**/hero-gif.spec.ts'],
  // One test file shouldn't leak state into another; each creates what it needs.
  fullyParallel: true,
  forbidOnly: isCI,
  retries: isCI ? 2 : 0,
  // SQLite is single-writer; serialize in CI to avoid write-contention flakes.
  workers: isCI ? 1 : undefined,
  reporter: isCI ? [['list'], ['html', { open: 'never' }]] : 'list',
  timeout: 30_000,
  expect: { timeout: 10_000 },

  use: {
    baseURL: `http://localhost:${WEB_PORT}`,
    trace: 'on-first-retry',
    screenshot: 'only-on-failure',
    video: isCI ? 'on-first-retry' : 'off',
  },

  projects: [
    { name: 'chromium', use: { ...devices['Desktop Chrome'] } },
    { name: 'firefox', use: { ...devices['Desktop Firefox'] } },
    { name: 'webkit', use: { ...devices['Desktop Safari'] } },
  ],

  webServer: [
    {
      // Run from frontend/ (the config dir) — NOT the repo root — so the e2e
      // API does not pick up the repo-root flexpm.toml. Config::load() returns
      // early when flexpm.toml exists and ignores all FLEXPM_* env vars, so the
      // toml's absence here is what lets FLEXPM_PORT / DATABASE_URL take effect.
      // cargo still resolves the workspace by searching upward.
      command: 'cargo run -p flexpm-api',
      url: `http://127.0.0.1:${API_PORT}/api/health`,
      timeout: 180_000, // first compile can be slow
      reuseExistingServer: !isCI,
      stdout: 'ignore',
      stderr: 'pipe',
      env: {
        FLEXPM_PORT: String(API_PORT),
        FLEXPM_DATABASE_URL: 'sqlite:e2e.db?mode=rwc',
        FLEXPM_STORAGE_DIR: './storage-e2e',
        FLEXPM_LOG_LEVEL: 'warn',
      },
    },
    {
      command: `npm run dev -- --port ${WEB_PORT} --strictPort`,
      url: `http://localhost:${WEB_PORT}`,
      timeout: 60_000,
      reuseExistingServer: !isCI,
      env: {
        VITE_API_URL: '/api', // force same-origin proxy, ignore any .env override
        VITE_PROXY_TARGET: `http://127.0.0.1:${API_PORT}`, // proxy to the isolated e2e API
      },
    },
  ],
});
