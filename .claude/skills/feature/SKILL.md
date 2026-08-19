---
name: feature
description: Add or finish a Tack feature (new entity, endpoint, UI view, config, or an open gap) following the repo's layering, migration, auth and testing patterns. Use for "add X", "finish Y", or any feature work not tied to a planning-board card.
---

# Add / finish a feature the repo's way

For work that exists as a card on the active `TODO.md` board, use `/card` instead — it
carries the ownership and handoff rules. This skill is the architectural path.

## 0. Check it isn't already half-built

This codebase repeatedly grows mechanisms with zero callers (modeled-but-unwired columns,
sweeps nothing invokes). Before designing:

```bash
rg -n "<concept>" crates/ frontend/src | head -30        # existing code paths
grep -n "<concept>" docs/agent-handoffs/*/*.md | head    # was it attempted/deferred?
```

Recorded open gaps live in the **latest wave's status-board entry** in `TODO.md` and the
most recent handoffs — read those for current gaps and any already-proposed route shapes
before inventing your own.

## 1. The layering path (inward-out; each layer has its own tests)

1. **tack-core** — model + pure logic (`models.rs`, `workflow.rs`, …). Zero I/O. Unit
   tests in-file.
2. **tack-db** — migration in `migrations.rs`: **one `ALTER` per migration name** (the
   runner executes statements individually without a wrapping transaction — a
   multi-`ALTER` migration that fails midway bricks the install). Repo module in `repo/`.
   Read-then-write transactions MUST be `BEGIN IMMEDIATE`. A new secret column is added
   to `remote_backup.rs::scrub_snapshot_secrets` in the same commit.
3. **tack-api** — handler in `handlers/`, DTOs, route in `router.rs`, spec entry in
   `openapi.rs`. Keep the two auth surfaces separate: operator routes under `/api`
   behind `require_token`; runner-facing routes under the runner protocol tree with
   their own credential — never route one through the other's middleware. Board-relevant
   mutations broadcast via `websocket::broadcast_event()`.
4. **Regenerate, never hand-edit** the OpenAPI spec and frontend schema (commands in
   `/gate`).
5. **frontend** — types from the generated schema only; colors via `--color-*` tokens,
   never raw hex; a changed response shape updates the matching unit/E2E mocks in the
   same change; unknown values render as an honest literal (e.g. `Not measured`) — a
   zero standing in for "unknown" is a lie, especially about money.
6. **tack-cli** (if operator-facing) — subcommand speaking HTTP to the server; the CLI
   never opens the DB directly.
7. Anything touching a frozen wire contract additionally updates the fixture dir + its
   pin table in one change (see `/card` rule) — fixtures are the authority.

## 2. Config & security posture

- Anything that **deletes data or reaches the network** ships off-by-default behind a
  `TACK_*_ENABLE` gate; read/log-only watchers may default on. Follow the precedent of
  the nearest existing flag.
- Privileged actions get their **own token**, distinct from `TACK_API_TOKEN`, fail-closed
  when unset (existing pattern: approval/decision tokens).
- New config vars are documented in `docs/CONFIG.md`'s table in the same change; secrets are
  write-only over the API and never logged.

## 3. Done means gated

Run `/gate <scope>` for every layer touched. Update an existing doc under `docs/` if one
covers the area; don't create new .md docs unprompted (workspace rule).
