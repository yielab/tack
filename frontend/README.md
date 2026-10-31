# Tack Frontend

The board's web UI, built with SolidJS, TypeScript and Tailwind CSS. The `tack` binary embeds the
built app and serves it from the same origin as the API.

## Tech Stack

- **Framework:** [SolidJS](https://www.solidjs.com/)
- **Build tool:** [Vite](https://vitejs.dev/)
- **Language:** [TypeScript](https://www.typescriptlang.org/)
- **Styling:** [Tailwind CSS v4](https://tailwindcss.com/), with the design tokens in `src/index.css`
- **Routing:** [@solidjs/router](https://github.com/solidjs/solid-router)
- **Icons:** [solid-icons](https://github.com/x64Bits/solid-icons)
- **Tests:** [Vitest](https://vitest.dev/) for units, [Playwright](https://playwright.dev/) for end to end

## Getting Started

### Prerequisites

- Node.js 22+ and npm
- The Tack API running (default: `http://127.0.0.1:3210`); start it with `cargo run -p tack-cli -- serve`

### Development

```bash
cd frontend
npm install
npm run dev
```

The dev server is at `http://localhost:5173` and proxies `/api` (REST and WebSocket) to the API,
so the app runs same-origin and needs no CORS. `VITE_PROXY_TARGET` points the proxy at another API.

### Build

```bash
npm run build     # type-check, then bundle into dist/
npm run preview   # serve the bundle locally
```

### Checks

```bash
npm run type-check
npm test              # vitest
npm run lint:tokens   # no raw Tailwind color literals; use the token utilities
npm run test:e2e      # Playwright; starts its own API and Vite (see docs/TESTING.md)
```

## Project Structure

```
frontend/src/
├── app/         # App shell, layouts and the route table (routes.tsx)
├── features/    # One folder per screen: projects, board, list, table, calendar, timeline,
│                #   sprints, dashboard, item-detail, templates, agents, settings
├── shared/      # What features share: api/, ui/, state/, execution/, runWithAgent/, vocab/, ...
├── test/        # Vitest setup
└── index.css    # Tailwind and the design tokens
```

A feature may import from `shared/` and never from another feature; `src/architecture.test.ts`
enforces it.

## API Client

`src/shared/api/` holds one module per area of the API (`projects.ts`, `items.ts`, `boards.ts`, ...)
over the fetch wrapper in `client.ts`; `index.ts` re-exports them. `schema.gen.ts` is generated from
`docs/openapi.json` (`npm run gen:api`, or `./scripts/regen-generated.sh` from the repo root) and is
never edited by hand. The API base is `/api`; `VITE_API_URL` overrides it for a build.

## Routing

The route table is `src/app/routes.tsx`. The top-level pages are `/projects`, `/templates`,
`/agents` and `/settings`; a project has `/projects/:id/overview`, `/factory` and `/settings`,
an item has `/projects/:id/items/:itemId`, and the work lenses (`board`, `list`, `table`,
`calendar`, `timeline`, `sprint`) sit under `/projects/:id/`.

## More

For the generated endpoint reference see [docs/book/src/developer/api-reference.md](../docs/book/src/developer/api-reference.md); for auth surfaces, WebSocket behavior and worked examples see [docs/API-REFERENCE.md](../docs/API-REFERENCE.md).
