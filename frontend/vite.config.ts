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

// When set, the proxy presents this token to the API as the `tack_session`
// cookie, standing in for the cookie the real server sets on the board's HTML.
// Vite serves the HTML itself, so the E2E harness (which starts the API with a
// fixed TACK_API_TOKEN) needs this for the browser to be signed in. The page's
// own code sends no credential of its own.
const sessionToken = process.env.VITE_PROXY_SESSION_TOKEN

export default defineConfig({
  plugins: [solid()],
  server: {
    proxy: {
      '/api': {
        target: proxyTarget,
        changeOrigin: true,
        ws: true,
        ...(sessionToken ? { headers: { Cookie: `tack_session=${sessionToken}` } } : {}),
      },
    },
  },
})
