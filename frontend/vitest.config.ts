import { defineConfig } from 'vitest/config';

export default defineConfig({
  test: {
    environment: 'jsdom',
    include: ['src/**/*.{test,spec}.{ts,tsx}'],
    globals: true,
    // Pin the API base so tests assert the relative-default behavior,
    // independent of the dev-only .env (which points at the absolute host).
    env: { VITE_API_URL: '/api' },
  },
});
