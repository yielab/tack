# IX-M7-docs handoff

This is the **generated documentation** half of IX-M7. The archival half (moving
`TODO.md` Parts I–III and their handoffs to `docs/closed-cycles/`) is a separate
card and writes its own `IX-M7-archive.md` — not touched here.

- Base SHA / branch / final SHA: `8613a05` (`develop`) / `agent/ix-m7-docs` /
  this branch's tip (worktree: `/tmp/ix-m7-docs`,
  `CARGO_TARGET_DIR=/tmp/ix-m7-docs-target`).
- Files changed:
  - Book pages: `docs/book/src/developer/{README,crate-tour,testing,deployment,api-reference}.md`,
    two new pages `docs/book/src/developer/{configuration-reference,mcp}.md`,
    `docs/book/src/SUMMARY.md`, `docs/book/src/user-guide/agent-runners.md` (one dead anchor)
  - `docs/API-REFERENCE.md`, `docs/ARCHITECTURE.md`, `docs/TESTING.md`, `frontend/README.md`
  - New: `scripts/gen-api-reference.py`
  - Wiring: `.gitattributes`, `scripts/regen-generated.sh`, `scripts/setup-git.sh`,
    `.githooks/post-merge`, `.githooks/pre-push`, `.github/workflows/ci.yml`
  - `.claude/context-budget.md`
  - Six `.rs` doc-comment-only fixes (broken intra-doc links found while proving
    the `cargo doc` gate): `crates/tack-db/src/repo/orch.rs`,
    `crates/tack-orch/src/adapters/{github_actions,prometheus,registry}.rs`,
    `crates/tack-orch/src/scheduler/types.rs`,
    `crates/tack-runner/src/{transport,harness/local_process}.rs` — no
    production logic changed in any of the six; verified by `cargo check
    --workspace` and `cargo clippy --workspace --all-targets -- -D warnings`
    both green, and by reading each diff (link text/target only).
- Explicitly out of scope, not touched: `TODO.md`, `docs/closed-cycles/`, any
  Part archival, `crates/*/src/**` comment budgets (M1/M2/M6 already did that).
- Behavior implemented: none — a documentation and CI-gate card. No `.rs`
  production behavior changed.
- Tests added: none (not applicable to this card).
- Secrets/logging review: n/a.
- Checklist: no unowned files touched outside the stated scope above; no live
  secret; no panic stub; no blind retry.

## 1. The book includes, it does not copy

| Book page | Old shape | New shape |
|---|---|---|
| `developer/testing.md` | 296 hand-maintained lines duplicating `docs/TESTING.md` | Short book-context intro + `{{#include ../../../TESTING.md}}` (whole file) |
| `developer/deployment.md` | 189 lines, mostly duplicating `docs/DEPLOYMENT-GUIDE.md` (systemd, Caddy/nginx, security checklist, troubleshooting) | Kept the one genuinely book/workspace-specific section (Local Development with the `.test` Caddy domain — has no home in the production-facing guide) + `{{#include ../../../DEPLOYMENT-GUIDE.md}}` for everything else |
| `developer/README.md` ("Architecture Overview") | Narrative "why" walkthrough, no reference to `docs/ARCHITECTURE.md` | Added a pointer note: this page is narrative, `docs/ARCHITECTURE.md` is the authority for crate-boundary tables, DB schema, endpoint list and troubleshooting |
| `developer/crate-tour.md` | Same — no cross-reference; its own intro claimed the book "predates `tack-orch`/`tack-runner`" (copied from `ARCHITECTURE.md`'s old claim) | Added a pointer note (per-file walkthrough vs. reference) |
| new `developer/configuration-reference.md` | did not exist | `{{#include ../../../CONFIG.md}}` + a pointer to the user-guide's `configuration.md` for the loading-order narrative |
| new `developer/mcp.md` | did not exist | `{{#include ../../../MCP.md}}` |
| `docs/book/src/SUMMARY.md` | no entries for Config/MCP | added both, in the existing list style, between "Adding Features" and "API Reference" |

**Judgment call — did not convert `crate-tour.md`/`README.md` into includes of
`ARCHITECTURE.md`.** The card's framing assumed the book's crate tour was stale
("predates `tack-orch`/`tack-runner`, hasn't caught up" — `ARCHITECTURE.md`'s own
words). I read both files fully before touching either: `crate-tour.md` already
has dedicated, detailed sections for `tack-orch` (including `scheduler/`,
`model_policy/`, `execution_retention.rs`, `execution_observability.rs`,
`usage_provenance.rs`) and `tack-runner`, **and** a `tack-desktop` section that
`ARCHITECTURE.md` did not have at all. The staleness claim was itself stale —
exactly the "measure before you repeat a number/claim" failure mode this
workspace has hit before. I:
1. Corrected `ARCHITECTURE.md`'s header to state the true relationship (crate
   tour is a deeper, separately-maintained per-file walkthrough; `ARCHITECTURE.md`
   wins on a *fact* disagreement, not a depth disagreement) instead of a
   directionally wrong "the book hasn't caught up."
2. Added the missing `tack-desktop` crate to `ARCHITECTURE.md`'s structure
   diagram, since it's real content the tour already had and the map was
   missing.
3. Added bidirectional pointer notes (above) instead of forcing an `{{#include}}`
   merge that would have deleted `crate-tour.md`'s superior, unique per-file
   detail to satisfy a premise that didn't hold up. `testing.md` and
   `deployment.md` got real includes because those *were* genuine duplicates
   (near-identical prose, verified side by side); `crate-tour.md`/`README.md`
   are a different genre (tutorial vs. reference) with real but small overlap.

## 2. The API reference is generated

New script: `scripts/gen-api-reference.py` (178 lines — over the plan's ~150-line
estimate; the estimate wasn't a cap). Pure function of `docs/openapi.json`:
walks every path/method, groups by the spec's own `tags` (in their declared
order), and for each operation prints the route, a de-duplicated summary
(strips a doc-comment's own `` `GET /path` — `` lead-in so it isn't repeated
under a heading that already shows it), a parameter table, the request-body
schema name, and a response-status/schema table. Schema **names** only — never
inlines a schema body — per the card's "at minimum" bar.

Run: `python3 scripts/gen-api-reference.py` → writes
`docs/book/src/developer/api-reference.md` (1,829 lines, 99 paths / 136
operations, up from a 114-line hand-written page that had already been
narrowed to "don't enumerate endpoints, the spec is truth" prose without
actually rendering the spec).

Wiring:
- `.gitattributes`: added `docs/book/src/developer/api-reference.md
  merge=tack-generated linguist-generated=true`, same as `openapi.json` and
  `schema.gen.ts`.
- `scripts/regen-generated.sh`: appended `python3 scripts/gen-api-reference.py`
  after the OpenAPI-freshness step (so it always renders the *confirmed*
  spec, per the card's instruction).
- `.githooks/post-merge`: added the file to the `generated=(...)` array it
  diffs/regenerates/stages.
- `.githooks/pre-push`: added a regenerate-and-diff step (same pattern as the
  existing `schema.gen.ts` check) — always runs, not gated behind
  `node_modules` like the TS one, since the Python script has no such
  dependency.
- `scripts/setup-git.sh`: updated its one echoed file list for accuracy.

**`docs/API-REFERENCE.md`: 1,556 → 329 lines** (both `wc -l`, verified after
the rewrite — not carried over from the plan doc's number, which was already
13 lines stale by the time I measured). What stayed: the two auth surfaces
(`/api` operator bearer token + the three separately-tokened privileged
routes; `/api/runner/v1`'s per-handler hashed credential, fencing, and the
known `409`-vs-`stale_lease` inconsistency from `CLAUDE.md`), WebSocket event
types and a client snippet (no OpenAPI representation exists for this), and
worked multi-step examples (GitHub/Linear import, the full `POST
/api/executions` → poll → decision-token flow, `orch-dispatch`/`orch-run`).
What moved: every flat per-endpoint parameter/response listing (Projects,
Items, Dependencies, Attachments, Sprints, Roles, Comments, Search, Templates,
Custom Fields, Multiple Boards, Backup, Health/Debug) — now covered by the
generated page. Non-schema business rules that lived inside those flat
listings (status-not-settable-at-create, sprint full-replacement-on-PATCH,
backup's staged-restore/`.bak`/blank-keeps-secret semantics, import's
rate-limit/pagination facts) were **not deleted** — they moved into one new
"Notable behaviors not visible in the schema" section, since a schema
genuinely cannot say them and dropping them would have been a real
information loss, not a dedup.

**Found and fixed in passing (not asked for, but load-bearing while rewriting
this file):**
- `docs/API-REFERENCE.md`'s "Import Project (Placeholder)" section claimed
  `POST /api/projects/import` was "basic validation implemented, full import
  pending." Checked `crates/tack-api/src/handlers/export.rs::import_project`
  directly: it is a complete two-pass (create → wire `parent_id` → wire
  dependencies) import that also restores workflow/vocabulary and rolls back
  on any failure — matching what `docs/ARCHITECTURE.md`'s own Export/Import
  section already said correctly. The placeholder claim was simply stale;
  removed rather than carried into the new doc.
- `docs/API-REFERENCE.md` linked to `docs/API-EXAMPLES.md`, which does not
  exist in this tree (confirmed with `ls`). `frontend/README.md` had the same
  dead link. Both repointed — the frontend one to the new generated reference
  plus the worked-examples section here, the old one just deleted along with
  the section that named it.
- `docs/book/src/user-guide/agent-runners.md` linked to
  `API-REFERENCE.md#runner-fleet--execution`, an anchor this rewrite removed
  (that section is now "Worked examples"). Repointed to `#worked-examples`.

## 3. `cargo doc` as a CI gate

Added to `.github/workflows/ci.yml`'s `rust` job, right after the existing
Clippy step:

```yaml
- name: Doc build (broken intra-doc links deny)
  run: RUSTDOCFLAGS="-D rustdoc::broken_intra_doc_links" cargo doc --workspace --no-deps
```

That is the exact flag that works — `-D rustdoc::broken_intra_doc_links` via
`RUSTDOCFLAGS`, not a `cargo doc` CLI flag (no such flag exists; `RUSTDOCFLAGS`
is the standard way to pass rustdoc lint level overrides through cargo). No
`missing_docs` lint anywhere, per the card's explicit instruction — confirmed
by `grep -rn missing_docs crates/ .github/` returning nothing this card added.

Verified locally before wiring it in, per the card's instruction (`cargo doc
--workspace --no-deps` under the same `RUSTDOCFLAGS`). It failed with **9 real
broken intra-doc links** across `tack-db`, `tack-orch` (5) and `tack-runner`
(2), each fixed by qualifying the link path or, where the target is a
trait-provided method rustdoc can't resolve via `Type::method` syntax without
an inherent impl, de-linking to a plain code span:

| File | Broken link | Fix |
|---|---|---|
| `crates/tack-db/src/repo/orch.rs` | `[`tack_orch::ControlPlane::decide_approval`]` — `tack-db` does not depend on `tack-orch` (dependency points the other way) | De-linked to plain text |
| `crates/tack-orch/src/adapters/registry.rs` | `[`ControlPlane`]`, `[`RegistryError::UnknownKind`]`, `[`RegistryError::Construction`]` in the file's `//!` module doc — items imported via `use` or defined in the same file don't resolve by bare name from a `//!` comment the way they do from a `///` item doc in the same file (confirmed: unqualified `///`-doc references to the same items, elsewhere in the same file, resolved fine) | Fully qualified (`crate::ControlPlane`, `crate::adapters::registry::RegistryError::*`) |
| `crates/tack-orch/src/adapters/prometheus.rs` | `[`MetricSample`]` in `//!` doc, same cause | Qualified `crate::MetricSample` |
| `crates/tack-orch/src/adapters/github_actions.rs` | `[`ControlPlane`]`, and `GithubActionsAdapter::kind`/`capabilities` (trait methods with no inherent impl) | `ControlPlane` qualified; the two trait methods de-linked to plain code spans (fully qualifying the type didn't resolve them either — rustdoc doesn't reliably resolve `Type::method` intra-doc links when `method` only exists via a trait impl and there's no inherent impl to anchor it) |
| `crates/tack-orch/src/scheduler/types.rs` | `[`super::select::IneligibleReason`]` — wrong path; `IneligibleReason` is defined in this file (`types.rs`), not in `select` | Fixed to `[`IneligibleReason`]` (same-module bare name, correct this time) |
| `crates/tack-runner/src/transport.rs` | `[`establish_session`]` — a private async method on generic `HttpRunnerClient<A, W, C>`, referenced by bare name from an unrelated free function's doc | De-linked to plain text |
| `crates/tack-runner/src/harness/local_process.rs` | `[`SupervisedProcess`]` in `//!` doc — defined in `crate::harness::process`, not this file | Qualified `crate::harness::process::SupervisedProcess` |

None of these were behavior changes — every fix is a doc-comment link target
or a de-linked code span. `cargo check --workspace` and `cargo clippy
--workspace --all-targets -- -D warnings` are both green on the final tree.
Also verified `crates/tack-desktop` (its own workspace, own CI job, not
touched by this card's CI change) documents clean under the same flag —
noted for whoever next touches its CI job, not acted on here since the card
scoped this to the `rust` job only.

`docs/TESTING.md`'s CI job table (included verbatim into the book, per §1
above) got one clause added to the `rust` row so it doesn't go stale on day
one.

## 4. Context budget re-measurement

`.claude/context-budget.md`'s own prescribed command
(`wc -l`/`wc -c`, tokens ≈ `chars/4`) re-run against every row. Structure
unchanged; only numbers and two references corrected:

- **Biggest drift:** `docs/agent-handoffs/**` went from 12,161 lines/~221k
  tokens to **46,019 lines/~820k tokens** (263 files, up from 48) — Parts VI
  through IX added most of that. `TODO.md`'s active-boards section (Parts
  IX/VIII/VII/VI/V/IV) roughly doubled, from ~2,370 to ~4,870 lines, almost
  entirely because Part VI's single status-table cell is now ~187k
  chars/~47k tokens on its own (consistent with
  `docs/plans/human-maintainability.md` §4's separately-measured "one cell is
  18,906 [characters]" claim about a *different*, smaller snapshot of the
  same cell — it has grown further since).
- **`docs/API-REFERENCE.md` row corrected for this card's own edit**: 1,433 →
  329 lines, and its "Read it?" guidance rewritten — it used to say "grep for
  the endpoint," which is now wrong instruction since endpoints no longer
  live in that file. Added one new row for the generated
  `docs/book/src/developer/api-reference.md` (1,829 lines/~15k tokens, "never
  read whole, grep it like the spec") since it's a large file that didn't
  exist before this card and belongs in a table whose whole purpose is
  flagging exactly that.
- **`docs/CONFIG.md` grew 3x** (73 → 203 lines) since it was last measured —
  still cheap in absolute terms (~6k tokens), noted so nobody is surprised.
- Verified every extraction-recipe example still resolves: `head -58 TODO.md`
  was measuring against a header that has since grown to 68 lines (the "Which
  board is live" table gained rows); corrected to `head -68`. The worked
  `### V-A2` grep example still matches a real heading. `III-C2.md` is still
  the single largest handoff — checked against all 263 files by `wc -c`, not
  assumed.
- No file this page references has moved or been deleted.

## What a stranger still cannot do

Nothing behavioral changed. What's different: a stranger reading the
developer book now gets `docs/TESTING.md`, `docs/DEPLOYMENT-GUIDE.md`,
`docs/CONFIG.md` and `docs/MCP.md` rendered in one place instead of hunting
for whichever of book-page-or-source-file happened to be current; the API
reference they read is generated from the same spec CI already gates, instead
of two documents that could each independently drift from the code and from
each other; a broken doc-comment link now fails CI instead of silently
shipping a dead cross-reference in `cargo doc` output; and `.claude/context-budget.md`
tells the truth about `docs/agent-handoffs/**`'s current size instead of a
number that was off by nearly 4x.

## Gate — final green output

```
$ command -v mdbook
/home/ox/.cargo/bin/mdbook

$ mdbook build docs/book
 INFO Book building has started
 INFO Running the html backend
 INFO HTML book written to `/tmp/ix-m7-docs/docs/book/book`

$ CARGO_TARGET_DIR=/tmp/ix-m7-docs-target ./scripts/regen-generated.sh
[... cargo test-profile build, openapi_contract test ...]
     Summary [   0.079s] 5 tests run: 5 passed, 0 skipped
(git diff --exit-code after this run: only this card's own intentional edits —
 no further drift from running the script again; docs/openapi.json and
 frontend/src/shared/api/schema.gen.ts show zero diff, confirming the spec
 was already current and this card touched no API code)

$ CARGO_TARGET_DIR=/tmp/ix-m7-docs-target RUSTDOCFLAGS="-D rustdoc::broken_intra_doc_links" cargo doc --workspace --no-deps
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.16s
   Generated /tmp/ix-m7-docs-target/doc/tack_api/index.html and 7 other files

$ cargo fmt --all -- --check
(no output — clean)

$ (cd crates/tack-desktop && cargo fmt --all -- --check)
(no output — clean)

$ CARGO_TARGET_DIR=/tmp/ix-m7-docs-target cargo check --workspace
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 1.27s

$ CARGO_TARGET_DIR=/tmp/ix-m7-docs-target cargo clippy --workspace --all-targets -- -D warnings
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 19.46s

$ ./scripts/check-comments.sh
✓ no board archaeology in crates/ frontend/src frontend/e2e

$ ./scripts/check-test-hygiene.sh
✓ tests take their temporary paths from a guard

$ python3 scripts/maintainability.py check --changed
✓ maintainability budgets hold (7 files checked)
```

No `cargo nextest run` was required by this card and none was run beyond what
`regen-generated.sh`'s `openapi_contract` filterset itself triggers.

## Re-baselined?

`no`. This card touches doc-comment link targets only in `.rs` files (see the
table in §3), never comment volume or production line count in a way the
maintainability baseline tracks; `scripts/maintainability-baseline.json` is
untouched (`git status --porcelain -- scripts/maintainability-baseline.json`
is empty).

## Context spent

- Tokens read before the first edit: the workspace and project `CLAUDE.md`
  (already in context), `docs/plans/human-maintainability.md` §4 in full (it
  is short — the whole file is ~5k tokens), then every file this card
  touches or replaces, read in full before editing: both `TESTING.md`
  variants, both `DEPLOYMENT-GUIDE.md` variants, `ARCHITECTURE.md`,
  `crate-tour.md`, `developer/README.md`, `docs/API-REFERENCE.md` (all 1,556
  lines — necessary to decide what was schema-redundant vs. a real business
  rule), the existing hand-written `developer/api-reference.md`,
  `docs/openapi.json`'s structure (via `python3 -c`, not read whole),
  `docs/CONFIG.md`, `docs/MCP.md`, `SUMMARY.md`, `.gitattributes`,
  `regen-generated.sh`, `.githooks/{post-merge,pre-push}`, `ci.yml`'s `rust`
  job, and `.claude/context-budget.md` whole (it is the file this card
  corrects).
- Context size at handoff: moderate-high — this card's four pieces are
  genuinely independent, so nothing was re-read across them, but each piece
  needed at least one large source file read whole to make a defensible
  keep/cut decision (most expensive: `docs/API-REFERENCE.md` at 1,556 lines,
  read entirely before cutting 79% of it).
- Files opened and not used as a source of new content: none — every file
  read fed a decision recorded above.
- Read-list lines that were wrong: the card's own framing that the book's
  crate tour "predates `tack-orch`/`tack-runner`" (quoting
  `ARCHITECTURE.md`'s stale self-description) was wrong — see the §1
  judgment call. This is exactly the kind of decayed status claim
  `CLAUDE.md` warns about, and it was one commit away from being propagated
  into a destructive edit (gutting `crate-tour.md`'s superior detail to match
  a premise that didn't hold).

## Amendments

*(none yet)*
