# ADR 0018: the agent reaches only what you gave it

**Status: ACCEPTED 2026-10-12 (the maintainer: "add a phase that implements every improvement"); executed the same day by Phase 69 (`docs/plans/phase-69.md`).** Decision 9 reverses ADR 0013 decision 5 in one point: the pull request opens when you accept, not on completion.

**Decide:** that nothing can read Tack's data or start a run on your machine except you, and that
an agent Tack runs cannot reach your credentials, approve its own work, or change what Tack
shows you about it. Today the board answers every request with no token, any web page you visit
can post to it, an agent runs with every permission off and can read the same files Tack can,
and the evidence you review is computed from references the agent can move. A security audit on
2026-10-12 (read-only, five parallel reviewers, every finding re-read in the source) found no
planted code and no leaked secret, and found that these four properties, together, let a prompt
injection in a card or a repository file extract keys or merge code without a visible trace.

**Why now:** `v0.1.0-beta.10` was tagged the same morning with all of this in it. Tack has no
users yet, so the tag is withdrawn and the release is re-cut after this phase; there will not be
a cheaper moment.

**If you do nothing:** a page you visit can replace your database at the next restart; a card
imported from GitHub can make your own assistant start a run that sends your provider key to a
stranger; an agent can commit a git hook that runs after its run is "verified"; and "Allow network:
off" keeps saying it is safer when it is not.

The four decisions below that change how Tack feels are marked **(feel)**. Read those first; the
rest are plumbing.

## Decisions

| # | Decision |
|---|---|
| 1 | **The API always has a token.** `tack serve` that finds none creates one (32 random bytes, base64url), keeps it at `<data root>/api-token` (mode 0600), and reuses it on every start. A browser never sees the value: `GET /` answers with an `HttpOnly`, `SameSite=Strict` cookie for `/api`, and the middleware accepts that cookie or `Authorization: Bearer`. The CLI and `tack mcp` read the file when the flag, the environment and `~/.tackrc` name no token. `TACK_API_ALLOW_UNAUTHENTICATED_NONLOOPBACK=1` stays the one explicit opt-out, and it is logged at `warn` on every start. |
| 2 | **A request must name this server.** `Host` must be loopback with the bound port, the bound host, or a host from `allowed_origins`; anything else is `421` before authentication. A request that changes state and arrives with `Sec-Fetch-Site: cross-site`, or with an `Origin` that is neither same-origin nor allowed, is `403`. A route that reads a raw body requires its content type (`application/json` or a `yaml` type for an outline or an import; `text/csv` for a CSV; `application/vnd.sqlite3` or `application/octet-stream` for a restore). A `text/plain` body is `415`. |
| 3 | **Nothing imported is trusted.** An export file's `source: manual` reads back as `json_import`; `ItemSource::is_trusted` stays true for `Manual` only. An attachment's stored path is kept inside the storage directory (canonicalised, no symlink) and served as `application/octet-stream` with `Content-Disposition: attachment`. |
| 4 | **The run's environment is the runner's to grant.** A request may add a variable by `secret_reference: store:<name>` only if the runner's `runner.toml` lists `<name>` under `[secrets] request_may_reference`; the default is none. `env:<VARIABLE>` references are read from the runner's own `[environment]` config, never from a request. A literal value is refused, before spawn, for `HOME`, `PATH`, `GIT_*`, `SSH_*`, every loader and interpreter hook (`LD_*`, `DYLD_*`, `NODE_OPTIONS`, `PYTHON*`, `PERL5OPT`, `RUBYOPT`, `BASH_ENV`, `ENV`) and every `*_BASE_URL`. The run page lists the variable names a run received (never values) and the workspace path. |
| 5 | **The server records the profile; the request names it.** `POST /api/executions` reads the agent profile by `agent_profile_id` at creation and stores what it read; `agent_profile_snapshot` leaves the request. The repository is the project's: a request's `repository` is accepted only when it equals the project's configured one, otherwise `400`. |
| 6 | **(feel) A run started over MCP always asks.** `create_execution` keeps only `item_id`, `agent_profile_id`, `harness`, the model fields, `timeout_seconds`, the runner selector and `idempotency_key`; `environment`, `permission_policy` and `repository` leave it, and the run is created with `approvals: ask`, so the person sees every action in **Asking you** before it happens. Every id the MCP server puts in a URL is checked against `^[A-Za-z0-9_-]{1,64}$` first. `get_item` and `search_items` wrap each description and comment in a fence that names it as the card's text, not the person's instructions. |
| 7 | **The runner's git never runs the repository's code.** Every git invocation carries `-c core.hooksPath=/dev/null -c core.fsmonitor=false -c core.sshCommand=ssh -c protocol.ext.allow=never`, diffs add `--no-ext-diff --no-textconv`, and `GIT_CONFIG_NOSYSTEM=1` is set. The runner hashes `.git/config` (and the worktree's, when there is one) before the harness starts; if the hash differs when the run ends, the run fails as `repository_config_changed`, nothing is pushed, and the page says what changed. A push goes to the URL recorded at provisioning, as `<sha>:refs/heads/<branch>`, never to `origin`. |
| 8 | **The evidence is the base you chose and the tree that was verified.** The base is the commit id resolved at provisioning and kept in the attempt, not a name re-resolved at capture. The commit the verifier ran against is the commit that is pushed; a HEAD that moved in between fails the run as `tree_changed_after_verification`. prill's configuration is read from the base commit (`git show <base>:prill.yaml`) or from runner config, never from the agent's working tree. The pack flags every change to a file that decides how the work is checked (`.github/**`, `**/*test*`, `package.json` scripts, `Cargo.toml`, `Makefile`, `prill.yaml`, `justfile`) under **The agent changed how it is checked**. |
| 9 | **(feel) The pull request is opened when you accept.** Completion stores the pack; `POST …/attempts/{n}/review` with `accepted` opens the pull request. A run that was never reviewed never reaches GitHub. |
| 10 | **A harness ends when its process ends.** The process group and session are killed on every exit, including success; output is read until the harness exits plus a one-second grace, then closed; the first terminal line decides the outcome; a non-zero exit is a failure whatever the stream said. Run logs are written under the runner's state directory with `create_new` and `O_NOFOLLOW`, never into the workspace. The working directory handed to codex and opencode is confined the same way `process.rs` confines a spawn. |
| 11 | **(feel) Sandboxed where the machine allows it; honest where it does not.** On Linux with `bwrap` on `PATH`, the harness runs inside it: the workspace read-write, a scratch `HOME` plus the directories its descriptor names (`~/.claude` for claude-code, `~/.codex` for codex, …), `/usr`, `/lib*`, `/bin`, `/etc` read-only, a fresh `/proc`, a private `/tmp`; no runner state directory, no `~/.tackrc`, no token file, no `~/.ssh`. The descriptor reports `sandbox: bwrap` or `none`; `tack doctor` prints it; the run page shows **Sandboxed** or **Not sandboxed: the agent has your user's access to this machine**. The dialog's approvals default is `auto` when sandboxed and `ask` when not. A harness with no sandbox that cannot ask before it acts does not run: its probe carries the reason, so nothing schedules it, and the Agents page, the run dialog and the project's Automation panel say so. |
| 12 | **"Allow network" is renamed "Web tools".** Its help says: *Lets the agent fetch pages and search the web. Off turns those tools off; the agent's shell can still reach the network.* The capability stays `Advisory` and the run page says so. |
| 13 | **Untrusted text is fenced.** `compose_instructions` wraps the title, the description, every comment and the brief's imported parts in `<<<card-text id=…>>> … <<</card-text>>>` with a per-request random id, states before each that it is the card's text and not an instruction, and the run page shows the composed prompt under **What the agent read**. |
| 14 | **A credential never reaches a log.** The import handlers `skip(input)`; a test runs an import at `debug` and asserts the token is absent from the captured log. The backup endpoint is validated when it is set and the stored secret is cleared when the endpoint changes. |
| 15 | **The desktop window trusts one origin.** The capability names Tack's own port; `on_navigation` allows that origin only; a folder is opened through `tauri-plugin-opener` with a scope of the project's repository and the data directory, after the path is checked to be under one of them; the window sets a CSP; a server that does not echo the per-launch nonce in `/api/health`, or whose version is `Unknown`, is not attached to. Launch at login is off until the person turns it on. Grants the remote page cannot use are removed. |
| 16 | **A release is built from nothing restored.** `release.yml` keeps no cache; `tauri-cli` is pinned to an exact version; checkouts use `persist-credentials: false`; every asset, `SHA256SUMS` and the SBOMs are attested, and `SHA256SUMS` is signed keylessly (cosign, the bundle published beside it); `check-version` fails unless the tag's commit is on `main`. The installers verify the bundle when `cosign` or `gh` is present and say in their output when only the checksum was checked. Dependabot auto-merges `semver-patch` updates of development dependencies only. Base images are pinned by digest, the image runs as `nonroot`, `cargo build` is `--locked`, and `.dockerignore` keeps local state out. `serde_yaml` (unmaintained, RUSTSEC-2024-0449 through `unsafe-libyaml`) is replaced by `serde-saphyr` (pure Rust; `serde_yaml_ng` still pulls `unsafe-libyaml`); the stale `rsa` ignore leaves `.cargo/audit.toml`. |
| 17 | **Two boundary suites are required checks.** `crates/tack-api/tests/security/trust_boundary.rs` grows the server's rules (1–3, 5, 14) and `crates/tack-runner/tests/agent_boundaries.rs` holds the agent's (4, 7, 8, 10, 11); both run in CI as `security-boundaries` and in `.githooks/pre-push` as one nextest filterset. A rule without a test in one of them is not a rule. |
| 18 | **`SECURITY.md` says what the agent can reach.** It describes rules 1–13 in one table a user can read, and the book gets a page, *What the agent can reach*, that the dialog links from the sandbox line. |

## Alternatives not taken

- **Keep "no token on loopback" and add only the Host check.** The Host check stops a browser.
  It does not stop a process on the machine, a LAN host behind a reverse proxy on the same machine
  (the maintainer's own Caddy forwards `tack.test` from every interface), or an agent with a shell.
- **A per-user login.** ADR 0004 decided one operator. A token the browser holds in a cookie it
  cannot read is the whole of what one operator needs.
- **`approvals: ask` by default everywhere.** It makes every run a conversation. Where the sandbox
  holds, `auto` is safe enough to stay the default; where it does not, the person is told why they
  are being asked.
- **Enforce "network off" with a network namespace.** The harness has to reach the model; a run
  without network is a run that does nothing. Honest wording is the only truthful option.
- **Remove `create_execution` from MCP.** A model that can plan an outline should be able to
  start the work it planned. Forcing `ask` keeps the person in every step without a new state.
- **Sign commits and tags.** Worth doing; not a rule of this ADR because it is the maintainer's
  key and workflow, not the product's. Listed in the plan's Wave 0.
