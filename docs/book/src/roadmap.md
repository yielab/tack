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

**Phase 65 is the live phase**, and its plan is the only list of pending work:
[`docs/plans/phase-65.md`](../../plans/phase-65.md). It folds together what the roadmap,
the ADRs and the harness plan still owed — the release tag and `docs/LAUNCH-CHECKLIST.md`,
inbound GitHub sync, `tack start`, and the harness upgrades — as one ordered sequence of
tasks in four waves, with a "Decided" table so no task re-opens a settled decision and a
"Parked" list so nothing has to be re-inventoried.

---

## Phase 65 — the release, and every open item in one sequence

**Status:** open 2026-09-20; Waves 1–4 landed 2026-09-21, and the release is prepared
(version `0.1.0-beta.9`, its changelog section, the docs). What remains is Wave 0.
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
