# Part IV handoffs — Standalone Single-Binary Operation (Phase 58)

One file per card, named `IV-<card>.md` (`IV-A1.md`, `IV-A2.md`, …). Each card writes
**exactly one**; corrections are appended to it as dated amendments and never rewritten.
No card edits another card's handoff, and no card edits the Part IV board in `TODO.md` —
the wave integrator updates the board after independent verification.

The board is `TODO.md` → **Part IV**, §IV.0–§IV.6. The decision of record is
[`docs/adr/0058-standalone-single-binary-runner.md`](../../adr/0058-standalone-single-binary-runner.md).

## Template

Use §III.2's handoff template verbatim:

```markdown
# IV-<card> handoff

- Base SHA / branch / final SHA:
- Files changed (must equal ownership list):
- Contract fixtures consumed:
- Behavior implemented:
- Tests added and exact commands/results:
- Failure/adversarial case proved:
- Schema/API/contract change requested from another owner:
- Known limitations or `not_measured` fields:
- Secrets/logging review:
- Safe merge order and likely conflicts:
- Checklist: no unowned files, no live secret, no panic stub, no blind retry.
```

Plus the three additions §IV.6 requires for this Part:

- **Binary-size delta** — measured before and after, for any card that changes what `tack`
  links. A real number from the built artifact, never an estimate.
- **Which role executed what** — for any card claiming a live run: whether the attempt was
  claimed by an embedded or a standalone runner, and over which address.
- **Loopback/gating proof** — the test that demonstrates off-by-default and the
  non-loopback refusal, named explicitly. Asserting the behaviour in prose is not the
  proof; the test is.

## The two mistakes this Part is most likely to make

1. **Optimizing away the loopback HTTP hop.** The embedded runner must speak runner-v1 over
   HTTP exactly like a remote one. It looks like an easy win to call the handlers directly;
   it creates a second code path that can drift from `docs/contracts/runner-v1/` and that
   `scripts/smoke.sh` never exercises. If it looks necessary, escalate — do not do it.
2. **A smoke step that cannot fail.** `scripts/smoke.sh` shipped a false green once already
   in this repository: steps that printed `SKIPPED` unconditionally and never set failure,
   so the script reported `SMOKE PASSED` while proving less than it claimed. Any new step
   must be proven load-bearing by breaking the feature once and watching it `FAIL`.
