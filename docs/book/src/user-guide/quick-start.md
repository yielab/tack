# Quick Start

Two ways to get Tack running: **install the binary** (the fast path — no build tools, you just want to use Tack) or **run from source in development mode** (for contributors and people who want hot reload). Pick the one that matches you.

---

## Install and run (the fast path)

Tack is a single self-contained binary — the web UI, REST API, and SQLite engine are all inside one file. No runtime, database server, or container required. Choose any one of the methods below.

**One line (Linux / macOS):**

```sh
curl -fsSL https://raw.githubusercontent.com/yielab/tack/main/install.sh | sh
tack            # starts the server + web UI at http://localhost:3210
```

**Download a release archive** from the [releases page](https://github.com/yielab/tack/releases):

```sh
# Linux / macOS
tar xzf tack-*.tar.gz && cd tack-*/
./tack
```

On **Windows**, extract the zip and double-click `tack.exe`.

Then open **`http://localhost:3210`** in your browser. On first start, Tack creates `tack.db` and a `storage/` folder next to the binary and runs all 18 database migrations automatically. Those two paths *are* your data — back them up and you've backed up everything.

> **First-run note (unsigned binary).** The binaries are not code-signed yet. On macOS, right-click → **Open** the first time (or run `xattr -d com.apple.quarantine tack`). On Windows, click **More info → Run anyway** if SmartScreen appears.

**Verify the server is up:**

```sh
curl http://localhost:3210/api/health
# {"status":"ok","version":"0.1.0-beta.6","migrations_applied":18}
```

If this fails, see [Troubleshooting](troubleshooting.md).

---

## First use

1. **Create a project.** Click **New Project** on the Projects page, or press `Ctrl+K` and type "new project". Choose a **project type** — a template that pre-loads a matching [workflow](workflows.md) and [vocabulary](vocabulary.md) you can customize later.

2. **Add an item.** On the Board, click the **+** inside any column, or the **+ New** toolbar button. Give it a title and press Enter. An *item* is the basic unit of work — a task, bug, building, assignment, or whatever your vocabulary calls it.

3. **Move it.** Drag the card to another column. Status changes save immediately with optimistic UI (the card moves before the server confirms).

4. **Open the detail drawer.** Click the card body (not the drag handle) to see all fields, dependencies, comments, attachments, and custom fields. See [Working with Items](items.md).

5. **Find your way around.** Press `Ctrl+K` for the [command palette](command-palette.md) (jump to any view, run an action) or `Ctrl+/` to search items. Switch theme and palette from the sidebar footer — see [Appearance](appearance.md).

Ready to put it on a network or add a token? See [Administration & Security](administration.md).

---

## Development mode (run from source)

For contributing to Tack or running with hot reload. The API server and Vite dev server run as separate processes.

**Prerequisites:**

- **Rust toolchain** via [rustup](https://rustup.rs) (stable, 1.75+)
- **Node.js 20+** and npm

```sh
rustc --version
node --version
```

**Terminal 1 — API server:**

```sh
git clone https://github.com/yielab/tack.git
cd tack
cargo run -p tack-cli -- serve
```

The server binds to `http://127.0.0.1:3210` and runs migrations on first start.

**Terminal 2 — frontend dev server:**

```sh
cd frontend
npm install
npm run dev
```

Vite starts at `http://localhost:5173` and proxies all `/api/*` requests to the API server. Open that URL in a browser.

### Build the single binary yourself

One process that serves both the API and the SPA — what the release archives ship:

```sh
# 1. Build the frontend
cd frontend && npm run build && cd ..

# 2. Build the API with the embedded SPA
cargo build --release --features embed-spa -p tack-cli

# 3. Run it
./target/release/tack
# open http://127.0.0.1:3210
```

Without `--features embed-spa` the binary serves only the API; use the Vite dev server or any static file host for the frontend.

---

## Environment variables

| Variable | Default | Description |
|---|---|---|
| `TACK_PORT` | `3210` | Listen port |
| `TACK_DATABASE_URL` | `sqlite:tack.db?mode=rwc` | SQLite file path |
| `TACK_LOG_LEVEL` | `info` | `trace` · `debug` · `info` · `warn` · `error` |
| `TACK_API_TOKEN` | _(none)_ | When set, all API calls need `Authorization: Bearer <token>` |

See [Configuration](configuration.md) for the full reference and `tack.toml` format, and [Administration & Security](administration.md) for tokens, CORS, webhooks, and cloud backup.
