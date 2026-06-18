import { defineConfig } from 'vite'
import solid from 'vite-plugin-solid'

// The dev server proxies /api (REST + WebSocket) to the tack-api server so the
// SPA runs same-origin. This avoids CORS entirely and mirrors production, where
// the API binary serves the SPA from the same origin (see README, embed-spa).
//
// VITE_PROXY_TARGET lets the E2E harness point the proxy at an isolated API
// instance on a dedicated port (see frontend/playwright.config.ts). Defaults to
// the standard dev API port.
const proxyTarget = process.env.VITE_PROXY_TARGET || 'http://127.0.0.1:3210'

export default defineConfig({
  plugins: [solid()],
  server: {
    proxy: {
      '/api': {
        target: proxyTarget,
        changeOrigin: true,
        ws: true,
      },
    },
  },
})
