# Part III agent handoffs

Each Part III card writes one handoff here and stays inside the card's ownership list.
Handoffs are evidence, not acceptance: the wave integrator reruns the card gate against the
combined tree and records the resulting integration SHA in `TODO.md`.

All Wave 0 cards branch from the reviewed historical baseline `1d71785` unless their
handoff explicitly records a later accepted integration SHA.

## How to read a handoff

A handoff records what a card *claims*, with the commands it ran. It is not a verified
statement. Several claims in this directory were later found wrong by independent review —
each such correction is recorded as an amendment section appended to the original file, with
the original claim left standing rather than rewritten. When a handoff and the code disagree,
the code wins; report the discrepancy rather than editing someone else's handoff.

Amendments accumulate at the end of a file in chronological order. A card with three
amendment sections has been revised three times, usually because a later wave produced
evidence that falsified an earlier assumption.

## Status

Wave status and accepted integration SHAs live in `TODO.md`'s Part III board, which is the
authority. Summary as of Wave 3:

| Wave | Cards | Phase | Accepted SHA |
|---|---|---|---|
| 0 — Clean boundary and safety | A0 · A1 · A2 · A3 · A4 | 50 | `f042085` |
| 1 — Domain, schema and runner skeleton | B1 · B2 · B3 · B4 | 51, 52 | `f14019b` |
| 2 — Pull protocol vertical slice | C1 · C2 · C3 · C4 · C5 | 52 | `f931fc0` |
| 3 — Real harness proof | D1 · D2 · D3 · D4 · D5 | 53 | `6a53a18` |
| 4 — Fleet scheduling and PM UX | E1 … E6 | 54 | not started |

`III-wave2-gate.md` is not a card. It records the Wave 2 integration gate, which no single
card could prove because every card's tests were scoped to its own surface.

## Index

### Wave 0 — boundary and safety

- `III-A0.md` — contract/ADR freeze and clean baseline. Owns `docs/contracts/runner-v1/**`.
  Amended in Wave 3 to add the `accept`/`start` exchanges (42 → 46 fixtures).
- `III-A1.md` — trust-boundary repair: sanitisation, CSP, origin-bound credentials,
  redirect handling on credential-bearing clients.
- `III-A2.md` — atomic item mutation and browser concurrency (ETag / `If-Match`).
- `III-A3.md` — migration runner and rebuild recovery.
- `III-A4.md` — green frontend and release baseline.

### Wave 1 — domain, schema, runner skeleton

- `III-B1.md` — neutral execution domain in `tack-orch`. Amended twice: the contract-derived
  retryability authority, and `EmbeddedCapabilitySnapshot` for the enrollment/refresh
  capability shape that no type had modelled.
- `III-B2.md` — execution schema and repository. Amended three times, all concurrency:
  enrollment redemption, the claim path, and a systemic audit of every transaction site.
- `III-B3.md` — `tack-runner` crate skeleton; owns the root manifest.
- `III-B4.md` — contract conformance harness; byte-pins every fixture.

### Wave 2 — pull protocol vertical slice

- `III-C1.md` — operator execution and fleet handlers. Amended: the error envelope did not
  match the fixtures it claimed to implement.
- `III-C2.md` — runner protocol and auth handlers, 13 `/api/runner/v1` endpoints. Amended
  three times: the retryability authority, the conflict-variant split and CAS rotation, and
  a `tracing` test-isolation flake.
- `III-C3.md` — runner client, journal and isolated workspace. Amended: cleanup-refusal
  coverage, and a correction to prose that described two distinct failure paths as one.
- `III-C4.md` — mock end-to-end crash matrix; SQL fault injection against the real
  repository.
- `III-C5.md` — router, OpenAPI and generated-contract integration. Amended: the runner
  body limit did not inherit the configured global limit, contrary to this handoff's claim.
- `III-wave2-gate.md` — the wave gate, driven through the mounted production router.

### Wave 3 — real harness proof

- `III-D4.md` — common process/event infrastructure and the shared fake binary. Corrects its
  own card's premise: `HarnessAdapter` already existed, so this card added `HarnessProbe`
  and `AdapterRegistry` around it rather than defining a competing trait.
- `III-D1.md` — Codex adapter. `codex` was not installed, so this is the one adapter proven
  only against the fake binary; its unverified assumptions are enumerated.
- `III-D2.md` — Claude Code adapter, built against the real CLI. Source of the finding that
  shell-tool subprocesses escape the runner's process group.
- `III-D3.md` — OpenCode adapter, built against the real CLI. Keeps provider/model
  combinations paired rather than flattened, because OpenCode itself accepts a mismatched
  pairing and fails only afterwards.
- `III-D5.md` — harness-contract reconciliation. Four small interface changes, no fixture
  edit, and a registration-time check that refuses a probe claiming `Supported` cancellation.
