# VI-A3 handoff

- Base SHA / branch / final SHA: base `4740ee0` (`develop` tip at dispatch) / branch
  `agent/vi-a3-two-components` / final SHA — not committed (per instructions, this card's
  work stays uncommitted in the working tree for the caller to review).
- Files changed (must equal ownership list): `README.md` (whole-file restructure),
  `docs/book/src/introduction.md`, `docs/book/src/developer/README.md` (opening only),
  `CLAUDE.md` (Project Overview, first sentence only), `docs/diagrams/two-components-light.svg`
  and `docs/diagrams/two-components-dark.svg` (new), `docs/book/src/diagrams/two-components-{light,dark}.svg`
  (new — relative symlinks into `docs/diagrams/`, so the book renders the same file the
  README does, not a second copy that can drift). Matches the ownership list exactly; no
  crate, no `docs/screenshots/**`, no `agent-runners.md` touched.
- Contract fixtures consumed: none — this card touches no wire contract.
- Behavior implemented: no runtime behavior; this is a documentation-only card. The README's
  first screen was rebuilt in this order — H1, an opening line naming both components, the
  new two-component SVG diagram (behind a `<picture>` for light/dark), the §VI.0 statement
  verbatim, a "Durable by design" slot, a "How it works" 3-step strip, and a "Two components"
  comparison table — with no PM screenshot above any of it. `hero.gif` moved down into
  *Screenshots* with alt text that says plainly it shows no agent run. *Features* now lists
  agent execution first. `introduction.md` opens with the same statement and the same
  diagram, and its core-concepts table gained **Runner** and **Run**. `developer/README.md`
  opens with the statement's third paragraph only, per the card. `CLAUDE.md`'s first sentence
  now names both components and states that `--with-runner` embeds one in the other.
- Tests added and exact commands/results: none applicable (no code); verification instead:
  `mdbook build docs/book` → clean, no output (CI's own broken-link check is literally
  grepping this build's stdout for "error"/"broken" — see `.github/workflows/ci.yml:288-292`
  — and it printed nothing). Confirmed the diagram assets are physically present and non-empty
  in the built output: `docs/book/book/diagrams/two-components-{light,dark}.svg`, 4530–4531
  bytes each, real SVG content (not broken symlinks).
- Failure/adversarial case proved: see "What a stranger still cannot do" and the adversarial
  read-test below — both are the equivalent of a failure case for a documentation card (the
  page failing to communicate the architecture).
- Schema/API/contract change requested from another owner: none.
- Known limitations or `not_measured` fields: the "Durable by design" slot is a link to
  `scripts/smoke.sh`'s step 9, not a recording — V-C2 has not landed (no
  `docs/agent-handoffs/part-v/V-C2.md` exists yet), so per the card's own instruction the
  slot "carries the sentence and a link to the smoke step that proves it, and no placeholder
  image." The GitHub-light/dark render proofs are a faithful local simulation of GitHub's
  documented `<picture>` + `prefers-color-scheme` technique (verified against the *mechanism*,
  not against a live github.com page, since this branch is not pushed).
- Secrets/logging review: not applicable — no code, no secrets touched.
- Safe merge order and likely conflicts: `README.md` is a shared-conflict file per the
  board (Part V's V-C2/V-C3 may also touch it — §VI.3/§V.3). This card explicitly
  **supersedes V-A4's README structure** (stated in the card text) while keeping V-A4's
  claim → evidence rows; a merge should take this card's structure as authoritative and let
  V-C2 fill the named "Durable by design" slot with its recording afterward, rather than
  re-merging V-A4's old flat layout. No conflict expected with VI-A1/VI-A2 (Wave 14
  siblings) — they own `docs/CONFIG.md`, `agent-runners.md`, and the ADR, none of which this
  card touched.
- Checklist: no unowned files (confirmed against `git diff --stat develop`, 4 files: `CLAUDE.md`,
  `README.md`, `docs/book/src/developer/README.md`, `docs/book/src/introduction.md`, plus new
  files under `docs/diagrams/**`); no live secret; no panic stub (n/a, no code); no blind
  retry (n/a, no code).

## Claim → evidence

| Claim (README first screen) | Evidence |
|---|---|
| "A self-hosted project board that dispatches its items to coding agents ... through runners that live where your code and credentials already are" | `crates/tack-runner/` owns credentials/workspace/harness subprocess per CLAUDE.md's crate map; pull-based protocol at `/api/runner/v1` (CLAUDE.md boundary rules) |
| Diagram: board holds "leases · fencing · history"; runner holds "credentials stay here" | `crates/tack-api/src/handlers/runner_protocol.rs` — 16 fencing-token call sites (CLAUDE.md); `tack-runner` never exposes credentials to the API (ADR 0050/0058, cited in TODO.md §VI.0) |
| Diagram: one pull arrow, runner → board, "the board never calls out" | README's own (unchanged) Architecture section, line ~259: "runners pull work, Tack never calls back into developer machines" |
| The §VI.0 statement (all three paragraphs, verbatim) | `TODO.md` §VI.0 — the design authority this card was told to copy verbatim, not re-derive |
| "Durable by design": kill the runner mid-attempt → `needs_operator`, no blind duplicate, explicit operator requeue → succeeded | Read directly: `scripts/smoke.sh` step 9, lines 322–409 — SIGKILLs runner+harness (line ~373), asserts `needs_operator` (line ~382), asserts run/attempt counts unchanged before vs. after restart (line ~387), touches the release file and POSTs `/requeue` (line ~394), asserts a succeeded attempt follows (line ~398) |
| Two components table — "How many": One / Many | Statement paragraph 3, "One board, many runners" (pre-approved text) |
| Two components table — "What happens when it dies" (board side) | Statement paragraph 3, "A board that restarts cannot lose a run — the runner's journal knows what it started" |
| Two components table — "What happens when it dies" (runner side): "no duplicate, no silent loss" | `scripts/smoke.sh` step 9 assertions: harness run count and attempt count identical before/after the kill+restart, with no operator decision in between |
| Two components table — "How you run it": `tack serve` / `tack serve --with-runner` (embedded) or `tack-runner` (remote) | README's own (unchanged) "Run it" section; CLAUDE.md Commands section |

## Measured numbers

- Diff vs. `develop`: 4 files changed, 108 insertions, 20 deletions (`git diff --stat develop`).
- First screen (H1 through the *Two components* table): **README.md lines 1–64**. Zero PM
  screenshots in that range (`grep -n "docs/screenshots" README.md` → first hit is line 116,
  inside *Screenshots*).
- Statement byte-identical check: `diff` between `TODO.md:193-211`, `README.md:19-37`, and
  `docs/book/src/introduction.md:5-23` — **zero diff lines**, all three identical. Paragraph 3
  alone, additionally identical against `docs/book/src/developer/README.md:3-8` — **zero diff
  lines**, all four identical.
- Vocabulary-check grep (§VI.1 rule 8 words) over the four files this card wrote: see below.
- `mdbook build docs/book` (local mdbook 0.5.3; CI pins 0.4.40 — not cross-checked against that
  exact version): clean build, no stdout/stderr.
- Adversarial read-test subagent cost: 44,142 tokens (reported by the Agent tool), 1 tool call,
  ~15s — see below.

## What a stranger still cannot do

A stranger who reads only the new first screen now correctly concludes the product is two
components (a board and a runner), what each holds, and that one board serves many runners —
proved below. Two things are still not true after this card: they cannot yet **watch** the
recovery sequence happen, because the "Durable by design" slot links to the smoke script
rather than a recording (V-C2 has not landed); and the **product's own UI** still doesn't
reflect this framing — its default screens still say whatever VI-A1/VI-C1 have or haven't
changed them to say. This card only changed static documentation, not the running app.

## Surface-map delta

None. VI-A3 is documentation-only and moves no row of §VI.0's surface map from console to
UI — it doesn't touch the UI at all. No escalation needed.

## Vocabulary check

The README is allowed "runner" (and its neighbors) — it is telling the two-component story,
which is explicitly exempted by §VI.1 rule 8 and by this template's own note. Full grep of
rule 8's words (`runner|fleet|enroll|heartbeat|capacity|lease|fencing|harness`, case-insensitive)
over every file this card wrote or edited:

- `README.md` — dozens of hits, all inside the story (the opening line, the diagram alt text,
  the statement, "Durable by design", "How it works", the "Two components" table, the
  pre-existing "Run it"/"Status"/Architecture sections which this card did not rewrite). All
  are documentation prose describing the architecture, not a rendered default UI screen —
  none of this card's changes touch `frontend/`.
- `docs/book/src/introduction.md` — hits in the statement, the diagram alt text, and the new
  **Runner**/**Run** core-concepts rows and the "Agent Runners & Fleet Execution" Quick Links
  entry. Same exemption: this is the developer/user book, explicitly one of the two places
  (with the README) the rule allows the vocabulary.
- `docs/book/src/developer/README.md` — the statement's third paragraph (intentional, per the
  card) plus one unrelated false-positive: line 132, `broadcast::Sender<BoardEvent> (capacity:
  100)` — a Rust channel buffer size in an existing code snippet, not the architecture term
  "capacity" the rule means (unchanged by this card; flagged here for completeness).
- `CLAUDE.md` — the one changed sentence says "runner" once ("shipped as a single `tack`
  binary where `tack serve --with-runner` embeds a runner in the board's own process") —
  CLAUDE.md's overview is explicitly named in §VI.0 as one of the pages that carries this
  story.

No hit appears on a default app screen; this card wrote no frontend code.

## Adversarial acceptance test — how it was verified

Per the task's instruction to run this myself rather than only describe it: extracted
`README.md` lines 1–64 (the exact first screen, H1 through the *Two components* table) to a
standalone file containing nothing else about this repository, then dispatched a fresh
general-purpose agent (no memory of this session, no other file access, instructed not to
explore anything else) with only that file and four direct questions: what is the product;
what does each component hold; what is the numeric relationship between them; is this "a
generic project manager" or something else.

The fresh agent's answers: two components, "the board" and "the runner"; correctly restated
what each holds (board: workflow/policy/budgets/durable history, never executes code or holds
a credential; runner: credentials/workspace/harness, pulls work and reports back); correctly
identified "one board, many runners" as the explicit relationship (citing the diagram's own
alt text and the comparison table's "How many" row); and concluded, unprompted, that it is
**not** a generic project manager but "a project board purpose-built to dispatch and durably
track work executed by coding agents/runners, with a hard separation between planning/policy
and code-execution/credentials" — the exact distinction the card exists to land. This is the
same test the dispatcher's adversarial check specifies; I ran it myself instead of leaving it
for the dispatcher, per this task's explicit instruction.

## Render proof

Three screenshots, plus one supporting full-page capture, all produced with
`npx playwright screenshot` (pattern from `frontend/e2e/screenshots.spec.ts`) against a local
harness page embedding the exact `<picture>` markup used in `README.md`, and against the
actual `mdbook build` output:

- `docs/agent-handoffs/part-vi/vi-a3-render-proof/proof-github-light.png` — the `<picture>`
  under `--color-scheme light`, simulating GitHub's light theme (white background, `img`
  variant selected).
- `docs/agent-handoffs/part-vi/vi-a3-render-proof/proof-github-dark.png` — the same page under
  `--color-scheme dark` (GitHub's actual dark background, `#0d1117`); the `<source
  media="(prefers-color-scheme: dark)">` variant is selected, confirming the swap GitHub's own
  documented dark-mode-image technique relies on.
- `docs/agent-handoffs/part-vi/vi-a3-render-proof/proof-book-introduction.png` — the real
  `docs/book/book/introduction.html` file (mdBook's own output, opened directly, not a mockup)
  showing the statement and the diagram rendering inline in the built book, ahead of "Core
  concepts".

These three PNGs are untracked working-tree files (not committed, per instructions). A
fourth, full-page (not just viewport) capture of the same book page exists only in session
scratch, not the repo, and is not needed beyond what it already confirmed (the diagram's
position relative to the rest of the page):
`/tmp/claude-1000/-home-ox-Sites-objetivosMios/5cb297d8-ec52-4a49-92db-e4c89509c8cf/scratchpad/render-proof/proof-book-introduction.png`.
The repo copy (`proof-book-introduction.png`, viewport screenshot) is the one to keep.

## Drafted GitHub repository description (NOT applied)

Current live description (read via `gh repo view yielab/tack --json description`):
> "Tack assigns board items to AI coding agents — Codex, Claude Code, or OpenCode — and keeps
> the run as part of the project's history, in one self-hosted binary."

Proposed replacement (outward-facing; needs the user's explicit approval before
`gh repo edit yielab/tack --description "..."` is ever run — not done by this card):

> A self-hosted project board that dispatches items to coding agents — Claude Code, Codex,
> OpenCode — through runners that live with your code and credentials. One board, many
> runners: durable history, no accounts, no cloud, one binary.

(238 characters — well inside GitHub's ~350-character limit.)

## Context spent

- Tokens read before the first edit (cold start), against the block's ~11k estimate:
  roughly 12k — close to the estimate. Breakdown: dispatch README header+block (~830),
  `TODO.md` §VI.0 board prelude (~3.1k), original `README.md` whole (~3.5k, per the block's
  own figure), original `introduction.md` whole (~1.5k), `developer/README.md` head -40
  (~600), V-A4 claim/evidence grep+range (~500), V-C2 card section (~200), `ls
  docs/screenshots/` + `book.toml` (~100). On top of the block's own list, the parent task
  also directed reading `TODO.md`'s VI-A3 card section itself (~1.2k) and §VI.1 rule 8
  (~450) for the vocabulary check, and `TEMPLATE.md` (~375) — not counted in the block's own
  11k estimate but necessary to deliver the card's stated deliverables.
  Also opened, not in the read list: `.github/workflows/ci.yml`'s docs job (~150 tokens), to
  confirm the exact gate command and its broken-link check semantics before trusting a clean
  `mdbook build` was sufficient — worth the block adding as a one-line pointer for the next
  docs card (VI-D1).
- Context size at handoff: mid-range, comfortably under the ~150k stop threshold.
- Files opened and not used: none — every file read fed directly into the deliverable or the
  handoff's evidence.
- Read-list lines that were wrong: none found; the block's line ranges and size estimates
  (README 263 l / ~3.5k, introduction.md 60 l) matched the pre-edit files closely.

## Amendments

*(Appended by later readers, dated. The original text above is never rewritten.)*
