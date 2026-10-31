# Quick Start

Install Tack, start it, create a project and add a task. Prefer pictures? The [Step-by-Step Tutorial](tutorial.md) walks the same path, through to an agent run, with a screenshot at every step.

---

## Install

Tack is a single self-contained binary — the web UI, REST API, and SQLite engine are all inside one file. No runtime, database server, or container required. Pick any one method.

**One line (Linux / macOS):**

```sh
TAG=$(curl -fsSL "https://api.github.com/repos/yielab/tack/releases?per_page=1" | sed -n 's/.*"tag_name": *"\([^"]*\)".*/\1/p' | head -n 1)
curl --proto '=https' --tlsv1.2 -fsSL "https://github.com/yielab/tack/releases/download/$TAG/install.sh" | TACK_VERSION="$TAG" sh
```

Verifies the download against that release's `SHA256SUMS` and refuses to install on a mismatch. The first line looks up the newest published release; set `TAG` to a release name to install another; choose the install directory with `TACK_INSTALL_DIR` (default `~/.local/bin`).

**Homebrew (macOS and Linux):**

```sh
brew install yielab/tap/tack
```

**Windows:**

```powershell
$tag = (irm "https://api.github.com/repos/yielab/tack/releases?per_page=1")[0].tag_name
$env:TACK_VERSION = $tag
irm "https://github.com/yielab/tack/releases/download/$tag/install.ps1" | iex
```

Set `TACK_VERSION` to choose a release and `TACK_INSTALL_DIR` to choose the install directory (default `%LOCALAPPDATA%\Programs\tack`). The script verifies `SHA256SUMS` and adds the install directory to your user `PATH` — open a new terminal afterwards. Or install with Scoop:

```sh
scoop install https://raw.githubusercontent.com/yielab/tack/main/packaging/scoop/tack.json
```

Or download the `.msi` from the [releases page](https://github.com/yielab/tack/releases).

**Desktop app.** Download it from the [releases page](https://github.com/yielab/tack/releases) — the `.AppImage` or `.deb` on Linux, the `.msi` on Windows, or the `.dmg` on macOS (built separately for Apple Silicon and Intel). Opening it starts the same server this page describes, inside its own window, with an icon in your system tray: closing the window leaves it running, and the tray's **Quit** is what actually stops it. If the server it started stops on its own, the tray tells you once — the status line reads "Server stopped" with the exit reason, and reopening the app starts it again; if the app is instead pointed at a server it did not start and that server goes quiet, the tray says so after a few seconds rather than staying silent. Once it's open, skip to [First use](#first-use).

**Docker:** build the image from the repository's `Dockerfile`:

```sh
git clone https://github.com/yielab/tack.git && cd tack
docker build -t tack:latest .
docker run -d --name tack -p 3210:3210 -v tack-data:/data tack:latest
```

See [Deployment → Docker](../admin-guide/deployment.md#docker) for the compose file and configuration.

**Manual download.** Grab the archive for your platform and `SHA256SUMS` from the [releases page](https://github.com/yielab/tack/releases), verify, then run:

```sh
# Linux / macOS
sha256sum -c --ignore-missing SHA256SUMS   # macOS: shasum -a 256 -c --ignore-missing SHA256SUMS
tar xzf tack-*.tar.gz && cd tack-*/
./tack
```

On **Windows**, extract the zip and double-click `tack.exe`.

**From source.** There is no `cargo install` path; see [Run from source](#run-from-source) below.

> **First-run note (unsigned binary).** The binaries are not code-signed. On macOS, right-click → **Open** the first time (or run `xattr -d com.apple.quarantine tack`). On Windows, click **More info → Run anyway** if SmartScreen appears.

## Start

```sh
tack            # starts the server + web UI at http://localhost:3210
```

Then open **`http://localhost:3210`** in your browser. On first start, Tack creates `tack.db` and a `storage/` folder in the folder you started it from, and runs the schema migrations automatically. Those two paths *are* your data — back them up and you've backed up everything.

**Verify the server is up:**

```sh
curl http://localhost:3210/api/health
# {"status":"ok","version":"…","migrations_applied":…}
```

`version` is the build you installed and `migrations_applied` is how many database migrations it has run; it only grows as Tack does.

If this fails, see [Troubleshooting](troubleshooting.md).

---

## First use

1. **Create a project.** Click **New Project** on the Projects page, or press `Ctrl+K` and type "new project". Choose a **project type** — a template that pre-loads a matching [workflow](workflows.md) and [vocabulary](vocabulary.md) you can customize later. Then answer **Does this project have code?**:
   - **An existing folder on this computer** — pick the folder once, by typing its path or, in the desktop app, with **Browse…** (in a browser tab the button is shown disabled, with "Available in the desktop app"). This is the choice if you want an agent to work on code you already have.
   - **A new folder Tack creates** — Tack makes the folder and starts a git repository in it.
   - **No code yet** — a plain project manager. You can change the answer later under **Settings → Automation** (see [Automating a Project](automation.md)).

2. **Add an item.** On the Board, click **Add item** in any column, or the **+ New** button in the top bar. Give it a title and create it. An *item* is the basic unit of work — a task, bug, building, assignment, or whatever your vocabulary calls it.

3. **Move it.** Drag the card to another column. The status saves at once; the card moves before the server confirms.

4. **Open the detail drawer.** Click the card body (not the drag handle) to see all fields, dependencies, comments, attachments, and custom fields. See [Working with Items](items.md).

5. **Find your way around.** Press `Ctrl+K` for the [command palette](command-palette.md) (jump to any view, run an action) or `Ctrl+/` to search items. Theme, palette, text size and zoom are in Settings → Display & accessibility — see [Appearance](appearance.md).

Ready to put it on a network or add a token? See [Administration & Security](../admin-guide/administration.md).

---

## Hand a task to an agent

Nothing here runs an agent until you turn one on: the server is a full project manager first, and **Run with agent** says "Agent execution is off" until then. Three steps, none of which asks for a model name or a repository address:

1. **Turn agents on for this computer.** Open **Agents** in the sidebar and click **Turn on**. It starts at once, with no restart. Sign in to the agent you want with the agent's own command (for example `claude`); the page tells you which agents are installed and whether each is signed in. See [Agents on this computer](running-agents.md#agents-on-this-computer).
2. **Tell the project where its code is.** If you answered **Does this project have code?** when you created it, this is done; otherwise open **Settings → Automation**. See [Where the agent works](automation.md#where-the-agent-works).
3. **Open a task and click Run with agent**, then **Run**. The defaults are the recommended ones. See [The Run with agent dialog](running-agents.md#the-run-with-agent-dialog).

<img src="../screenshots/run-with-agent.png" width="60%" alt="The Run with agent dialog: a one-line summary of who runs it and where, the text the agent will read, two switches, and an Advanced section.">

When the run ends, the task's **Execution** tab shows the attempt. If the agent changed anything, the task is marked **Needs your review** until you press **Accept** or **Reject**; see [Reviewing a Run](reviewing-runs.md). To start a run from a terminal instead, see [Running from the command line](running-agents.md#running-from-the-command-line).

To start the server with agents already on, run `tack serve --with-runner`; see [Standalone mode](../admin-guide/runners.md#standalone-mode-tack-serve---with-runner).

---

## Run from source

For contributing to Tack or running with hot reload. You need the Rust toolchain from [rustup](https://rustup.rs) and Node.js 22 or later.

```sh
git clone https://github.com/yielab/tack.git
cd tack
make dev      # API server on http://127.0.0.1:3210 and the Vite dev server on http://localhost:5173
make build    # the single binary, with the web UI embedded, in target/release/tack
```

Open the Vite address in a browser while developing; it forwards every `/api` request to the server. Setup, the project layout and the pull request process are in the [contributing guide](https://github.com/yielab/tack/blob/develop/CONTRIBUTING.md).

---

## Settings and environment

Tack reads its port, database location, log level and access token from settings you give the server when it starts. The defaults suit a single computer: the server listens on `127.0.0.1:3210` and makes its own access token, which your browser and the `tack` command pick up without being told. The [Configuration](../admin-guide/configuration.md) page lists every setting, and [Administration & Security](../admin-guide/administration.md) covers tokens, allowed origins, webhooks and cloud backup.
