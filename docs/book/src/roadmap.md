# Roadmap

> **This file records intent, not status.** What shipped is in `CHANGELOG.md` and the commit
> history; closed phases are archived under
> [`docs/closed-cycles/boards/`](../../closed-cycles/boards/).

**Tack is delivered through Phase 64.** That covers the project-management core, the
harness-agnostic runner fleet, the single-binary embedded runner, adoption and
distribution, agent onboarding and provider choice, the desktop app with its background
service, and the codebase cleanup that retired the Docket control-plane bridge
(`ControlPlane`, `tack orch`, the Fleet/Approvals/Economics/Provision screens).

**Phase 64 closed on 2026-09-19.** Its plan and its chapter of this file are archived; what
it left open is in Phase 65.

**Phase 65 is the live phase**, and its plan is the only list of pending work of that phase; Phase 66, below, is open
beside it with its own plan:
[`docs/plans/phase-65.md`](../../plans/phase-65.md). It folds together what the roadmap,
the ADRs and the harness plan still owed — the release tag and `docs/LAUNCH-CHECKLIST.md`,
inbound GitHub sync, `tack start`, and the harness upgrades — as one ordered sequence of
tasks in four waves, with a "Decided" table so no task re-opens a settled decision and a
"Parked" list so nothing has to be re-inventoried.

---

## Phase 65 — the release, and every open item in one sequence

**Status:** open 2026-09-20; Waves 1–4 landed 2026-09-21 and `v0.1.0-beta.9` shipped the
same day — the first tag with the four-harness fleet and the desktop bundles
(`.AppImage`, `.deb`, two `.dmg`, `.msi`). What remains of Wave 0 is the publish list
and the platforms this machine cannot verify.
**Plan:** `docs/plans/phase-65.md`.

| Wave | What lands | Who |
|---|---|---|
| 0 | Ruleset required checks, branch and build-dir cleanup, the Dependabot decision, the repository description, the `v0.1.0-beta.9` tag, the publish list, one macOS and one Windows install, the signing decision. | the user; refused to agents or costs money |
| 1 | Measurements of codex, opencode and docket's contract, each a captured fixture; `tack start` / `tack open`; inbound GitHub issue state by polling. | Sonnet agents |
| 2 | The capture cap leaves the harness descriptor; opencode's served model; GitHub comments both ways. | Sonnet agents |
| 3 | codex reads usage and the served model, then applies the permission policy; opencode attempts share one package cache; per-project GitHub token and manual issue link. | Sonnet agents |
| 4 | A run that asks before it acts on codex and opencode where the measurement allows it; docket cancel, artifacts and asking once docket ships `harness-v1.1`. | Sonnet agents |

What Waves 1–4 build ships as `v0.1.0-beta.9`. Nothing in the phase waits on a decision:
the two it needed — inbound sync polls rather than receiving webhooks; opencode attempts
share a package cache and nothing else — are taken in the plan and are one line each to
reverse.

---

## Phase 66 — close the Level 3 loop: evidence, briefs, escalation packs, merge-readiness

**Status:** closed 2026-10-05; every task in the table below is built and merged, and ships as `v0.1.0-beta.10`. What is still open is listed under the table. **Plan:** [`docs/plans/phase-66.md`](../../plans/phase-66.md).
**ADRs:** 0069 (the brief is an entity on the item and travels), 0070 (docket contract 1.1,
negotiated at boot; cancel becomes per-harness evidence), 0071 (evidence before deletion, the
verifier boundary, the pushed branch and its pull request), 0072 (the "Run with agent" flow).

After this phase every attempt leaves a patch, both commits and a file list; an item can
carry a typed brief that reaches the harness and an independent verifier; a question arrives
as a pack with options and a recommendation; a verifier's merge-readiness pack is rendered
and accepted or rejected with a reason; a branch is pushed and its pull request followed to
merged, closed or reverted; and one page shows escalation rate, human minutes, the
verification tax in tokens and the acceptance rate. The verifier itself is a separate program
Tack never contains.

The interface comes first. The "Run with agent" dialog shows the whole flow from the second
wave on — who runs it, what it gets, how far it may go, what happens after — and a step whose
integration has not landed is shown disabled with the reason, never hidden.

docket is a moving target by design. Tack negotiates the harness contract and probes each
optional flag when the runner starts, so one Tack build works with docket `0.2.0-beta.3`, the
coming `0.2.0-beta.4` and what follows; each docket link below starts when docket's own board
merges the card it needs, not when docket cuts a release.

| Wave | What lands | Who |
|---|---|---|
| 0 | Decided 2026-10-03. | the maintainer |
| 1 | Evidence captured before the workspace is deleted; the phase's four migrations; `status_map_policy_id` moves the item; the `mrp-v1` and `brief-v1` contracts; each harness's tool list, measured; the README leads with the desktop app. | Sonnet and Haiku agents |
| 2 | The whole flow in the dialog, deferred steps disabled; the brief's routes and export; the consultation pack; the verifier step behind `[verify]`. | Sonnet agents |
| 3 | The brief travels to the harness and the evidence; the brief editor; the merge-readiness review record; the pack in the runner and the inbox. | Sonnet agents |
| 4–5 | The merge-readiness panel; the runner pushes the branch behind `[git]`; the pull request and its fate from the poll that already runs; verification and push become live controls in the dialog. | Sonnet agents |
| 6–7 | Factory metrics, endpoint then page; the pull-request badge. | Sonnet, then Haiku |
| A | docket in five links, following docket's Phase 35: negotiation and contract 1.1; process events and a proven cancel; asking over stdin; limits, policy and files; the recipe flag. | Sonnet; the last link Haiku |

Left open by the work, each listed in the
[Agent Runners](user-guide/agent-runners.md) chapter's "Known gaps" or beside the feature it limits: a docket recipe run takes no token
budget (docket refuses one with `--recipe`) and a recipe with an Implementer step fails for
want of a verify command; a docket run that times out, rather than being cancelled, stops
only its main process group; the branch push runs git in a workspace the agent could have
reconfigured; on a docket that accepts a policy, a request must name docket's own tools; and
`tack runner doctor` does not yet print each harness's `artifacts` and `permission_policy`
lines. Opening a pull request as a choice in the "Run with agent" dialog is shown disabled:
a pull request is opened for a pushed branch whose item is linked to a GitHub issue.

---

## Archived

- [Phases 0–57](../../closed-cycles/boards/roadmap-phases-0-57.md) — the original
  engineering phases, the audit-driven cycle (26–32), the Agent-Factory Control Center
  (33–38), the Agnostic Control Plane (39–49), and the Harness-Agnostic Runner Fleet
  (50–57).
- [Phases 58–63](../../closed-cycles/boards/roadmap-phases-58-63.md) — standalone
  single-binary packaging, the first public release, agent onboarding & provider UX,
  the desktop app and background service, and human maintainability.

---

## Contributing

See [CONTRIBUTING.md](../../../CONTRIBUTING.md) for code style, PR process, and how to add new
features. The [Adding Features](developer/adding-features.md) guide walks through the
three most common extension patterns.

---
