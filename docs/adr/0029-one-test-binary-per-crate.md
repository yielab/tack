# ADR 0029: one test binary per crate, dependencies without debug info, agent builds without the incremental cache

**Status: ACCEPTED 2026-10-26.** The maintainer asked why an agent's build folder reached 25 GB
and had the three changes below applied the same day. It narrows ADR 0007 decision 4 and adds to
its decision 3; the rest of ADR 0007 stands.

**Decide:**

| # | Decision | What changes for whoever writes a test |
|---|---|---|
| 1 | **Each crate has one integration-test binary, `it`.** `tests/it.rs` declares every file under `tests/` as a module; the crate's `Cargo.toml` sets `autotests = false` and names that one target. Helpers shared by several modules (`common`, `support`, `fake_docket`, …) are declared once in `it.rs` | A new subject is a `mod` line in `tests/it.rs`. A file is selected by its module: `-E 'test(/^<module>::/)'`; one crate's: `-E 'binary_id(<crate>::it)'` |
| 2 | **Dependencies carry no debug info** (`[profile.dev.package."*"] debug = false`). The workspace's own crates keep line tables | A backtrace still names a dependency's functions and gives file and line for Tack's |
| 3 | **A build that runs once has no incremental cache.** Agent cards and `scripts/docket-prill-e2e.sh` build with `CARGO_INCREMENTAL=0`, as CI already does. A person's own checkout keeps it | Nothing: it is set where the build is started |

**Why now:** an agent's build folder reached 25–28 GB and the main checkout's 29 GB. ADR 0007
grouped `tack-api`'s files by subject but left 46 integration binaries across six crates (19 in
`tack-app` alone), each one linking all 470 packages of the graph. A card builds a few times and
throws the folder away, so its incremental cache is pure cost.

**If you do nothing:** two cards building at once need about 50 GB, which is what filled `/` on
2026-08-14 and `/home` on 2026-08-12.

## Evidence

Measured on 2026-10-26, a clean `cargo nextest run --workspace --no-run --build-jobs 4` of the same
tree into an empty build folder, before and after:

| | Before | After |
|---|---|---|
| Build folder | 7.6 GB | 2.4 GB |
| Executables in `debug/deps` | 82, 3.9 GB | 42, 1.07 GB |
| Incremental cache | 2.2 GB | none |
| Wall time | 107 s | 91 s |
| Tests listed (`nextest list --run-ignored all`) | 1 518 | 1 518, same names |
| Full suite | — | 1 515 passed, 3 ignored, 17 binaries, 70 s |

The 25–29 GB folders were not one build: they were this 7.6 GB plus what cargo never evicts. The
main checkout held 215 executables for 122 distinct names (`tack_runner`'s tests nine times),
left behind by builds with other flags or features, and 13 GB of incremental sessions. Decision 3
removes the second for agents; for the first, an agent's folder is deleted when its card merges
(already the rule), and the main checkout is cleaned after a toolchain bump or a large merge.

The six `it` binaries are 13–127 MB each.

## What moved

- Nextest filters that named a binary now name a crate's `it` and a module: the boundary suites in
  `.githooks/pre-push` and CI are
  `(binary_id(tack-api::it) & test(/^security::/)) | (binary_id(tack-runner::it) & test(/^(agent_boundaries|h3_checkout|crash_matrix)::/))`;
  the OpenAPI drift gate is `binary_id(tack-api::it) & test(/^openapi_contract::/)`; the serial
  override in `.config/nextest.toml` selects the three serve tests of `tack-cli::it` by module.
- `tests/live/main.rs` is `tests/live/mod.rs`, since it is a module now.
- A test that relied on its binary being its own process still is: nextest runs every test in a
  process of its own. Only `cargo test`, which nothing here uses for the workspace, would share one.

## Considered and not adopted

- **One binary for the whole workspace.** A crate's tests import that crate; one binary across
  crates would rebuild everything for a change in any crate.
- **`opt-level = 1` for dependencies.** Faster tests, a few percent smaller, but every dependency
  is rebuilt on a profile change and the gain was not measured here. ADR 0007 left it out too.
- **`sccache` or a shared build folder between agents.** Two agents writing one folder block on
  cargo's lock; a cache saves compile time, not disk.
