# Roadmap

> **This file records intent, phase by phase. It is not the status board.** What actually
> shipped, and at which commit, is in the closed boards under
> [`docs/closed-cycles/boards/`](../../closed-cycles/boards/) — one board per cycle, each
> with its own card ownership and accepted integration SHAs. Where this file and a board
> disagree, the board is right.
>
> Phases 0–61 are delivered and merged on `develop`: the project-management core, the
> harness-agnostic runner fleet, the single-binary embedded runner, the adoption and
> distribution work, agent onboarding and provider choice, and the desktop app with its
> background service. The phase sections below are mostly closed history; each one states
> its own outcome at the top.

**Current plan: Phase 64 — a codebase for human maintainers** (ADR 0068, accepted
2026-09-18), at the bottom of this file. It follows Phase 63 and comes before the release
tag. Its harness stage has its own plan, `docs/plans/harnesses.md`.

**Status:** All engineering phases through 61 are delivered. The earlier ground: the
thirteen original engineering phases, then competitive and growth phases 20 (MCP server),
22 (dev-native CLI), 23 (Table view), 24 (positioning & presets) and 25 (local-first), then
the audit-driven phases 26–32, whose per-phase outcomes are in the status board below.

**Next cycle (added August 2026): Phases 33–38 — the Agent-Factory Control Center.**
Tack becomes the control panel for a factory of products built by
[docket](https://github.com/yielab/docket) agent fleets: a new `tack-orch` crate with
a `ControlPlane` trait and a pull-based reconciler, six new tables, dispatch from the
board, a fleet-wide approvals inbox, one-click product+pod provisioning, and
per-product unit economics. Executable task cards for parallel agents were in
`TODO.md`, now [`docs/closed-cycles/boards/part-1.md`](../../closed-cycles/boards/part-1.md);
the reciprocal docket-side work is Phase 22 of that project's `ROADMAP.md`. **That cycle is
complete** — all six phases shipped 2026-08-05.

**Historical cycle (partially implemented August 2026): Phases 39–49 — the Agnostic
Control Plane.**
Phases 33–38 built a control center against exactly one backend. This cycle makes it true
of any backend: capability negotiation so the UI disables what a provider cannot do and
names why, a `ControlPlane` trait with no docket nouns left in it, a **GitHub Actions
adapter** chosen because it shares none of docket's shape, an inbound telemetry channel
pushed from inside a run, per-item model choice owned by Tack, and the GitHub pipeline
finished in both directions (which closes Phase 21). Full plan with per-item verification
commands in [docs/closed-cycles/plans/agnostic-control-plane.md](../../closed-cycles/plans/agnostic-control-plane.md);
task cards in [`docs/closed-cycles/boards/part-2.md`](../../closed-cycles/boards/part-2.md).

**Status correction (2026-08-06):** Phases 39–42 exist in the current unreleased working
tree, but Phase 41's atomic-write acceptance and Phase 42's provider-scoped identity are
reopened. Phases 43–49 are frozen/superseded by the harness-agnostic runner plan appended
at the bottom of this roadmap. Their text is intentionally retained as design history.

---

## Archived

Phases 0–57 (Parts I–III) moved verbatim to
[`roadmap-phases-0-57.md`](../../closed-cycles/boards/roadmap-phases-0-57.md): the
audit-driven status board, `Completed` (Phases 0–19), four shipped phases from
`Planned` (20, 23, 24 and 25 — Phase 21 and 22 below are still open), the three "Next"
cycles for Phases 26–32, 33–38 and 39–49, and the Harness-Agnostic Runner Fleet chapter
(Phases 50–57).

- [Audit-driven cycle (Phases 26–32) — status board](../../closed-cycles/boards/roadmap-phases-0-57.md#audit-driven-cycle-phases-2632--status-board)
- [Completed](../../closed-cycles/boards/roadmap-phases-0-57.md#completed)
- [Next — Audit-Driven Phases 26–32](../../closed-cycles/boards/roadmap-phases-0-57.md#next--audit-driven-phases-2632-july-2026)
- [Next — Agent-Factory Control Center (Phases 33–38)](../../closed-cycles/boards/roadmap-phases-0-57.md#next--agent-factory-control-center-phases-3338-august-2026)
- [Next — Agnostic Control Plane (Phases 39–49)](../../closed-cycles/boards/roadmap-phases-0-57.md#next--agnostic-control-plane-phases-3949-august-2026)
- [Harness-Agnostic Runner Fleet (Phases 50–57)](../../closed-cycles/boards/roadmap-phases-0-57.md#harness-agnostic-runner-fleet-phases-5057)

---

## Planned

The product is feature-complete for the solo-dev / small-team use case. Phases 20–25
are **competitive / growth** work, driven by a deep competitive analysis of Tack
against the Rust and self-hosted PM ecosystem.

**Why these phases (the research, in brief):** No mature Rust-native, web-based,
self-hostable PM tool exists among the leading Jira alternatives — Tack is alone in
its niche. Competitors run other stacks (Plane/Django, Vikunja/Go, Huly/TypeScript,
OpenProject/Rails); the only _actually Rust_ rivals are hobby-scale terminal tools
(taskwarrior-tui, rust_kanban, kanbanban, fulsomenko/kanban). The verified gaps Tack
should close are: a **Table view** (Vikunja), **bi-directional GitHub sync** (Huly),
**AI-agent / MCP** support (Plane, Vibe Kanban), and **git-from-card** dev
conveniences (fulsomenko/kanban). Tack's differentiators to strengthen: the **single
~10 MB binary** (no Postgres/Docker-compose), **per-project vocabulary + domain
presets**, the **web + API + CLI triad**, and the **MIT license** (vs the AGPL/EPL
field). The Rust/self-host community values keyboard-first, offline/local-first,
plaintext, single-binary, low-memory tools — and criticizes heavy Docker-compose
deployments and bloated SPAs. The dominant 2025–26 trend is **AI agents as
first-class PM actors**, which makes Phase 20 the highest-leverage work.

Each phase below carries file paths and acceptance criteria so it can be picked up
cold. Three product decisions gate the dependent tasks — call them before dispatching:

1. **MCP transport** — stdio sidecar (`tack mcp`) vs an HTTP/SSE endpoint in `tack serve`? _(blocks Phase 20)_ — recommend **stdio sidecar** for v1.
2. **GitHub sync scope** — mirror status + comments + close-state, or status only for v1? _(scopes Phase 21)_
3. **Local-first** — is offline/CRDT sync in scope this cycle, or parked? _(gates Phase 25)_

> **Suggested parallelization:** Phase 22 (T1), Phase 24 (T1, T2), and the Phase 20
> decision have no dependencies and can start immediately. Phases 20 and 21 are each
> internally sequential; Phase 23 is a self-contained frontend track.

### Phase 21 — Bi-Directional GitHub Sync ⏳ _v1 push-only shipped; inbound + comments pending_

**Goal:** Upgrade the existing one-way GitHub _import_ into two-way _sync_ so item
status/comments/close-state mirror back to GitHub — closing the biggest "real work" gap
vs Huly. Scope per decision #2.

> **v1 shipped (push-only, status):** imported items are linked in a `github_links`
> table; completing a linked item closes its GitHub issue (reopening on the way back
> out) when `TACK_GITHUB_TOKEN` is set. Best-effort/fire-and-forget. Pieces:
> migration 018, `repo/github_links.rs`, `github_sync.rs` (`push_issue_state` +
> `state_change`), the `maybe_sync_github` hook in `handlers/items.rs`, and import
> linking. `TACK_GITHUB_API_BASE` makes import+push testable/Enterprise-ready. Tests:
> wiremock push (3), link round-trip, and a full import→complete→close integration
> test — all against a **mocked** GitHub (no real repo touched). Decision recorded in
> [docs/GITHUB-SYNC.md](../../GITHUB-SYNC.md).
>
> **Still pending (future slices):** Task 1 comments-mirroring, Task 4 inbound sync
> (webhook/poll), per-project tokens + manual item linking, and a real-repo live test.
> The task notes below describe the _full_ bi-directional vision.
>
> **Scheduled:** the remaining slices are **Phases 47 and 49** of the Agnostic
> Control Plane cycle. Phase 47 builds the inbound webhook receiver (signature
> verification, delivery dedupe, echo suppression) that Task 4 describes; Phase 49 adds
> comment/label mirroring, per-project credentials, PR and check-run state, and merge
> evidence. Task 2's "backfill links for previously imported items" is covered by
> migration 049's reverse index.

#### Task 1 — Spec the sync model

Write `docs/GITHUB-SYNC.md`: link storage (item ↔ GitHub issue number + repo), conflict
policy (last-write-wins vs GitHub-authoritative), v1 scope (recommend: status +
close-state + comments outbound; inbound via webhook or poll), and push trigger (reuse
the webhook dispatch path in [webhook.rs](../../../crates/tack-api/src/webhook.rs) vs a
periodic reconciler). **Blocks the rest of Phase 21.**

#### Task 2 — Persist the issue↔item link

Add migration #18 in
[tack-db/src/migrations.rs](../../../crates/tack-db/src/migrations.rs) — an
`external_links` table (or `gh_repo` / `gh_issue_number` / `gh_synced_at` columns on
items) — plus a repo module under `crates/tack-db/src/repo/`, following the "Adding a
New Entity" pattern. Backfill links for previously imported items.

#### Task 3 — Outbound push

On a linked item's update, PATCH the GitHub issue (state, optionally labels) and POST
mirrored comments via the GitHub REST API using the stored PAT. Hook into the item
update path in `handlers/items.rs`; new logic in `handlers/import_github.rs` (or a new
`github_sync.rs`). Rate-limit aware; best-effort with logged failures.

#### Task 4 — Inbound sync

Implement either `POST /api/projects/{id}/github/webhook` (verify
`X-Hub-Signature-256`) or a poll-on-interval reconciler, per Task 1. Apply remote
changes through `tack-core` so workflow rules hold. Register the route in
[router.rs](../../../crates/tack-api/src/router.rs).

#### Acceptance criteria

- Moving a linked item to a Done status closes the GitHub issue; adding a comment mirrors
  it (test with a mocked GitHub endpoint).
- Closing an issue on GitHub moves the linked Tack item to Done (test with a captured
  webhook payload fixture).
- Migration runs clean on startup; `cargo test --workspace` and `cargo clippy` pass.

### Phase 22 — Dev-Native CLI (git-from-card)

**Goal:** Make the CLI the surface Rust devs love — cheap, high-signal, and a praised
feature in the Rust kanban field (mirrors fulsomenko/kanban).

#### Task 1 — `tack branch <item-id>` ✅ _done_

Fetch the item via `client.rs`; slugify `type/id-short-title` (e.g.
`feat/a1b2-add-table-view`). Print `git checkout -b <branch>` by default; run it with
`--checkout`; `--json` outputs the name. New `crates/tack-cli/src/git.rs` +
subcommand in `main.rs`. Respect a configurable prefix template. Unit-test the slug
function.

> Shipped: [git.rs](../../../crates/tack-cli/src/git.rs) (`slugify`, `type_prefix`,
> `branch_name`, 8 unit tests) + the `Branch` subcommand and `cmd_branch` in
> `main.rs`. `--prefix` overrides the type-derived prefix (feature→feat, bug→fix,
> …). Documented in the [CLI reference](user-guide/cli.md).

#### Task 2 — `tack open` / smart start

`tack open <id>` opens the item's web URL in `$BROWSER`. Optional `tack start <id>` =
move to the first in-progress status + `tack branch --checkout` in one step. Cover with
CLI help + a smoke test.

#### Acceptance criteria

- `tack branch <id>` prints a sane branch name; `--checkout` creates+switches; `--json`
  works. `cargo test` green.

---

## Known Gaps

> **This table is the Phases 26–32 audit snapshot, not a current gap list.** Each row records
> what that audit found; the "Tracked in" column names the phase that took the work on, and
> those phases are marked done above. Rows verified closed as of 2026-08-14 are annotated
> inline. Read the owning phase's own section for what actually shipped versus what it
> deferred — several closed only partially. Current Part III gaps live in the Part III section
> below and on `TODO.md`'s board, which is the authority.

| Area | Gap | Tracked in |
|---|---|---|
| Item updates | `sprint_id` / `due_date` / `estimate_unit` not persisted by `update_item`; `started_at`/`completed_at` never set on ordinary status moves | Phase 26 (blocker) |
| Security | Alexa endpoint lacks Amazon cert-chain validation; no warning on unauthenticated non-loopback bind; tar-slip + unverified sha256 in backup restore; S3 secret embedded in backup bundles | Phase 27 |
| Backup as sync | No conflict detection between installs; scheduler ignores UI-saved settings; non-atomic restore swap | Phase 28 |
| API contract | ~~No OpenAPI spec; hand-maintained TS types and API docs have drifted from the router~~ **closed** — `docs/openapi.json` (90 paths) is generated from the code, `frontend/src/shared/api/schema.gen.ts` is generated from it, and CI fails on drift. "Two error JSON shapes" **still holds and is now deliberate**: `ErrorEnvelope` for operator routes, `RunnerV1ErrorEnvelope` for the runner protocol — two separate auth surfaces, not an oversight | Phase 29 |
| Vocabulary | Global "+ New" modal, Sprints view, tabs/palette/first-run guide hardcode "sprint"/"Story Points" | Phase 30 |
| Coverage reporting | ~~168 Vitest unit tests and a Playwright E2E suite ship; automated coverage thresholds in CI are not yet enforced~~ **closed** — CI's `coverage` job enforces `cargo llvm-cov --fail-under-lines` per crate (tack-core 85, tack-db/tack-api/tack-orch 70) alongside Vitest thresholds; the frontend suite is now 724 tests across 85 files | Phase 32 |
| Release integrity | No checksums/SBOM/provenance on release assets; SECURITY.md routes disclosure through public issues | Phase 32 |
| Custom field validation | `validation` rules enforced (pattern, min/max, min/max_length, max_items); full JSON Schema not supported | Future |
| Auth | No multi-user auth (by design for v1) | Future |

---

## Contributing

See [CONTRIBUTING.md](../../../CONTRIBUTING.md) for code style, PR process, and how to add new
features. The [Adding Features](developer/adding-features.md) guide walks through the
three most common extension patterns.

---

# Standalone Single-Binary Operation (Phase 58)

**Status:** delivered. Decided in
[`docs/adr/0058-standalone-single-binary-runner.md`](../../adr/0058-standalone-single-binary-runner.md).
Execution is tracked card-by-card on the **Part IV board** in `TODO.md` (§IV.0–§IV.6), which is
the authority for wave status, card ownership and accepted integration SHAs. This section
records architectural intent; the board records what actually shipped.

**Does not depend on Phase 57's release.** The two are independent: Phase 57 is about proving
and tagging the fleet, Phase 58 is about how the product is packaged and first run.

## Why this phase exists

Tack's stated principle is a single `tack` binary. The runner fleet, correctly, ships as a
second one. For a fleet of remote runners that is the right shape — but for the most common
case, one developer on one machine, it means: build or install a second binary, create a
pending runner, copy a one-time token out of terminal output, export three environment
variables, and keep a second process alive. Four manual steps and two artifacts before the
product does the thing it exists to do.

The feedback that opened this phase was blunt: the split is "totalmente antiintuitivo" and
"muy fraccionado para la mayoría de usos" against "su principio final de un solo binario".

## The observation that resolves it

**ADR 0050 separates roles, not binaries.** Its rules are about who schedules work, who owns
processes, who holds credentials, and which direction connections open. None of them say
anything about how many executables ship. The `tack` binary already hosts two distinct roles —
HTTP server and API client — chosen by subcommand. A third is consistent with that design
rather than a departure from it.

## Product outcome

One command, `tack serve --with-runner`, takes a machine with one harness installed and no
prior Tack state to a completed agent attempt visible in the UI. No second binary. No pending
runner to create. No one-time token to copy.

Distributed operation is unchanged: a fleet of remote runners works exactly as before, through
the same protocol and the same client, and `tack-runner` remains shippable on a machine that
has no server.

## Non-negotiable rules for this phase

- **The embedded runner speaks runner-v1 over loopback HTTP, like any remote runner.** No
  in-process handler calls, no shared `AppState`, no privileged path. There stays exactly one
  protocol-client implementation and one server-side path serving it. An in-process shortcut
  would be a second path free to drift from `docs/contracts/runner-v1/` — which the fixtures
  exist to prevent — and a path `scripts/smoke.sh` does not exercise, making the mode most
  users run the mode least proven.
- **`tack-api` never depends on `tack-runner`.** The composition root is `tack-cli`, which
  already depends on `tack-api`. ADR 0050's "the Tack API never starts a coding harness" stays
  literally true of the API crate.
- **Off by default; loud on failure.** An embedded runner executes arbitrary coding-agent
  processes on the host serving the UI — strictly more dangerous than anything Tack currently
  gates. It is opt-in (`--with-runner` / `TACK_LOCAL_RUNNER_ENABLE`), and a server that keeps
  running after its runner failed to start is indistinguishable from a scheduler bug, so that
  is an error rather than a log line.
- **Auto-enrollment is refused on any non-loopback bind.** Tack served to a team on `0.0.0.0`
  plus a self-enrolling agent executor is a remote-code-execution surface, not a convenience.
  Refusal is a startup error, never a silent downgrade.
- **Vendor credentials stay outside Tack.** Provider keys (Anthropic, OpenAI, OpenRouter, a
  local endpoint) live in each harness's own environment on the machine running the runner
  role. A Tack model gateway remains an explicit non-goal — see *Explicitly deferred* above,
  unchanged.
- **Packaging only.** No change to the runner-v1 contract, the scheduler, fleets, migrations,
  the operator API or the frontend.

## Scope

- Extract the runner's composition root into a reusable library entry point so the standalone
  binary and any embedder share one wiring rather than two that drift.
- Add a readiness and bound-address signal to the server so an embedder can start a runner
  against the real address once the listener accepts, without polling a guessed URL.
- `tack runner start` — the runner role in the `tack` binary.
- `tack serve --with-runner` — both roles in one supervised process, gated and loopback-checked.
- Zero-touch local enrollment: self-provision a pending runner in-process, then redeem the
  one-time token over loopback HTTP through the ordinary protocol path, storing the durable
  credential owner-only. Hashes-only storage server-side is unchanged.
- `tack runner doctor` — what this machine can run, what each harness declares, and where its
  model credentials come from. This closes a real reported gap: today that information exists
  only inside a capability snapshot posted to a server, so "how do I configure Claude, Codex,
  OpenRouter or a local model" has no local answer.
- A standalone smoke step that can genuinely fail, plus the configuration and provider-
  credential documentation `docs/CONFIG.md` currently lacks entirely.

## Explicitly deferred within this phase

- `tack.toml` support for the enable gate. That file is `tack-api`'s configuration surface and
  the gate belongs to `tack-cli`; a second reader of one file is not needed to deliver this.
- Any change to how models are selected, resolved or validated. `model_profiles` (migration
  043) remains consulted by nothing — a standing finding since Phase 56, untouched here.

## Exit

On a machine with one harness installed and no prior state, `tack serve --with-runner` is the
only command needed to reach a completed attempt. The embedded runner is off unless enabled,
refuses to auto-enroll on a non-loopback bind, and shares one code path with remote runners.
Remote fleets are unaffected. `runner_contract`, `wave2_gate` and `openapi_contract` are
unchanged and drift-free, because nothing in this phase may touch what they pin. Binary-size
growth is measured and recorded as a real number, never estimated.

---

# Adoption & First Public Release (Phase 59)

**Status:** every card delivered; what remains is publishing, which is a human action
outside this repository and is listed in `docs/LAUNCH-CHECKLIST.md`. Opened by an
**adoption audit**, whose
findings are consolidated below. Execution is tracked card-by-card on the **Part V board**
in `TODO.md` (§V.0–§V.6), which is the authority for wave status, card ownership and
accepted integration SHAs. This section records the audit and the intent; the board records
what actually shipped.

**Independent of Phase 58** except for one card: the demo (V-C2) needs `tack serve
--with-runner` so the recording is one command rather than four steps and a copied token.

## Why this phase exists

Tack has been a public repository since 2026-03-15. On 2026-08-30 it had **zero stars, zero
forks, zero human-filed issues and one human contributor**. Every open pull request was from
Dependabot.

That is not a verdict on the code. The same audit measured ~122k lines of Rust across six
crates, ~45k lines of SolidJS, 1380 passing workspace tests, 57 migrations, a 92-path
documented API, a ~113 ms cold start at ~11.7 MiB resident, and durable execution semantics
— leases, fencing tokens, replay tables, recovery audits — that no competitor in the
category has. It is a verdict on the fact that **the project has never been released,
published, positioned, or shown to anybody.**

The preceding eight phases asked "does it work?". This one asks "can a stranger get it, run
it, and understand what it is?" — and today the answer is no, for reasons that are mostly
not code.

## What the audit found

### The install path advertised to the public does not work

`README.md` prints, as the headline install method:

```bash
curl -fsSL https://raw.githubusercontent.com/yielab/tack/main/install.sh | sh
```

**There is no `main` branch.** The repository's default branch is `develop`;
`git ls-remote --heads origin main` returns empty, and the URL has returned **404** since the
repository was made public. The script itself is fine — served from `develop` it returns 200.
Only the path is wrong, and `cargo install --git` masked it locally because cargo follows the
default branch. Card **V-A1**.

### The differentiator has never been downloadable

The only release is `v0.1.0-beta.6`, cut **2026-06-22**, whose four assets are `tack-*`
archives only. Everything that makes Tack distinct — the durable execution domain, the
`tack-runner` binary, harness adapters, fleet scheduling, decisions, artifacts, model
profiles: Phases 50 through 58 — postdates it entirely. A visitor who follows the README
today downloads a Tack with no runner fleet at all.

**A correction the audit made to itself, recorded so it is not repeated:** the first reading
was that CI does not package the runner. That is **false**. `release.yml` has built
`tack-runner` with `cargo auditable` and packaged it into its own per-platform archive, with
the systemd unit, since `7d78de3` on 2026-08-19. The gap is that **no tag has been cut
since**. Card **V-A3** cuts one; it does not fix CI.

### The live smoke fails, and Phase 57's blocker was misdiagnosed

Phase 57's tag was believed to be one `codex` installation away from green. It was installed
on 2026-08-26 — three of three harnesses present for the first time — and
`./scripts/smoke.sh --live` returned **`SMOKE FAILED`** for a different reason: a live
attempt that never reached a terminal state, and a step-8 failure message that had been
recorded as stale six days earlier and never reworded, which then misled two readers into
seeing a regression that was not there. Card **V-A2** owns the four open questions the Wave 9
amendment left. Codex has still never completed a live attempt.

### The positioning names the category where Tack loses

The repository description reads *"Self-hosted project manager in a single binary — no
Docker, no database server."* It does not mention agents. That sentence enters Tack in the
category of Plane (~54.6k ★), Huly (~26.9k ★), Focalboard (~26.3k ★), Leantime (~9.4k ★) and
Vikunja (~3.8–5k ★) — mature tools with large communities — while omitting the one capability
none of them has. The README's opening line then describes two products at once and assumes
the reader knows what a harness is. Meanwhile the mdBook under `docs/book/` is good and
**unpublished**: `yielab.github.io/tack` is a 404 and `homepageUrl` is empty. Card **V-A4**.

### The identity model is undeclared rather than decided

There is no users table, no sessions and no per-user permissions. `assignee` is a free-text
column; `roles` is a per-project colour label attached to items, not an identity.
Authorization is one shared bearer token, and when no token is configured `require_token`
allows everything by design, for pure-local mode.

That is a defensible design for a single operator. It is not "self-hosted for a team", and
the documentation does not distinguish them — a reader who sees "self-hosted" reasonably
assumes accounts exist. The posture also aged badly: Phase 27.2 made a non-loopback bind with
no token a **warning**, which was proportionate when the product stored task text and is not
now that the same server schedules agents that execute code. ADR 0058 already chose a startup
**error** for that shape of risk. Card **V-B1** decides and records the posture; it does not
build identity.

### Two execution models coexist in the schema and the UI

Parts I and II built a complete control plane against exactly one backend (docket): a
`ControlPlane` trait, a reconciler, an adapter, 9 `orch_*`-prefixed tables plus a
`control_planes` table (10 Docket-specific tables total), an Approvals inbox and a
ControlPlanesManager. Part III replaced that model with the native pull-based runner and
kept docket as an optional legacy bridge. Both now sit side by side, and a reader cannot
tell which "fleet" is current.

The cost of removing it is not trivial and the audit will not pretend otherwise: **234 Rust
doc comments cite `TODO.md` section numbers** from those cycles, and the docket half of
`tack-orch` is interleaved with the runner-v1 execution domain that Part III depends on. Card
**V-B2** measures the surface, writes the ADR, and gates — deletion, if chosen, is a later
card that ADR authorizes.

## The competitive situation, and why the timing is the argument

The nearest thing to Tack that ever existed — **Vibe Kanban**, a kanban board orchestrating
Claude Code, Codex and Gemini agents — **shut down on 2026-04-10** when its company (Bloop)
closed. Its farewell states it had thousands of engineers using it daily and never found a
business model. **Crystal**, the other popular open-source orchestrator, was **deprecated in
February 2026**.

What remains above Tack in that category is closed-source (Conductor — macOS-only, $22M
Series A; Sculptor — Claude-only) or is not a board at all (OpenHands, Emdash). What remains
in the project-management category executes nothing.

**Tack is the only thing that is credibly both**, and there is an identifiable,
currently-orphaned audience for exactly that. Emdash (YC W26) is collecting it now. The
category's history carries one warning worth stating plainly: it killed its leader through
failure to monetize, not failure to be useful — which for a project with no company behind it
is an advantage rather than a risk.

## What this phase deliberately does not do

No card in Phase 59 adds a product feature. Three cards remove or gate; the rest are
packaging, proof and truthfulness. The real feature gaps the audit found are recorded here
and scheduled after adoption, not before it:

- **Outbound notifications / SMTP** — zero references in the tree today; the only webhooks
  are inbound from GitHub.
- **Internationalization** — the UI is English-only with no locale infrastructure at all.
- **Time tracking** — `estimate` exists; `time_spent` does not.
- **In-UI diff review of agent artifacts** — the step where a human actually decides.
- **A harness that asks a mid-run question** — the `decisions` path is built, contract-pinned
  and has never been exercised, because no harness in this tree asks anything. It remains a
  documented scope limit rather than a defect.
- **Multi-user accounts** — gated behind V-B1's posture decision.

The ten project-type presets (construction, legal, homework, events) are a live positioning
question: they pull the story toward generic project management, which is the losing
category. V-A4 records the trade-off for a human decision and changes no preset code.

## Exit

A stranger with no prior knowledge can find Tack, understand in fifteen lines what it is and
what it does not do, install it with the command the README prints, run one command to reach
a completed agent attempt, and watch a sixty-second recording of durable recovery that no
competitor can produce. Every capability claim on the first screen traces to a proof. The
identity posture is written down rather than inferred. Nothing is published without explicit
human approval.

---

# Agent Onboarding & Provider UX (Phase 60)

**Status:** every card delivered. Opened by an **agent-UX audit**, whose
findings are consolidated below. Execution is tracked card-by-card on the **Part VI board**
in `TODO.md` (§VI.0–§VI.6), which is the authority for wave status, card ownership and
accepted integration SHAs. This section records the audit and the intent; the board records
what actually shipped. The dispatch plan — per-card read lists sized for a 200k-context
agent, gates, stop conditions, and the per-wave integrator checklist — is
`docs/agent-handoffs/part-vi/README.md`.

**Runs alongside Phase 59's last wave.** It needs Phase 58 (done — the embedded runner is
what makes a UI-only path possible at all) and ADR 0060 (docket stays and is not touched).
It shares `README.md` and `docs/screenshots/` with Phase 59's V-C2/V-C3; the rule is in
`TODO.md` §VI.3. VI-A3 restructures the README that V-A4 wrote; V-A4's four questions and
its claim → evidence rule carry over unchanged.

## Why this phase exists

Phase 58 made one command reach a completed attempt. Phase 59 makes the project findable and
its claims honest. Neither answers the question a user asks in the first minute — *which
model, from which provider, and where do I put the key?* — and today the product answers it
with a negation: `docs/CONFIG.md` says there is *no* `TACK_*` variable for a model provider,
by design. That is true of credentials, and every reader takes it to mean they cannot choose
a model, while the request body, the CLI and the modal all route exactly that choice.

The deeper defect is structural. The steps between an installed binary and a completed
attempt are spread across **three surfaces** — a harness's own vendor login in a terminal,
the `tack` CLI, and the web UI — and no screen or page shows the whole path. The web UI can
run an item but asks for a runner id, a git remote, a commit and a timeout by hand on every
run. The CLI has the full command tree and the CLI reference documents none of it. The
model-selection precedence that the scheduler implements exists only as a Rust doc comment.
Someone who did not build this cannot use it, and a stranger who installs beta.7 — the
release Phase 59 cut precisely so a stranger *could* — will find that out in the first ten
minutes.

The product's posture — Tack never proxies a model and never holds a vendor login — is
right and stays. This phase keeps it and moves the **guidance** into the product: one
screen that owns the path from an installed binary to a completed attempt, a project-level
answer to "which model", and one provider — Vercel AI Gateway — through which a user who
only wants the UI can authenticate with a single pasted key and choose among every model
the gateway serves, from a list the runner actually measured.

## What the audit found

### Three surfaces and no path

`docs/API-REFERENCE.md` delegates the execution surface to the agent-runners guide; the
guide has 433 lines on enrolling runners, credentials, capability matrices and recovery, and
**zero** examples of creating an execution. The only enumeration of the request's thirteen
required fields is a table row in `docs/MCP.md`. The Quick Start and *Working with Items*
pages do not contain the word "agent". The CLI reference omits `tack execution`, `tack
runner` (including `doctor`, which `docs/CONFIG.md` tells the reader to run), `tack fleet`,
`tack agent-profile` and `tack model-profile`. Two pages each claim to be the configuration
reference and list different variables. Cards **VI-A1** (now, against the product as it is)
and **VI-D1** (after the product changes).

### The provider answer is a negation

ADR 0050 keeps the API server out of the model-calling business; ADR 0058 says "vendor
credentials remain outside Tack". Both are statements about the API server and both are
correct. Both are cited — in `docs/CONFIG.md`, in `tack runner doctor`'s own output — as if
they meant "Tack cannot help you configure a provider". The runner side of the boundary was
never decided: the runner already owns its own credential, owns the harness subprocess and
its environment, and accepts `secret_reference` environment entries that every adapter warns
about and skips because "no secret-store client exists in tack-runner yet". Card **VI-A2**
writes ADR 0061 (see `docs/adr/0061-provider-credentials-at-the-runner-boundary.md`,
accepted 2026-09-03) and decides that side; cards **VI-B1** through **VI-B3** implement it.

### The docs contradict the code in four places

The harness id the guide prints (`claude_code`) is not the wire id (`claude-code`); a request
built from the page fails. The guide says model profiles have "no runtime effect"; the modal
copies the chosen profile into the request's highest-precedence tier. The guide says fleet
membership has no write route; the route exists and the Fleet panel says nothing calls it.
Two of the three "known gaps" listed are no longer gaps, which costs the third — the real one,
no list route for artifacts or decisions — its credibility. **VI-A1** fixes the first three;
**VI-C4** closes the fourth.

### The modal asks for what the product should already know

Five hand-typed fields per run, no memory between runs, a free-text runner id when
`GET /api/runners` exists, a fleet selector with no way to add a member from the UI, and no
project-level storage for a repository at all — the only repository link in the schema is
per-item `github_links`. **VI-C3** gives the project the three facts an attempt needs;
**VI-C2** makes the modal read them.

### The tiers exist; the storage and the UI do not

`resolve_model_policy` walks four tiers — request override, agent-profile default, project
default, fleet default — with an exhaustive test over every presence combination. The
project tier has no storage and always resolves to nothing; the agent-profile and fleet
tiers are JSON conventions inside `limits` and `default_policy` with no field in any panel.
The mechanism is finished and nobody can reach it. **VI-C3**.

### The README shows a project manager

The hero asset's own alt text is *"Board, Timeline, and vocabulary editor"*. The *Features*
section lists project management first and agent execution second. All five screenshots —
board, timeline, dashboard, list, vocabulary editor — are project-management views, and
**none** shows an agent doing anything. The two-component architecture is explained at line
169, after *Status*. The book's introduction lists four core concepts — item, workflow,
project type, vocabulary — with neither *runner* nor *run* among them. V-A4's first sentence
was right about the *what*; the shape of the page still says the opposite, and a reader who
arrives from the category Tack is trying to leave sees exactly that category. Card **VI-A3**
restructures the README, the introduction and the developer overview around one statement
and one diagram; card **VI-D2** makes the assets that show the execution plane, once there
is an honest screen to record.

## The surface map

The design authority for the phase, reproduced from the board. Every step from an installed
binary to a completed attempt, where it happens today, where it happens after this phase,
and — when the answer is not "the UI" — the structural reason, so the question is settled
once.

| Step | Today | Target | Why not fully UI |
|---|---|---|---|
| Turn on agent execution | console flag | **UI** — one switch on a loopback bind; the command only where the switch cannot exist | — starting Tack itself is the one command left |
| Install a harness binary | outside Tack | console, rendered in the UI per harness | external binaries |
| Authenticate a harness with its vendor login | outside Tack | console, rendered in the UI with a re-check | OAuth device flows need a TTY; Tack never holds them |
| Authenticate through Vercel AI Gateway | impossible | **UI** with the embedded runner; console for a remote runner, rendered in the UI | none in the embedded case |
| Enroll a runner | zero-touch / token → console | unchanged | — |
| Create a fleet, add a member | UI / API only | UI | — |
| Create an agent profile | UI | UI, default created on first use | — |
| Choose a default model | impossible | UI, from a measured catalog | — |
| Run an item | UI, five typed fields | UI, zero typed identifiers | — |
| See what an attempt produced, answer a decision | UI with a typed id | UI lists | — |

The last column is closed. A step that cannot move to the UI for a reason not in this table
is an amendment to ADR 0061, not a card's judgement.

## Vercel AI Gateway, and why it is the one provider this phase adds

The harnesses' own logins are the first case and already work. The gateway is the second
case, and the only one worth building now, because it is simultaneously: a **single API
key** rather than a vendor OAuth flow; documented by its vendor with a **dedicated endpoint
for each harness** in this tree (`/claude-code`, `/codex/v1` — fetched 2026-09-03, to be
re-fetched before any card relies on it); and a **catalog endpoint**, so the model picker
shows what a runner measured rather than a list someone typed. That combination is the
only route to "a UI-only user authenticates without a console".

It is added as a **provider configuration at the runner** — the key lives in the runner's
owner-only state directory next to its own credential, the runner's probe fetches the
catalog into the capability snapshot the scheduler already intersects against, and the
adapters inject the vendor-documented environment only for attempts whose resolved model
names the gateway. The Tack API still makes zero model-provider calls. The single exception
— a loopback-only, embedded-runner-only, write-once route that hands a pasted key to the
co-located runner without persisting it — is what ADR 0061 exists to bound. OpenRouter,
direct vendor keys and local endpoints stay in each harness's own configuration until the
gateway path is proven live and a second gateway measurably differs from it.

## Credentials: who runs what, where, and how a key is kept

The credential design follows from where each component runs, so that is settled first.
Tack is one binary with three roles chosen by subcommand — server (`tack serve`), client
(every other subcommand, `tack mcp` included) and runner (`tack serve --with-runner`, or
the separate `tack-runner` binary when the runner is on another machine). The roles
combine into three deployment shapes, and only the first is the target of this phase:

| Shape | Who | Board runs | Runner runs | Status |
|---|---|---|---|---|
| **One person, one machine** | a developer with several projects, their own harness logins and their own API keys | on the laptop, `tack serve --with-runner` | inside the same process, loopback only | **the normal case — what every card in this Part is built for** |
| One person, an always-on board | the same developer, wanting the board reachable when the laptop is closed | on a home server, NAS or small VPS, `tack serve` | on the laptop, `tack-runner`, pulling work over HTTPS | possible today (deployment guide), **not a target**: it needs a reachable host, TLS and a tunnel or overlay network that the normal case never has |
| Several people, one board | a team; each person's runner holds that person's keys | on a shared host | one per person, on their own machines | **deferred** — needs identity; see below |

The split is not a bet that the board will live elsewhere. What the normal case needs
from it is narrower and more important: **the work must outlive the window.** Closing
the browser tab, or the whole UI, must not stop a running attempt or lose a queued one,
and reopening it must show the current state. That is exactly the line the split draws:
the server process (board state plus the embedded runner) keeps running, and the UI is a
view that attaches to it over loopback and fetches the current state when it comes
back. The runner is also the component that holds credentials and touches code, so it
must be able to live wherever those are; in the normal case that is the same machine,
the same process. The split costs the normal case one flag (or the UI switch decision 6
of ADR 0061 adds) and is what makes the other two shapes possible without a redesign.

### Three kinds of secret, three different owners

| Secret | Example | Held by | Standard |
|---|---|---|---|
| A harness's own login | Claude Code's OAuth session, `codex login` | **the harness CLI** — Tack never reads or copies it | OAuth 2.0 device flow (RFC 8628); each CLI keeps its own session (the OS keychain on macOS, an owner-only file on Linux). Every orchestrator worth copying delegates here. |
| A provider API key injected into the harness's environment | a Vercel AI Gateway key | **the runner**, on the machine that launches the harness | OS keychain first, owner-only file second — what `gh` and `docker` do. Never the shared board database. |
| Tack's own secrets | operator token, runner enrollment credential, S3 backup key | already settled | hashed on the server; owner-only and `[REDACTED]` on the runner |

Only the second row was undecided. ADR 0061 decides where it lives; this section fixes
*how*.

### How the runner keeps a provider key

The store the runner gets (VI-B1) has two backends, chosen at runtime, in this order:

1. **The operating system's credential store** — macOS Keychain, Windows Credential
   Manager, Linux Secret Service (what GNOME Keyring and recent KWallet expose) —
   through the `keyring` crate. Encrypted at rest by the OS, bound to the logged-in
   user, invisible to other accounts on the machine, absent from every backup and sync
   folder. This is the desktop standard, and the one a developer on their own machine
   gets without configuring anything.
2. **An owner-only file** in the runner's state directory, mode `0600`, when no
   platform store answers — a headless Linux box, a container. This is the level the
   harnesses themselves use on Linux and what `gh` falls back to. The runner says which
   backend it is using — in `tack runner doctor` and in the UI's response to a pasted
   key — so nobody believes a key is in a keychain when it is in a file.

Entries are named, never positional: `<provider>/<label>`, with `default` the only label
this phase writes. A second key for the same provider (a client's gateway key next to
your own) fits the naming without a migration; letting a project pin a label is a later
card, once a second key exists.

A work request's `secret_reference` — already in the wire contract, never resolved until
now — gains one optional scheme: `store:<name>` (the default when no scheme is given, so
the frozen fixture stays valid) and `env:<VARIABLE>`, which reads the value from the
runner's own environment at spawn time. The second is the twelve-factor path for a
runner started by systemd with `Environment=` or `LoadCredential=`, and needs no store
at all. A store encrypted with a key-encryption key taken from the environment (the
n8n / Gitea pattern) is the *server* standard, and is deliberately not built until a
headless deployment that wants the UI paste route actually exists.

Rejected outright, because they look convenient and are not standard: a provider key in
`tack.db` or `app_meta`, encrypted or not (it turns the S3 backup into a container of
vendor credentials and the server into the holder ADR 0050 forbids); reading or copying
the harnesses' own auth files (no contract, formats change, and it breaks the user's
login); any home-grown encryption.

### Identity and a second person — later, with a trigger

The user model this Part serves is one person with several projects, their own harness
logins and their own keys, on their own machine. Under that model the runner already *is*
the per-person credential boundary: whoever runs the runner owns the keys it holds, and
the board never sees one. Accounts, sessions, roles and per-user tokens would add nothing
that model can use.

They become necessary the day a second person shares a board — to say who created a
request, which runners they may dispatch to, and to give the audit trail a real actor
instead of a hash of the one shared token. That is the demand ADR 0059 asked to see
before being reopened. When it appears, the shape is the ordinary one for a self-hosted
service, recorded here so it is not re-derived: a users table (Argon2id), opaque
sessions in an `httpOnly` cookie, per-user hashed API tokens, two roles (owner, member),
`runners.owner_user_id` and `execution_requests.created_by`, then OIDC. A runner holding
several people's keys (a shared CI box) is a further step — per-user envelope encryption
at the runner — and is not designed until such a runner exists. That work is a Part of
its own, with a new ADR that supersedes 0059; nothing in Part VI depends on it, and
nothing in Part VI is built in a way it would have to undo.

## The two-component story

Every page that introduces agents — the README, the book's introduction, the agent-runners
guide, the developer overview — opens with this statement or links to it. It is written once,
on the Part VI board (§VI.0), and applied verbatim by VI-A3:

> **Tack is two components, built to be one product.**
>
> **The board** is the project manager: workflows, timelines, dependencies, per-project
> vocabulary — one binary, one SQLite file, no accounts, no cloud. It is the plan, the
> policy and the record. It decides *what* runs, *when*, under *which* limits, and it keeps
> the durable history of every run: events, decisions, artifacts, and what it measurably
> cost. **It never executes code and never holds a model credential.**
>
> **The runner** is a small worker that lives where the code and the credentials already
> are — a laptop, a CI box, a machine with a GPU. It pulls work from the board, checks out
> an isolated workspace, launches the coding agent you already use — Claude Code or
> Codex — and reports back. **It holds the keys; the board never sees them.**
>
> They are separate because they scale and fail differently. **One board, many runners:**
> a board on a small VPS dispatches to runners on ten developers' machines, each with its
> own agent, model and capacity. A runner that dies mid-run cannot corrupt the board — its
> lease expires and its fencing token stops writing. A board that restarts cannot lose a
> run — the runner's journal knows what it started. **One developer runs both in one
> process with one command**, on the same contract, with the same recovery.

Two consequences follow. **The runner is named in the story, never on a default screen** —
the README and the docs explain two components because that is the product, while the UI's
default screens say *agent*, *model*, *provider*, *run* and keep "runner", "fleet",
"enroll", "heartbeat", "lease" and "harness" under *Advanced*. And **the recovery demo
is the visual proof of the third paragraph**: V-C2's recording — kill the runner mid-run,
see `needs_operator` and no duplicate, requeue, succeed — is not a competing hero asset; it
is the picture of "they fail differently", and the README gives it a named slot beside
that paragraph. The everyday picture — an item on the board, one click, a run streaming on
the item, done — is the hero, and VI-D2 records it from a release build with a real agent.

## What this phase deliberately does not do

- **Add a second provider**, or let Tack call a model API for any reason, including key
  validation — the runner's probe does that.
- **Broker a vendor OAuth login.** Those stay in the terminal and are rendered, step by
  step, in the UI.
- **Touch docket** (ADR 0060), **build identity** (ADR 0059; the trigger and the shape it will take are
  recorded above, under "Identity and a second person"), or pick up the features Phase
  59 deferred — notifications, i18n, time tracking, in-UI diff review. Listing an attempt's
  artifacts (VI-C4) is not reviewing a diff; that stays a later card.
- **Exercise the `decisions` path with a real harness.** No harness in this tree asks a
  mid-run question; VI-C4 lists decisions, it does not manufacture one.

## Exit

A person who has never seen Tack starts it with `tack serve`, opens the board, and follows
one screen: they turn agent execution on with a switch, paste a gateway key, or copy the one console command it shows
them for the harness they already use; they choose a model from a list the runner actually
measured; they press *Run with agent* on an item without typing an identifier; and they
watch the attempt complete with the model they asked for recorded as the model they got.
Every step they could not do in the UI was shown to them in the UI, with a check that it
worked. The key they pasted sits in the operating system's keychain — or, where there
is none, in one owner-only file the runner names as such — and nowhere else, and the
documentation says so in the same paragraph that says Tack never proxies a model. And a
stranger who reads the README's first screen reports two components — a board that plans
and records, runners that execute where the code lives — before they see a single Kanban
column.

# Desktop app and background service (Phase 61)

**Status:** delivered.

**Tack runs as a background service, and the window is a view of it.** Closing the window
never stops the work; only Quit does. The normal install is a desktop application with its
own window and icon, like Docker Desktop, that starts and supervises the Tack server. The
`tack` binary stays what it is for servers and the terminal, and gets the same daemon
through `tack service install`.

The decision record is [ADR 0062](https://github.com/yielab/tack/blob/develop/docs/adr/0062-desktop-app-and-background-service.md)
— eight decisions in one table, **accepted 2026-09-03**. The board is Part VII in
`TODO.md` (top of the file) and the dispatch plan is
`docs/agent-handoffs/part-vii/README.md`; both were created from this section. **Wave 18
(VII-A2, VII-B1) was dispatched 2026-09-03.**
**The board closed 2026-09-07** with VII-D1: a stranger reached a finished attempt from a
downloaded AppImage without a terminal. What shipped, and the two limits it states, are on
the Part VII board in `TODO.md`, not here.

## Why this phase exists

Today a person starts Tack in a terminal and opens a browser tab. The board and runner
already survive a closed tab — the UI reconnects and re-fetches — but nothing survives
the terminal: closing it kills a running agent attempt. Part VI removes every console
step but one, and that one is "start Tack". This phase removes it, and makes the daemon
promise visible instead of implied.

## The shape

| Piece | What it is | Why |
|---|---|---|
| `tack-desktop` | A separate Tauri 2 program: window + tray + supervisor | The server binary must never carry a webview or GTK |
| The sidecar | The platform's `tack` binary, bundled, run as `tack serve --with-runner` | One server, tested once; the app attaches to one that is already running |
| The window | The same web UI, loaded from the local server | No second frontend, no app-only API |
| The tray | Open · runner status · launch at login · Quit (warns on in-flight attempts) | The daemon promise, stated where the user can see it |
| Data | OS per-user app folders, passed as the existing `TACK_*` variables | Apps do not write next to where they were launched |
| `tack service install` | systemd (user) / launchd unit for the terminal path | "Outlives the terminal" without the app |

## Cards — created as the Part VII board on acceptance

| Card | Scope | Needs |
|---|---|---|
| VII-A1 | ADR 0062 accepted (decision card; the user accepts) | — |
| VII-A2 | `tack service install \| uninstall \| status`: systemd user unit, launchd plist, typed unsupported on Windows; uses the OS data folders; docs | A1 |
| VII-B1 | `crates/tack-desktop` skeleton: Tauri 2, sidecar `tack`, supervisor start / attach / stop with version check, window loads the local URL, single instance; `.deb` and `.AppImage` built on the dev machine | A1 |
| VII-B2 | Tray and lifecycle: close hides, tray menu, Quit warns on in-flight attempts, launch-at-login toggle on by default | B1 |
| VII-B3 | Data folders and first run: the four `TACK_*` variables under the OS data dir; first-run screen shows the location and accepts an existing `tack.db`; runner switch state shown, never flipped by the app | B1 |
| VII-C1 | Release pipeline: bundles for Linux, macOS, Windows in the release workflow; CI prerequisites; icon set; unsigned, with the one-time warnings documented in the release notes | B2 · B3 |
| VII-C2 | README "Run it" leads with the app; install page; book; screenshots of the window and tray under Part V's asset rules | C1 · VI-C1 (the first-run screen must be the real Agents page) |
| VII-D1 | Stranger proof: install from the release artifact on a clean user account, open, run an agent on an item, close the window, reopen, see the attempt finished, Quit warns. Linux measured on this machine; macOS and Windows `not_measured` until a machine exists, stated as such | C2 |

Waves continue Part VI's numbering: **18** A1 → A2 ∥ B1 · **19** B2 ∥ B3 · **20** C1 → C2 ·
**21** D1. Cross-Part: B2 reads the runner switch VI-B3 persists; C2 waits for VI-C1;
README and `docs/screenshots/**` follow the conflict rules in `TODO.md` §V.3 and §VI.3.

## What this phase deliberately does not do

- **Mobile or remote access.** A phone on the LAN or a board on a VPS needs infrastructure
  the normal case does not have; both stay out.
- **Decide code signing.** Certificates cost money; the app ships unsigned with the
  warnings documented until that decision is made separately.
- **A second frontend, or an app-only API.** The window shows the served UI, full stop.
- **Turn the runner on by itself.** Installing the app is not consent to run agents; the
  UI switch from ADR 0061 stays the only way.
- **A Windows service.** `tack service` returns a typed unsupported there; the app is the
  Windows path.

## Exit

A person downloads the app from the release page, opens it, sees the board in its own
window with Tack's icon in the dock or taskbar and in the tray, turns agent execution on
from the Agents page, runs an agent on an item, closes the window, comes back later and
finds the attempt finished with its artifacts listed. Quit warns them if something is still
running. `tack service install` gives a terminal user the same guarantee without the app.
Every platform's result is either measured or marked `not_measured`; none is assumed.


# Human maintainability (Phase 63)

**Status:** open — **the priority board**, ahead of the release tag and the publish list.
Created 2026-09-11. The board is Part IX in `TODO.md` (top of the file); the specification
is `docs/closed-cycles/plans/human-maintainability.md`; the dispatch plan is
`docs/agent-handoffs/part-ix/README.md`. All three were created from the audit this
section summarises.

**Status correction (2026-09-12):** Waves 27–31 (cards IX-M0 through IX-M7) are done on
`develop` — the tool and gate, the mechanical moves, `tack-test-support`, all 28 test
binaries pruned, the harness core (`LocalProcessHarness<G>`/`HarnessGrammar`) extracted
with both adapters migrated and cross-adapter test duplication eliminated, comments
trimmed to budget, and documentation generated/archived. Auditing before dispatching Wave
32 found IX-M4's own test-volume target (§1 below) was never actually reached — measured,
not re-derived from the plan's estimate — so the old IX-M8 split into **IX-M8** (a second,
realistically-scoped volume pass) and **IX-M9** (the ratchet-lock, now sequenced after it);
both are open, in Waves 32 and 33. `TODO.md`'s Part IX table and its Wave 32 audit note are
the authority for current numbers; this page only records intent and does not track
completion further.

**Status correction (2026-09-14):** closed. Waves 32–33 landed and a follow-up cut the size
exclusions from 89 to 44. Measured against the tree before this phase
(`measure --totals`): production code **+416** lines, comments −2 906, tests −4 404 (−6 %),
fixed sleeps 74 → 3. The exit line below held for sizes and did not hold for its purpose:
the phase moved code rather than removing it, because it budgeted sizes and excluded
behaviour change. Phase 64 replaces it.

**The code is fine; what surrounds it is not.** Production Rust is 57k lines. Tests are
74k lines — 1.29 per production line — and 21k of them sit inside production files, so the
three largest "source" files are 70–90 % test. Comments are 23 % of production, and 149
comment blocks are over any reasonable budget. Non-generated documentation is 91k lines,
60k of which are agent handoffs. The suite runs in 18.8 s; nothing here is about speed.
It is about how much a person must read before touching anything.

## Why this phase exists

Every card that built this tree proved itself in isolation and never paid the cost of
reading what it left behind. Each test is correct; each comment is true; the sum is a tree
no maintainer can hold. "Write fewer tests" is not a rule an agent can keep. A place where
each kind of test lives once, a size budget per file and per test, and a script that fails
the push when a file grows past it — those are.

## The shape

| Piece | What it is | Why |
|---|---|---|
| `scripts/maintainability.py` | Measures every file; `check` compares to a committed baseline and fails on regression | A rule with a command survives; prose rules decayed here twice |
| One place per kind of test | `<module>/tests.rs` for unit tests over 150 lines; `tests/<subject>` per binary; `tests/contract/`, `tests/live/`; helpers in `tests/common` and `tack-test-support` | A helper written 18 times is 18 places to change |
| Budgets | Body ≤ 40 lines, name ≤ 60 chars, preamble ≤ 10 (tests) / 30 (source), `///` ≤ 15, comment share ≤ 30 %, invariant pinned in ≤ 2 layers | The numbers a person would hold another person to |
| The agent lane | Gitignored `tests/scratch_*.rs`; ≤ 15 tests and ≤ 600 test lines per card; the ratchet | Agents keep proving their work; the tree stops keeping the proof |
| Documentation | The book includes `docs/*.md` instead of copying; the API reference is generated from the spec; `cargo doc` in CI; closed cycles moved to `docs/closed-cycles/` | One source per topic |

## Cards — the Part IX board

| Card | Scope | Needs |
|---|---|---|
| IX-M0 | The tool, its baseline, `check --changed` in pre-push and CI, scratch tests gitignored | — |
| IX-M1 | Inline test modules move to `<module>/tests.rs`: 40 modules, 20 882 lines, one command | M0 |
| IX-M2 | Over-budget preambles leave the source: 25 files, one command; vendor lore to fixture READMEs | M1 |
| IX-M3 | `tack-test-support` and filled `tests/common`; the helper copies go — one sub-card per crate | M2 |
| IX-M4 | Prune each of the 28 test binaries to the rules; coverage floors guard it — one sub-card per binary | M3 |
| IX-M5 | The harness core, card T0 of the harness audit | M4 (runner) |
| IX-M6 | Comments trimmed to budget, batches of ten files; dev-notes resolved | M2 |
| IX-M7 | Docs: includes, generated API reference, `cargo doc`, the move to `docs/closed-cycles/` | M2 |
| IX-M8 | Close the test-volume gap M4 left (measured, not the plan's original estimate): 0 duplicate tests, 0 fixed waits, ≤ 40-line bodies outside named exclusions | M4 |
| IX-M9 | Budgets lowered to the targets; `check` becomes a hard gate at whatever ratio M8 achieves | M8 |

Waves continue Part VIII's numbering: **27** M0 → M1 → M2 · **28** M3 · **29** M4 ×28 ·
**30** M5 · **31** M6 ∥ M7 · **32** M8 · **33** M9.

## What this phase deliberately does not do

- **Change behaviour.** No route, function or fixture answers differently afterwards.
- **Touch the frontend.** It is inside the target ratio; the rules bind new tests only.
- **Add abstractions** beyond the test-support crate and the API-reference renderer.
- **Release.** The tag and the publish list wait for IX-M9.

## Exit

`scripts/maintainability.py check` is a hard gate at the target budgets and is green. Tests
are at or below M8's measured, achieved ratio (0.8 is the aspiration; a named, reasoned gap
to it — from contract/fixture test volume — is an accepted outcome, not a failure). No
comment block, test name, test body or file is over budget outside M8's named exclusions;
no fixed wait remains; no test claim is written twice. The book builds from
the authoritative files with a generated API reference, and the closed cycles are under
`docs/closed-cycles/`, out of the
working tree. A stranger opens `engine.rs` and reads an engine.

---

# A codebase for human maintainers (Phase 64)

**Status:** open since 2026-09-18, when ADR 0068 was accepted with its stages reordered
(the amendment at the bottom of that ADR says why). It is aligned with ADRs 0066 and 0067
and supersedes ADRs 0060 and 0065 and parts of 0050 and 0064. Stages are sequenced below; how
each is cut into tasks, in what order and under which limits is
`docs/plans/phase-64.md`. Work is recorded in ADRs, this page and the commit history — not
in a card board.

**The goal is the smallest codebase that does everything a user can do today.** Tack's
features for AI agents are product and stay: the runner, harnesses, the runner-v1 protocol,
MCP and the desktop app. What goes is the scaffolding that existed only because agents built
the tree, the mechanisms nothing calls, and the tests that proved a card was done rather
than that the product works.

## Why this phase exists

Phase 63 set out to make the tree maintainable by a person and measured the wrong thing.
File and function size budgets are met by moving code, so production code grew, tests fell
6 %, and a 10-table control plane nobody enables is still compiled, migrated, tested and
rendered. Two thirds of the test lines verify agent execution; attachments, custom fields
and multiple boards have close to none. CI requires 10 jobs on every pull request and fails
on a coverage measurement, not on lost coverage.

## Stages

| # | Stage | What it removes or changes | Done when |
|---|---|---|---|
| 0 | **Measure** | Nothing. Re-measure ADR 0068's tables with their commands: the legacy surface file by file, which `tests/orchestration` files are runner-v1, every caller of DAG-ordered sprint dispatch. | Every table in ADR 0068 re-measured and dated. |
| 1 | **CI and coverage first** | Five per-crate coverage builds → one `cargo llvm-cov nextest --workspace` run that is also the test run; one workspace floor; ≥ 80 % patch coverage; three tiers (pull request / merge to `main` / nightly); the `main` ruleset's required checks updated to match. The rustls advisory resolved. | Pull request #56 is green and merged into `main`; a pull-request run completes in under 15 minutes. |
| 2 | **Agent scaffolding out** | `.claude/`, `TODO.md`'s boards, dispatch plans, per-card handoffs (frozen into `docs/closed-cycles/`), `scripts/maintainability.py` and its baseline, `list-fixed-waits.py`, `check-test-hygiene.sh` → clippy lints; `CLAUDE.md` ≤ 40 lines. | `docs/closed-cycles/` is the only place history lives and no check reads it; `pre-push` runs fmt and clippy only. |
| 3 | **Harnesses on one core** | **Core landed 2026-09-18:** a harness is a `HarnessDescriptor` plus a four-method `HarnessGrammar` on `harness/local_process.rs`; `codex.rs` 886 → 189 lines, `claude_code.rs` 1 141 → 305, harness unit tests 3 240 → 1 554. **Remaining:** the Chat Completions wire, the docket grammar (against docket's shipped `harness-v1` contract) and the opencode grammar, each with captured fixtures and one zero-spend end-to-end test. Plan: `docs/plans/harnesses.md`. | As in ADRs 0066 and 0067; `tack runner doctor` lists four harnesses on a machine that has them. |
| 4 | **Mechanisms with no caller** | `model_profiles` goes (table, routes, MCP tool, panel). The `decisions` path stays and gets the caller it never had: one seam in the harness core that lets any CLI pause and ask the operator, claude-code first, and a choice at dispatch between `auto` and `ask`. | `git grep` finds `model_profiles` nowhere outside the migration that drops it; a run dispatched with `ask` reaches `waiting_decision` and continues when answered. |
| 5 | **Retire the Docket control plane** | `ControlPlane` trait, reconciler, adapters (docket, github_actions, prometheus, registry, legacy_bridge), orch routes and tokens, `tack orch`, the Fleet/Approvals/Economics/Provision screens, their tests; one migration per dropped table, the rows kept in the whole-database snapshot the migration runner takes before a table drop. **Not before Stage 3's docket harness has landed.** | No `orch_*` table on a fresh install; `TACK_ORCH_*` gone from `docs/CONFIG.md`; upgrade from a populated database tested once. |
| 6 | **Test suite rebuilt by layer** | Each invariant kept at its lowest layer plus at most one HTTP test; wave gates and narrative multi-claim tests deleted after their real claims move; behaviour tests added for the board features with no coverage; E2E reduced to critical journeys (Chromium on merge, cross-browser nightly); weekly report-only `cargo-mutants` on `tack-core` and `tack-db`. | Workspace line coverage at or above its Stage 1 floor with fewer tests; every public API route has a success, auth and error test; flaky-test quarantine documented in `docs/TESTING.md`. |

Stages 1, 2 and 3 touch no board behaviour and can run in parallel. Stage 4 and Stage 5
each change the product and each ship with release notes naming what was removed; what a
user of the bridge loses until a harness upgrade replaces it — the approvals inbox,
one-click pod provisioning, per-product cost from docket's events, DAG-ordered sprint
dispatch — is named there too. The release tag and `docs/LAUNCH-CHECKLIST.md` resume after
Stage 6.

## What this phase deliberately does not do

- **Remove agent-facing product.** Runner, harnesses, runner-v1, MCP and
  the desktop app stay.
- **Rewrite history.** Closed boards, handoffs and ADRs stay, frozen.
- **Chase a number.** No stage is done because a ratio or a percentage moved; each is done
  when its surface is gone or its behaviour is verified.
- **Add tooling for its own sake.** Standard tools only: nextest, llvm-cov, clippy, rustfmt,
  cargo-deny, Playwright, Vitest, cargo-mutants.

## Exit

A person clones the repository, reads `README.md`, `CONTRIBUTING.md` and
`docs/TESTING.md`, and has everything needed to change any part of Tack. There is one
orchestration model, no table or route without a caller, a test suite that verifies each
behaviour once at the right layer, and a pull-request CI that finishes in under 15 minutes
with honest coverage of the code the change touched.
