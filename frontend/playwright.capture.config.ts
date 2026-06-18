import { defineConfig } from '@playwright/test';
import base from './playwright.config';

// Config for the local-only capture specs (README screenshots + hero GIF).
// The main playwright.config.ts testIgnores those specs so they never run in
// CI; this config re-includes them while reusing the same webServer, projects,
// and settings. Used by `make screenshots` and `make gif`.
export default defineConfig({
  ...base,
  testIgnore: undefined,
  testMatch: ['**/screenshots.spec.ts', '**/hero-gif.spec.ts'],
});
