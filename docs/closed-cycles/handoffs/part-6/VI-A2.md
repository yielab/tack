# VI-A2 handoff

**ACCEPTANCE (fill in by the user, do not self-mark):**
`docs/adr/0061-provider-credentials-at-the-runner-boundary.md` is **NOT YET ACCEPTED**.
Wave 15 (VI-B1/VI-B2/VI-B3) may not branch until a human records acceptance here as a
dated amendment (see the template's own instruction below):

> User acceptance date: _______________ (append as an "## Amendments" entry when given —
> do not edit this line yourself)

- Base SHA / branch / final SHA: base `767d385813b5b5b86a140a121f3957fcb69f4f4d` (`develop`
  tip, matches the Wave 14 dispatch SHA in `docs/agent-handoffs/part-vi/README.md`'s table),
  branch `agent/vi-a2-adr-0061`, final SHA — see the last commit on this branch (uncommitted
  at handoff time per instruction; not committed).
- Files changed (must equal ownership list): `docs/adr/0061-provider-credentials-at-the-runner-boundary.md`
  (new), `docs/CONFIG.md` (the one provider-credentials bullet, l.89-108 before the edit),
  `docs/agent-handoffs/part-vi/VI-A2.md` (this file). Nothing else. No code.
- Contract fixtures consumed: none. `docs/contracts/runner-v1/` is unaffected — the ADR's
  Contract section notes `ModelCombination` (`crates/tack-orch/src/execution/capabilities.rs:55-60`)
  already carries a flattened `additional` map, and explicitly leaves "does decision 3's
  catalog need a new field" as an escalation for whichever card implements it, per §VI.2's
  "nobody by default" rule for that directory.
- Behavior implemented: none — this is a decision card. It owns no code and none was
  written or touched.
- Tests added and exact commands/results: none. Per the card and the dispatch README,
  this card has no gate to run; the deliverable is reviewed, not tested.
- Failure/adversarial case proved: not applicable — no code path exists to exercise.
- Schema/API/contract change requested from another owner: none required by this ADR
  itself. One conditional escalation is named for the future (decision 3 / Contract
  section): if VI-B2 finds a gateway catalog entry genuinely needs a new wire field on
  `ModelCombination`, that is a `docs/contracts/runner-v1/` change with no default owner
  per §VI.2 — VI-B2 raises it with evidence, this ADR does not pre-authorize it.
- Known limitations or `not_measured` fields: decision 6 (turning the embedded runner on
  from the UI) decides *that* a UI toggle exists but does not close the gap between that
  and the code as measured — `local_runner::with_runner_enabled`/`ensure_loopback` are
  evaluated exactly once, before `tack serve` binds a socket, and no existing path starts
  the runner role inside an already-running plain server. The ADR names this explicitly
  as VI-B3's implementation problem, with an escape hatch (amend the ADR) if the boundary
  cannot be crossed as assumed. This is a stated design gap, not an unmeasured number.
- Secrets/logging review: not applicable — no code, and the ADR itself never states or
  implies an actual key value, fingerprint, or log format; it only fixes where a key may
  live and how it must never be exposed (decision 1/2), consistent with `CLAUDE.md`'s
  "logs carry ids only" rule.
- Safe merge order and likely conflicts: this branch touches two files, both narrowly.
  `docs/CONFIG.md` is shared with VI-A1 (model-resolution rows) and VI-B2 (gateway rows,
  after B1 merges) per §VI.2 — this card's edit is confined to l.89-108's provider bullet,
  a disjoint range from both. `docs/adr/0061-*.md` has no other writer. No conflict
  expected merging to `develop` ahead of or behind VI-A1/VI-A3.
- Checklist: no unowned files touched (verified against §VI.2's ownership table above), no
  live secret introduced (none exists to introduce), no panic stub (no code), no blind
  retry (no code).

## Claim → evidence

| Claim (user-visible, added or kept) | Evidence — command, test name, or transcript |
|---|---|
| ADR 0061 exists and makes six decisions, each with rejected options and a cost | `grep -c "^### " docs/adr/0061-provider-credentials-at-the-runner-boundary.md` → 6; `grep -c "Rejected:" docs/adr/0061-*.md` → 11 (decisions 1 and 2 each reject two options, decisions 3, 4, 5 and 6 each reject two, one, one and two respectively) |
| ADR cites 0050 and 0058 by line for what it reaffirms and bounds | `grep -n "l\.29-30\|l\.80-83\|l\.69-75\|l\.99" docs/adr/0061-*.md` — matches `docs/adr/0050-runner-control-plane.md:29-30` ("never becomes a model proxy") and `:99` ("a Tack model gateway"), `docs/adr/0058-standalone-single-binary-runner.md:80-83` ("Vendor credentials remain outside Tack") and `:69-75` (off-by-default / loopback-only) |
| `docs/CONFIG.md`'s provider bullet no longer states an absolute negation | `git diff docs/CONFIG.md` — the sentence "there is no `TACK_*` variable for a model provider or endpoint, by design" is replaced with the API-server-scoped rule plus a pointer to ADR 0061 for the runner-side exception |
| The status quo before this ADR had zero UI-written provider credentials and zero resolved `secret_reference` entries | `rg -n "secret_reference" crates/tack-runner/src/harness/{claude_code,codex}.rs` → both skip/warn, never resolve, per the ADR's own Measurement section |
| Every sentence in `docs/` / `README.md` the ADR makes imprecise is listed below | see "Sentence list" section — the exact grep from the card's Tasks, run before and after this card's `docs/CONFIG.md` edit |

## Sentence list (Tasks' grep, for VI-A1 / VI-D1)

Exact command: `grep -rn "never becomes a model proxy\|no TACK_\* variable for a model
provider\|never reads, stores, or forwards" docs/ README.md`

**Before this card's edit**, non-ADR hits were:
- `docs/CONFIG.md:104` — "there is no `TACK_*` variable for a model provider or endpoint,
  by design." **Fixed by this card** (see diff above).
- `docs/book/src/roadmap.md:3273` — "ADR 0050 says the Tack API 'never becomes a model
  proxy'; ADR 0058 says 'vendor credentials remain outside Tack'. Both are statements
  about the API server and both are correct. Both are cited … as if they meant 'Tack
  cannot help you configure a provider'." **Not fixed by this card** — it is not in this
  card's `Owns`, and on inspection it already correctly frames the confusion (it is the
  roadmap's own diagnosis of the problem VI-A2 exists to solve, not an instance of the
  confusion). Flagged for VI-A1/VI-D1 to consider adding a forward reference to ADR 0061
  once accepted, but it asserts nothing false as written.

**After this card's edit**, re-running the same grep, the only remaining hits are:
- `docs/adr/0050-runner-control-plane.md:30` — the ADR itself, correctly stating its own
  scope (exempt by design).
- `docs/CONFIG.md:112` — now an attributed quote of ADR 0050's exact wording ("See … ('the
  Tack API never starts a coding harness and never becomes a model proxy')"), immediately
  preceded by the bounded, corrected sentence. This still matches the grep literally
  (it contains the quoted string) but no longer asserts the negation as the whole truth —
  it cites the ADR that does, correctly scoped to the API server. No further action
  needed on this line.
- `docs/book/src/roadmap.md:3273` — unchanged, as above.
- Two self-referential matches inside `docs/agent-handoffs/part-vi/README.md:153` and this
  ADR's own Consequences section, both of which quote the grep command itself rather than
  asserting anything. Not real hits.

`README.md` had zero hits before and after — no README sentence needs correction from
this ADR; VI-A3 is a separate concern (the two-component story), not this grep.

## Measured numbers

- 6 decisions in the ADR, `grep -c "^### " docs/adr/0061-*.md`.
- 11 distinct "Rejected:" paragraphs across the 6 decisions, `grep -c "Rejected:" docs/adr/0061-*.md`.
- 1 file changed outside the new ADR and this handoff: `docs/CONFIG.md`, 1 bullet
  (l.89-108 in the pre-edit file), confirmed by `git diff --stat`.
- 0 code files touched, 0 tests added, 0 migrations added — confirmed by `git status --porcelain`
  showing exactly the two docs files above as changed/new.

## What a stranger still cannot do

A stranger still cannot set a Vercel AI Gateway key from the UI, see a measured model
catalog anywhere, or turn the embedded runner on from a UI switch — none of that exists
yet. This card only removes the ambiguity about whether Tack could ever help with any of
it, and fixes the one `docs/CONFIG.md` sentence that stated outright that it could not.
Wave 15 (VI-B1 the secret store, VI-B2 the gateway and catalog, VI-B3 the UI switch and
key-entry route) has to actually build all three, and per `TODO.md` §VI.3 none of them may
branch until a human has recorded acceptance of this ADR above.

## Surface-map delta

This card writes no code, so it moves no row from console to UI itself. It does two
things to the map: it adopts §VI.0's table as binding (decision 5 — a future "cannot be
UI" claim now needs an ADR amendment, not a card's private judgment), and it rewrites the
stated target of the map's first row ("Turn on agent execution") from "flag-only" to
"UI switch, on a loopback bind, or the flag" (decision 6) — the row's "why not fully UI"
column stays filled ("starting Tack itself is the one console step left"), now reflecting
that the *embedded runner's* on/off state, unlike Tack's own process start, can move to
the UI.

## Context spent

- Tokens read before the first edit (cold start): the block's own estimate was ~12k. My
  actual reads: `TODO.md` header (58 lines) + §VI.0-§VI.3 prelude (~260 lines) + the VI-A2
  card section (76 lines) + ADR 0050 whole (99 lines) + ADR 0058 whole (103 lines) +
  `head -40` of ADR 0060 + `head -40` of ADR 0059 (read instead of 0060 alone, since 0060's
  own header pointed at 0059 for the "Measurement" section shape) + the named `docs/CONFIG.md`,
  `wiring.rs`, `claude_code.rs`, `codex.rs`, `config.rs`, `local_runner.rs`, `settings.rs`
  ranges + the IV-A5 grep. Estimated ~13-15k tokens — close to the block's estimate, slightly
  over because I also read ADR 0059's header (not named in the read list) to see a second
  worked example of the house format before committing to a structure.
- Context size at handoff: comfortably under the 120k ceiling; nowhere near the ~150k stop
  threshold.
- Files opened and not used: `crates/tack-orch/src/execution/capabilities.rs` (a 6-line
  `ModelCombination` struct read via `rg`/`sed`, not in the block's read list) — used, not
  wasted: decision 4 (vocabulary) and the ADR's Contract section needed to state the
  existing wire type's exact shape (the flattened `additional` map) accurately rather than
  from memory of the crate map alone. Recorded here per the instruction to justify any read
  outside the named list, even though it was small and load-bearing rather than wasted.
  `docs/adr/0059-single-operator-identity-posture.md`'s header (40 lines) was likewise not
  named but read to see two examples of the house format's "Measurement" section before
  writing one; also small and used.
- Read-list lines that were wrong: none found — every named range existed at the stated
  location and contained what the block said it would (`settings.rs`'s `app_meta`/`secret`
  precedent, `local_runner.rs`'s `with_runner_enabled`/`ensure_loopback`, both adapters'
  `secret_reference` skip comments).

## Amendments

*(Appended by later readers, dated. The original text above is never rewritten.)*

**2026-09-03 — integrator (Sonnet).** The user read the ADR and found it too long and
too dense to tell what was actually being asked, or why — the original draft opened
with a Status/Date/Supersedes/Contract block and evidence-heavy prose, and the six
actual decisions didn't appear in plain language until well past the first screen. That
is a real defect in a document whose only job is getting a human to say yes or no.

Rewrote `docs/adr/0061-provider-credentials-at-the-runner-boundary.md` following a new
`.claude/decision-contract.md` (added at the same time, modeled on the existing
`reporting-contract.md`): a three-line "Decide / Why now / If you do nothing" block and
a one-row-per-decision table now open the file, in plain language with no citations; all
of the original reasoning, rejected alternatives, and file:line evidence survive
underneath in a "Full reasoning" section, reorganized but not cut for content. The six
decisions themselves are unchanged — this was a rewrite for clarity, not a re-decision.
`.claude/skills/card/SKILL.md` now points future decision-writing cards at the same
contract so this doesn't recur. Status remains `proposed`, still awaiting the user's
dated acceptance below this line.

**2026-09-03 — integrator (Fable), before acceptance.** The user asked for a more robust,
standard way to hold harness and provider credentials, and for a multi-tenant or
authentication design alongside it. The answer, recorded in the roadmap's new
"Credentials: who runs what, where, and how a key is kept" section under Phase 60:

- Decision 1 of ADR 0061 is refined from "an owner-only file" to "the platform keychain
  first, an owner-only file only where no platform store answers, and the runner reports
  which backend holds a key" — the `gh` / `docker` pattern. Table row 1, the opening
  "Decide" paragraph and Full reasoning §1 changed (one rejected option added); decisions
  2–6 are untouched. `TODO.md`'s VI-B1 card and the dispatch README's VI-B1 block were
  rescoped to match (two backends, `store:`/`env:` reference schemes, live keychain proof
  and fallback proof in the acceptance).
- Multi-tenant / identity is answered as a deferred direction with a trigger — a second
  person sharing one board — not as a Part. The user's own framing is that the normal
  case is one person, several projects, their own keys, their own machine, where the
  runner already is the per-person credential boundary. ADR 0059 stands; the shape a
  future identity Part takes is written down so it is not re-derived.

Status remains `proposed`. The acceptance line above is still the user's to fill.

**2026-09-03 — ACCEPTANCE, recorded by the integrator on the user's behalf.** The user
accepted ADR 0061 in chat with the word "listo" in reply to the integrator's explicit
request to accept ADRs 0061 and 0062 ("Pendiente de ti: aceptar ADR 0062 (y la 0061, con la
decisión 1 ya refinada)"). The acceptance covers the six decisions as they stand in the
file at this date, including the refined decision 1. If the user disagrees with this
reading, they strike this paragraph and Wave 15 stops. **User acceptance date: 2026-09-03.**
