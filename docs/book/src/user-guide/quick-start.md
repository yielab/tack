# Quick Start

Two ways to get Tack running: **install the binary** (the fast path — no build tools, you just want to use Tack) or **run from source in development mode** (for contributors and people who want hot reload). Pick the one that matches you.

Prefer pictures? The [Step-by-Step Tutorial](tutorial.md) walks this same page's install → project → tasks → agent-run path once, end to end, with a real screenshot at every step.

---

## Install

Tack is a single self-contained binary — the web UI, REST API, and SQLite engine are all inside one file. No runtime, database server, or container required. Pick any one method.

| Platform | Install → first agent attempt |
|---|---|
| Linux | `measured` |
| macOS | `not_measured` |
| Windows | `not_measured` |

**One line (Linux / macOS):**

```sh
curl -fsSL https://raw.githubusercontent.com/yielab/tack/main/install.sh | sh
```

Verifies the download against that release's `SHA256SUMS` and refuses to install on a mismatch. Pin a version with `TACK_VERSION=v0.1.0-beta.10`; choose the install directory with `TACK_INSTALL_DIR` (default `~/.local/bin`).

**Homebrew (macOS and Linux):**

```sh
brew install yielab/tap/tack
```

**Windows:**

```powershell
irm https://raw.githubusercontent.com/yielab/tack/main/install.ps1 | iex
```

Same `TACK_VERSION` and `TACK_SKIP_CHECKSUM` env vars as above, plus `TACK_INSTALL_DIR` (default `%LOCALAPPDATA%\Programs\tack`); it verifies `SHA256SUMS` and adds the install directory to your user `PATH` — open a new terminal afterwards. Or install with Scoop:

```sh
scoop install https://raw.githubusercontent.com/yielab/tack/main/packaging/scoop/tack.json
```

Or download the `.msi` from the [releases page](https://github.com/yielab/tack/releases).

**Desktop app.** Download it from the [releases page](https://github.com/yielab/tack/releases) — the `.AppImage` or `.deb` on Linux, the `.msi` on Windows, or the `.dmg` on macOS (built separately for Apple Silicon and Intel). Opening it starts the same server this page describes, inside its own window, with an icon in your system tray: closing the window leaves it running, and the tray's **Quit** is what actually stops it. If the server it started stops on its own, the tray tells you once — the status line reads "Server stopped" with the exit reason, and reopening the app starts it again; if the app is instead pointed at a server it did not start and that server goes quiet, the tray says so after a few seconds rather than staying silent. Skip to [First use](#first-use) once it's open.

**Docker:** the `ghcr.io/yielab/tack` image isn't publicly pullable yet — build it yourself from the repo's `Dockerfile` instead:

```sh
git clone https://github.com/yielab/tack.git && cd tack
docker build -t tack:latest .
docker run -d --name tack -p 3210:3210 -v tack-data:/data tack:latest
```

See [Deployment → Docker](../developer/deployment.md#docker) for the compose file and configuration.

**Manual download.** Grab the archive for your platform and `SHA256SUMS` from the [releases page](https://github.com/yielab/tack/releases), verify, then run:

```sh
# Linux / macOS
sha256sum -c --ignore-missing SHA256SUMS   # macOS: shasum -a 256 -c --ignore-missing SHA256SUMS
tar xzf tack-*.tar.gz && cd tack-*/
./tack
```

On **Windows**, extract the zip and double-click `tack.exe`.

**From source.** No `cargo install` path exists yet — see [Development mode (run from source)](#development-mode-run-from-source) below.

> **First-run note (unsigned binary).** The binaries are not code-signed yet. On macOS, right-click → **Open** the first time (or run `xattr -d com.apple.quarantine tack`). On Windows, click **More info → Run anyway** if SmartScreen appears.

## Start

```sh
tack            # starts the server + web UI at http://localhost:3210
```

Or start it with the embedded agent runner in one step — see [Run an item with an agent](#run-an-item-with-an-agent) below for what that adds:

```sh
tack serve --with-runner
```

Then open **`http://localhost:3210`** in your browser. On first start, Tack creates `tack.db` and a `storage/` folder next to the binary and runs the schema migrations automatically. Those two paths *are* your data — back them up and you've backed up everything.

**Verify the server is up:**

```sh
curl http://localhost:3210/api/health
# {"status":"ok","version":"0.1.0-beta.7","migrations_applied":62}
```

`migrations_applied` is how many migrations this build actually ran — trust that field over any number quoted here; it only grows as the schema does.

If this fails, see [Troubleshooting](troubleshooting.md).

---

## First use

1. **Create a project.** Click **New Project** on the Projects page, or press `Ctrl+K` and type "new project". Choose a **project type** — a template that pre-loads a matching [workflow](workflows.md) and [vocabulary](vocabulary.md) you can customize later. Then answer **Does this project have code?**:
   - **An existing folder on this computer** — pick the folder once. This is the choice if you want an agent to work on code you already have.
   - **A new folder Tack creates** — Tack makes the folder and starts a git repository in it.
   - **No code yet** — a plain project manager. You can change the answer later under **Settings → Automation** (see [Automation](agent-runners.md#automation)).

2. **Add an item.** On the Board, click the **+** inside any column, or the **+ New** toolbar button. Give it a title and press Enter. An *item* is the basic unit of work — a task, bug, building, assignment, or whatever your vocabulary calls it.

3. **Move it.** Drag the card to another column. Status changes save immediately with optimistic UI (the card moves before the server confirms).

4. **Open the detail drawer.** Click the card body (not the drag handle) to see all fields, dependencies, comments, attachments, and custom fields. See [Working with Items](items.md).

5. **Find your way around.** Press `Ctrl+K` for the [command palette](command-palette.md) (jump to any view, run an action) or `Ctrl+/` to search items. Switch theme and palette from the sidebar footer — see [Appearance](appearance.md).

Ready to put it on a network or add a token? See [Administration & Security](administration.md).

---

## Run an item with an agent

By default, nothing here executes anything: the server you started above is a full
project manager with agent execution off, and clicking an item's **Run with agent**
button shows "Agent execution is off" instead of a form. **Run with agent** (the
button) and `--with-runner` (a boot flag, further down) are two different things —
the button needs *some* runner active to do anything, and `--with-runner` is one way
to get one. The two paths below are alternatives, not sequential steps: pick the UI
path if a server is already running (no restart needed), or the CLI path if you're
starting fresh from a terminal.

**The UI-first path.** Three things, in order. None of them asks you for a provider, a
model name or a repository address.

1. **Turn agents on for this computer.** Open the **Agents** page from the sidebar
   (`/agents`) and click **Turn on**. This starts an embedded runner in this same process,
   on this loopback bind, with no restart and no second binary. The page then lists which
   agents are installed here (`claude`, `codex`, `opencode`, `docket`) and when each last
   ran on this computer. If an agent is installed and you are already signed in to it in
   your terminal, there is nothing else to set. The page is about this computer only; see
   [Agents on this computer](agent-runners.md#agents-on-this-computer). Everything about
   *your project* lives in the project's settings instead.
2. **Make sure the project knows where its code is.** If you answered **Does this project
   have code?** when you created it, this is done. Otherwise open **Settings → Automation**
   and pick the folder there. Tack chooses how the agent works on it: on a new branch of
   your repository if the folder is a git repository, in the folder directly if it is not
   (see [Where the agent works](agent-runners.md#where-the-agent-works)).
3. **Open a task and click Run with agent.** The dialog is one summary line (who runs it,
   in which profile, on which model, where), a section called **What the agent will read**,
   two switches (**Ask before each action** and **Allow network access**) and **Advanced**.
   The defaults are the recommended ones: the agent's default model and the **Implementer**
   profile. Click **Run**.

<img src="../screenshots/run-with-agent.png" width="60%" alt="The Run with agent dialog: a one-line summary of who runs it and where, the text the agent will read, two switches, and an Advanced section.">

When the run ends, the task's own Execution tab shows the attempt. If the agent changed
anything, the task is marked **Needs your review** until you press **Accept** or
**Reject** on the attempt. The attempt card has **Open diff**, **Open folder** (or Copy
path), **Copy branch**, **Result** and **Log** so you can check the work before deciding.
See [What happens when a run finishes](agent-runners.md#what-happens-when-a-run-finishes).

**The CLI path** — scriptable, and what to reach for outside a browser. Every step
below is a real command against a real server, copied from an actual run; the fewest
steps from the binary you already have to a completed attempt, no second process and
no operator-issued token.

Start the server with the embedded runner instead of plain `tack` (it self-provisions
on first start — see [Standalone mode](agent-runners.md#standalone-mode-tack-serve---with-runner);
this is the console-command equivalent of step 1 above, not a second thing to do on top
of it):

```sh
tack serve --with-runner
```

In another terminal, create an agent profile — its instructions travel with every
request created against it:

```sh
tack agent-profile create "release-notes" \
  --instructions "Summarize the diff and write docs/CHANGELOG entries."
```

```text
Created agent profile: release-notes (ap_91b4e)
  id: ap_91b4ea76-9f1a-4725-8a58-21a57d92572c
```

Find the runner id — the embedded runner enrolled itself under it:

```sh
curl -s http://127.0.0.1:3210/api/runners | jq -r '.data[].runner_id'
```

Using the item id from [First use](#first-use) above, create the execution request
(swap in your own harness — `codex` or `claude-code` — and whichever model
it accepts; `workspace_mode` says where the agent works — here a fresh clone, see
[Where the agent works](agent-runners.md#where-the-agent-works); see [Choosing a model and a
provider](agent-runners.md#choosing-a-model-and-a-provider) if unsure):

```sh
tack execution create <ITEM_ID> \
  --runner <RUNNER_ID> \
  --agent-profile ap_91b4ea76-9f1a-4725-8a58-21a57d92572c \
  --harness claude-code --model-provider anthropic --model-id claude-sonnet-4-5 \
  --agent-profile-snapshot '{"name":"release-notes","instructions":"Summarize the diff and write docs/CHANGELOG entries.","tool_policy":{},"timeout_seconds":600,"budgets":{}}' \
  --repository '{"kind":"git","remote":"/path/to/your/repo","base_revision":"<COMMIT_SHA>","subdirectory":null,"workspace_mode":"clone"}' \
  --permission-policy '{"tools":[],"network":false}' \
  --timeout-seconds 600
```

```text
Created execution request: exec_0fe
  state: queued
  id:    exec_0fe7252989f5f3d40a056c1da45b035039e4a8247ad89e5222cf9280134ec5d1
```

```sh
tack execution get exec_0fe7252989f5f3d40a056c1da45b035039e4a8247ad89e5222cf9280134ec5d1
```

```text
Execution request exec_0fe7252989f5f3d40a056c1da45b035039e4a8247ad89e5222cf9280134ec5d1
  state:   succeeded (done)
```

That is a completed attempt, reached either way. See [Running an item with an
agent](agent-runners.md#running-an-item-with-an-agent) for all four ways to create a
request and the full field-by-field reference.

---

## Development mode (run from source)

For contributing to Tack or running with hot reload. The API server and Vite dev server run as separate processes.

**Prerequisites:**

- **Rust toolchain** via [rustup](https://rustup.rs) (stable, 1.94+)
- **Node.js 22+** and npm

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
