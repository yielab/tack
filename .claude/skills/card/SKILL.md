---
name: card
description: Pick up and deliver one card/task from this repo's active planning board (TODO.md) — token-lean cold start, correct branch, ownership rules, gates, handoff. Use when asked to "work card X", dispatch a wave, or continue whatever cycle is currently active.
---

# Work a board card

> **Report using `.claude/reporting-contract.md`.** Lead with the capability in plain language — what someone can or cannot do now — and keep file and function names in the technical-detail section at the end. Explain every blocker as: what is missing, what it was for, what it blocks.


You are delivering ONE unit of planned work from `TODO.md`. Works for any cycle (Part
III today, whatever succeeds it tomorrow). The argument is the card/task id if given.

## 1. Discover the active cycle — never read TODO.md whole

`TODO.md` is huge and mostly decision history of superseded cycles. Extract, don't read:

```bash
# The file's own header states which Part/cycle is ACTIVE — trust it over section order:
head -30 TODO.md

# Map the structure, then pull only what you need by line range:
grep -n "^# \|^## " TODO.md

# Your card's section (headings follow "### <ID> — <title>"):
n=$(grep -n "### <ID> " TODO.md | cut -d: -f1); sed -n "${n},$((n+45))p" TODO.md
```

From the active cycle's section, locate and read (line-ranged, not whole-file):
- the **status board** table — which waves are accepted, at which SHA, what's open;
- the **rules of engagement** for parallel agents;
- the **shared-file ownership** table for the current wave;
- your card's **Owns / Tasks / Acceptance**.

Then read only the handoffs in `docs/agent-handoffs/` that your card's section names as
dependencies. CLAUDE.md (already in context) covers architecture — don't re-derive it.

## 2. Durable rules (these outlive any cycle)

- **Branch** from the SHA the status board names as the current base, named
  `agent/<card-id-lowercase>-<slug>`. Integration branches are merged into by the wave's
  integrator card only.
- **Contract fixtures outrank code.** Wherever a frozen contract dir exists (e.g.
  `docs/contracts/`), fixtures are the authority over Rust/TS types. Disagreement is
  escalated in the handoff, not resolved by bending either side. A fixture edit and its
  pin-table update land in the same change.
- **Respect shared-file ownership.** If your change needs a file the ownership table
  assigns to another card, write the request into your handoff instead of editing it.
- **Generated files are never hand-edited** — find the regeneration command in CLAUDE.md
  or the gate skill and commit regenerated output together with its source change.
- Unsupported is typed, unknown is explicit, unmeasured is nullable. No `unimplemented!()`,
  no zero standing in for "unknown" — capability claims are load-bearing.
- Logs carry ids only (no credentials/prompts/env values), and tests assert the redaction.
- Never `git commit` without the user asking; never add Co-Authored-By/AI attribution.

## 3. Deliver, then gate

Implement against the card's **Acceptance** list. Test the claim itself: for "writes
nothing / rejects before X" claims, assert the absence directly (row counts, untouched
checkpoint) and prove the test load-bearing by reverting the fix once. Run `/gate
<scope>` before declaring done.

## 4. Handoff (mandatory)

Write `docs/agent-handoffs/<cycle-dir>/<ID>.md` following the template in the cycle's
rules section. Record what you believed, what you verified, what you escalated, and any
shared-file requests. Corrections are appended as amendments — the original claim stays;
the history of what was believed and later falsified is the point. Update the status
board row only if you are the integrator; otherwise propose the row text in the handoff.

## Final step — write the report

This is part of the card, not an afterthought. Full rules in
`.claude/reporting-contract.md`; the required shape is below and is not optional.

**Do not open with the branch, the commit, or "X is delivered".** That is housekeeping —
it goes at the very end. Open with what a person can now do.

```
## What this is about
One or two sentences. The feature in human terms. No file names, no ids, no modes.

## Where it stands
What works now that did not before. What still does not work. Plain sentences.
Evidence goes in Technical detail — not here.

## What is left
One short paragraph per item. Each answers, in order: what is missing, what it was
supposed to do, what it blocks — then who fixes it. Order: blocks the feature working
at all, then blocks the next piece of work, then can wait.
If you fixed something outside your ownership, say so here and name the integrator.

## Technical detail
Labelled items, ONE topic each. Never one dense paragraph listing five files.
  **Where the code lives** — files added or changed.
  **How <one mechanism> works** — one topic per item, repeat as needed.
  **What is blocking, technically** — exact type or file, and why.
  **Test results** — the numbers.
  **Not checked** — what was skipped, and why it is not covered.

## Next step
One sentence: what to do next, and the command if there is one.
Then, on its own line, the housekeeping: branch name and whether it is committed.
```

Two failure modes to check before sending: a plain-language section that contains an id,
a file path or a permission mode (move it down), and a technical section that is one
paragraph covering several topics (split it into labelled items).
