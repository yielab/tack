---
name: gate
description: Run this repo's verification, cheapest-first and scoped to what changed (e.g. /gate api, /gate frontend, /gate full). Use before declaring any work done and after resolving merges.
---

# Verification — cheapest first, scoped to the diff

Argument = scope: `core|db|orch|api|runner|frontend|contract|full`. If no scope given,
derive it from `git diff --name-only <base>` — test the crates/dirs actually touched.
Never start with `--release` or the full workspace when a scoped check answers the
question.

Disk note: parallel-agent builds fill `/home` — export `CARGO_TARGET_DIR` to the `/`
partition (see workspace memory).

## Rust

```bash
cargo fmt --check && cargo clippy --workspace -- -D warnings
cargo test -p <touched-crate>          # one -p per touched crate
```
Crate map is in CLAUDE.md. Two standing caveats:
- Concurrency claims must be proven against a **file-backed** DB — the shared in-memory
  harness can mask locking races.
- Live-harness runner tests stay `--ignored` (some are billed); the fake-harness path is
  the always-runnable one.

## Contract / spec (run when touching wire shapes, DTOs, handlers, or the API surface)

Named gate tests live under `crates/*/tests/` — list them (`ls crates/*/tests/`) rather
than trusting a remembered set; cycles add gates. As of Part III the load-bearing ones
are `runner_contract` (byte-pins the contract fixtures), `wave2_gate` (full lifecycle on
the real router), and `openapi_contract` (spec drift).

If the spec gate fails on drift and the API change is intentional, regenerate — never
hand-edit:
```bash
UPDATE_OPENAPI=1 cargo test -p tack-api --test openapi_contract
cd frontend && npm run gen:api     # commit both generated files with the source change
```

## Frontend

```bash
cd frontend && npm run type-check && npx vitest run && npm run build
```
E2E (`make e2e`) only when routes/journeys/response shapes changed. A response-shape
change must also update the matching mocks in unit/E2E tests. Record engines that cannot
run on this machine as *unverified* rather than chasing or hiding them (webkit has a
known missing-system-lib issue — check the latest handoff before burning time on it).

## `full`

All of the above plus `cargo test --workspace`; add `make audit` for release-facing work.

## Reporting rules

- **Baselines come from the repo, not memory**: compare test counts against the most
  recent status-board entry or handoff in `docs/agent-handoffs/`. A drop in test COUNT
  is a finding even when everything passes.
- A status-code assertion alone doesn't prove a "writes nothing / rejects before X"
  claim — assert the absence directly, and prove new tests load-bearing by reverting the
  fix once and watching them fail.
- Flaky-under-contention failures get recorded, never silently rerun into green.
