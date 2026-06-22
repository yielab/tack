# Quick Start

Up and running in about five minutes.

## Prerequisites

- **Rust toolchain** via [rustup](https://rustup.rs) (stable, 1.77+)
- **Node.js 20+** and npm

```sh
rustc --version
node --version
```

## Development mode (two terminals)

The API server and Vite dev server run as separate processes. Start the API first.

**Terminal 1 — API server:**

```sh
git clone https://github.com/yielab/tack.git
cd Tack
cargo run -p tack-cli -- serve
```

On first start, the server creates `tack.db` and runs all 16 database migrations automatically. Binds to `http://127.0.0.1:3210`.

**Terminal 2 — frontend dev server:**

```sh
cd frontend
npm install
npm run dev
```

Vite starts at `http://localhost:5173` and proxies all `/api/*` requests to the API server. Open that URL in a browser.

**Verify the API is up:**

```sh
curl http://localhost:3210/api/health
# {"status":"ok","version":"0.1.0","migrations_applied":16}
```

## First use

1. **Create a project.** Click **New Project** on the Projects page, or press `Ctrl+K` and type "new project". Choose a project type — this pre-loads a matching workflow and vocabulary you can customize later.

2. **Add an item.** On the Board, click the **+** inside any column, or the **+ New** toolbar button. Give it a title and press Enter.

3. **Move it.** Drag the card to another column. Status changes save immediately with optimistic UI (the card moves before the server confirms).

4. **Open the detail drawer.** Click the card body (not the drag handle) to see all fields, dependencies, comments, attachments, and custom fields.

## Single-binary mode

One process that serves both the API and the SPA — useful for deployments and for sharing Tack on a LAN.

```sh
# 1. Build the frontend
cd frontend && npm run build && cd ..

# 2. Build the API with embedded SPA
cargo build --release --features embed-spa -p tack-cli

# 3. Run the single binary
./target/release/tack
# open http://127.0.0.1:3210
```

Without `--features embed-spa` the binary serves only the API; use the Vite dev server or any static file host for the frontend.

## Environment variables

| Variable | Default | Description |
|---|---|---|
| `TACK_PORT` | `3210` | Listen port |
| `TACK_DATABASE_URL` | `sqlite:tack.db?mode=rwc` | SQLite file path |
| `TACK_LOG_LEVEL` | `info` | `trace` · `debug` · `info` · `warn` · `error` |
| `TACK_API_TOKEN` | _(none)_ | When set, all API calls need `Authorization: Bearer <token>` |

See [Configuration](configuration.md) for the full reference and `tack.toml` format.
