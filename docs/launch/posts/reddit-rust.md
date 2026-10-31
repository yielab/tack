# r/rust (draft, not posted)

**Format notes:** markdown tables supported. r/rust is the least tolerant of these four
audiences toward anything reading as a pitch — lead with the engineering problem, keep
the product framing minimal, expect the top comments to be about the architecture and
the testing approach, not the UI.

## Title

```text
Tack: a modular-monolith Rust workspace where a "kill -9 mid-task" is a tested,
recoverable state, not an incident
```

## Post body

Nine-crate Cargo workspace (`tack-protocol`, `tack-core`, `tack-orch`, `tack-db`, `tack-app`,
`tack-api`, `tack-runner`, `tack-cli`, and `tack-test-support` for tests), Axum + `sqlx`/SQLite
on the server, a separate binary for the piece that actually shells out to an agent CLI (four
supported today: Claude Code, Codex, docket, opencode). The layering rule that's actually
enforced, not just documented: `tack-protocol`, `tack-core` and `tack-orch` do no I/O — no
`tokio`, no `sqlx`, nothing — `tack-app` never names `axum` or `sqlx`, and `sqlx` and the raw
pool live only in `tack-db` (and the test support crate). `scripts/check-crate-graph.sh` fails on
any edge its table does not allow, and runs before every push, so it can't regress silently.

**The part I think is actually interesting for this sub:** the runner talks to the
server over a pull-based HTTP protocol (`/api/runner/v1`), and every attempt-scoped
write is guarded by a fencing token, not just a status flag. If a runner dies mid-attempt
and comes back (or a second runner claims the same work), a stale fencing token gets
`stale_lease` and writes nothing — proven in tests by asserting the actual row count is
unchanged after a rejected write, not just that the HTTP status was right. `BEGIN
IMMEDIATE` is mandatory on every read-then-write transaction against the SQLite pool —
deferred transactions here deadlock under real concurrency, and that's caught by
concurrency tests running against a file-backed DB rather than the shared in-memory
harness (the in-memory one won't reproduce it).

Numbers, measured `2026-10-26` (the release binary is the last measurement CI made), not aspirational:

| | |
| --- | --- |
| Rust | 154,519 lines across the nine crates (`find crates -name '*.rs' \| xargs wc -l`) |
| Tests | 1,505 Rust tests (`cargo nextest run --workspace`) and 772 frontend tests (`npx vitest run`, ~9 s) |
| Migrations | 95, one `ALTER` per migration file — the runner has no transaction wrapping a whole migration, so a multi-statement migration failing halfway would brick the install; the fix was a rule, not a retry loop |
| API surface | 97 documented paths, OpenAPI-generated, diffed against the handler set in CI (a drift gate, not just docs) |
| Release binary | 18.4 MiB, `lto = true, opt-level = "z"`, measured in CI (`stat -c%s target/release/tack`) |

Frontend is SolidJS, types generated from the OpenAPI spec rather than hand-kept in
sync — `schema.gen.ts` is regenerated and diffed in CI so a Rust response-shape change
that doesn't update the frontend fails the build instead of shipping a silent mismatch.

Things I'd genuinely like eyes on:
- Whether the crate-boundary rule (`tack-protocol`, `tack-core` and `tack-orch` do no I/O;
  `sqlx` stays inside `tack-db`) is enforced the right way — right now it's a table of allowed
  crate edges checked by a script in the pre-push hook rather than a workspace-level lint;
  happy to hear if there's a cleaner way to fail this at compile time.
- General Axum/`sqlx` review — this is a real, running system, not a toy, so actual
  criticism is more useful than encouragement.

MIT licensed, 0 stars so far (new post, first time anyone outside this repo has seen
it): <https://github.com/yielab/tack>.
