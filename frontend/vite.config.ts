import { defineConfig } from 'vite'
import solid from 'vite-plugin-solid'

// The dev server proxies /api (REST + WebSocket) to the flexpm-api server so the
// SPA runs same-origin. This avoids CORS entirely and mirrors production, where
// the API binary serves the SPA from the same origin (see README, embed-spa).
export default defineConfig({
  plugins: [solid()],
  server: {
    proxy: {
      '/api': {
        target: 'http://127.0.0.1:3210',
        changeOrigin: true,
        ws: true,
      },
    },
  },
})
