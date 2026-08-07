import { defineConfig } from 'vitest/config';
import solid from 'vite-plugin-solid';

export default defineConfig({
  plugins: [solid()],
  resolve: { conditions: ['development', 'browser'] },
  test: {
    environment: 'jsdom',
    include: ['src/**/*.{test,spec}.{ts,tsx}'],
    setupFiles: ['./src/test/setup.ts'],
    globals: true,
    // Pin the API base so tests assert the relative-default behavior,
    // independent of the dev-only .env (which points at the absolute host).
    env: { VITE_API_URL: '/api' },
  },
});
