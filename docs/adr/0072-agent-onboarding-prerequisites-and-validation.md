# ADR 0072: agent onboarding — prerequisites discovery, validation, and configuration clarity

**Status: ACCEPTED 2026-10-03, as amended at the end — the 2026-10-03 amendment governs.** Not yet linked to the plan; waiting on the user's review. Complements ADR 0069 (the brief), ADR 0070 (docket contract), ADR 0071 (evidence).

**Decide:** approve four things. **(1)** A **pre-run prerequisites checklist** that is discoverable before the "Run with agent" dialog opens, showing what is required, what is configured, and what is missing. **(2)** A **project settings page** where configuration is centralized — harness choice, model selection, repository, allowed tools, agent profiles — separated from execution decisions. **(3)** A **tool discovery and validation system** so users know what tools a harness offers and which are allowed, rather than free-form text; the dialog validates against available tools. **(4)** The "Run with agent" dialog becomes a **pre-flight gate** that either runs (when all prerequisites pass) or links to the missing configuration, not a form that collects prerequisites at execution time.

**Why now:** QA on Phase 66 found that the "Run with agent" button shows a complex form with unclear errors (no agent profile, no model, no repository, manual tool list). There is no way to know what is required until you click the button, and the form mixes settings (harness, model) with run-time decisions (approval mode, timeout). The onboarding fails before anything architectural (brief, evidence, contracts) can work.

**If you do nothing:** every user hits this form confused. The dialog will gain more fields as the harness platform grows (A1–A3 add docket policy, process events, token bounds). The Allowed Tools field becomes unmaintainable.

## The decisions, in short

| # | Decision | Why |
|---|---|---|
| 1 | **Pre-run prerequisites are discoverable before execution.** A new "Setup" or "Configuration" tab on the project shows: harness choice (radio, Current: `[Claude Code]` ✓ or `[None - configure]`), model (dropdown, if not Auto), repository (text, Current: `[none configured]` 🔗), agent profile (dropdown or radio, Current: `[None]` 🔗), approval mode (Auto/Ask/Refuse). A "Run with agent" button is present on task/epic detail; clicking it reads this checklist. If any required field is red, the button is disabled and shows "Configure first"; clicking opens that tab with focus on the first missing field. | Users know what is required before they hit the complex dialog. Configuration is separated from execution. |
| 2 | **Tool availability is discovered from the harness descriptor.** The runner's `HarnessDescriptor` already carries capabilities (`cancel`, `decisions`, `artifacts`); it gains a `tools: Vec<{name, description, supported}>` list. The runner attests this in the claim response (already extended by C3). The board reads it on first execution and memoizes it. The "Allowed Tools" field in the dialog becomes: radio selector "All available" / "Custom list" or checkboxes listing the discovered tools with descriptions. A tool not on the available list → validation error with the list shown. | Users learn what is available from the source of truth (the harness), not by guessing or reading docs. |
| 3 | **The configuration tab is the project's single source for harness setup.** It lives beside "Overview" / "Items" / "Workflows". It has sections: Harness (choice, current capability attestation), Model (dropdown, Auto option, current choice), Repository (text input, "Detect from git remote", "change", "none yet" states), Agent Profiles (table: name, harness, model, created/modified, use, delete), Allowed Tools (discovery, enable/disable per tool). Errors are clear: "Repository: not a git URL" or "Model: unsupported by this harness". Warnings are actionable: "No agent profile configured; create one first". | One place to go. No configuration scattered across dialogs. |
| 4 | **The "Run with agent" dialog is a pre-flight, not a form.** It shows a read-only summary of configuration (Harness: `[Claude Code]` ✓, Model: `[Opus]` ✓, Repo: `[github.com/yielab/tack]` ✓, Agent Profile: `[Default]` ✓, Approval: `[Auto]` ✓). Below that, execution parameters: timeout (default 3600), budget (read from profile or default), approval mode override (if applicable). A "Run" button is active only if all checks pass; a "Missing configuration" message with a 🔗 link replaces it if not. This replaces the current logic where the dialog is a form that collects and validates prerequisites. | Users see their configuration before every run. The dialog confirms, not collects. It is predictable. |

## Evidence

| Fact | Locator (from QA on phase-66 develop, 2026-09-30) |
|---|---|
| The "Run with agent" dialog shows errors without context on what is required | Screenshot: "Unsupported model" error, "no agent profile exists yet", "no repository configured" — none linked to where to fix them |
| No pre-execution visibility of prerequisites | Clicking "Run with agent" is the first time a user learns that a model must be selected or a repository configured |
| Tool availability is manual text input | "Allowed tools" section says "leave blank for none"; users do not know what tools the harness offers |
| Configuration is scattered | Harness choice in the dialog, model in the dialog, repository in the dialog, tools in the dialog — no single place to view or manage setup |
| The dialog mixes settings and decisions | Timeout, allowed tools, approval mode, harness choice, model — some are setup, some are per-execution; users cannot tell the difference |
| ADR 0069, 0070, 0071 are architectural, not UX | They specify what briefs, contracts, evidence are; none address how a user configures the harness or discovers what is available |

## Related tasks in Phase 66

| Task | How this ADR unblocks or improves it |
|---|---|
| **C3** (brief travels) | The brief must reach a harness that is configured. Pre-flight validation ensures the harness is ready. |
| **A1, A2, A3** (docket 1.1 grammar, asking, process events) | Tool discovery (decision 2) will make docket's tools (which include policy, decisions, process events) visible; allowing users to enable/disable them. Decision 3 includes docket policy in the configuration tab. |
| **D1, D2** (consultation pack in runner and inbox) | The decision dialog must know the harness has `decisions: Supported` before asking; pre-flight prevents asking on a harness that does not support it. |
| **E2, E3** (MRP review record and panel) | The verification checkbox lives in the configuration; users know before execution whether verification is enabled. |
| **G1** (factory metrics) | Metrics need a stable configuration to measure against; a checklist ensures configuration is intentional. |

## What this reverses or amends

Nothing in the Phase 65 or 66 Decided tables. It adds a **product** layer on top of the **architecture** layer the ADRs define. Configuration and onboarding are not architectural; they are user-facing.

## Cut and deferred

| Item | Status | Why |
|---|---|---|
| Per-project default agent profile | Deferred | Trigger: a project type system exists (education, software, ops…) that supplies a template. |
| Tool grouping (fetch/network, git, storage, etc.) | Deferred | Trigger: a tool taxonomy exists. |
| Configuration import/export | Deferred | Trigger: teams want to share setups. |
| Configuration validation by docket | Deferred | Trigger: docket publishes the `kind: policy` schema (ADR 0070 Q3). |

## Questions for the maintainer (blocking Wave 0.1.1)

1. **Does the "Setup" tab belong in the project detail, or at the workspace level (affecting all projects)?** The harness and model are per-attempt (an agent profile choice is per-execution), but the repository is per-project. Where should a user go to reconfigure their GitHub token?

2. **Should model selection be per-project, per-agent-profile, or per-execution?** Today it is per-execution (it is in the dialog). Phase 66 A1–A3 keep it per-agent-profile (the profile's `tool_policy.docket.recipe` is an example). Should a project have a default model?

3. **Should "Allowed Tools" be a project setting, a profile setting, or a run-time override?** If per-project, all profiles get the same tool deny list. If per-profile, each profile can have its own. If run-time, users can override per attempt (but that is the current experience, which is confusing).

4. **Should the pre-run checklist also validate that the item has a brief, or is that orthogonal?** Phase 66 C1–C3 make the brief an entity. Should "Run with agent" require one, or is a brief optional?

## Amendments

*(Appended by later readers, dated. The text above is never rewritten.)*

**2026-10-02 — review against `develop` at `7718420`, before the plan's re-cut.** The ADR was
written from screenshots. Read against the tree, three of its four decisions describe what
already exists or contradict a boundary another ADR draws, and one finding stands. (The
heading "blocking Wave 0.1.1" refers to no row of the plan; the row is 0.3b.)

| Decision | On the tree | Verdict |
|---|---|---|
| 1, 3 — a Setup/Configuration tab as the one place for harness, model, repository, profiles and tools | `/projects/:id/settings` exists with General, Agents, Workflow, Vocabulary, Fields, Roles and Data panels (`frontend/src/features/settings/ProjectSettings.tsx:8-14`); `/settings` is the global page; `/agents` walks harness → provider → default model → test run (`frontend/src/features/agents/steps/`); the project carries `default_model` (`crates/tack-core/src/models.rs:35`) and the book has "The four-tier precedence" for the model (`agent-runners.md:375`). | Not built: it exists. What is missing is the link from the dialog's error to that page (decision 4). |
| 2 — `HarnessDescriptor.tools: Vec<…>`, "attested in the claim response (already extended by C3)" | The descriptor is a `&'static` table of facts about a CLI (`crates/tack-runner/src/harness/local_process.rs:69-91`). Capabilities travel runner → board through `docs/contracts/runner-v1/capabilities.json` and `GET /runners`' `capability_snapshot` (`RunWithAgentModal.tsx:72-90`), not through the claim, which runs board → runner and is what C3 extends. No harness CLI has been measured to list its tools. | Not built as written. Replaced by P0 (measure first) and, where a list exists, the checklist half of P1. |
| 4 — the dialog is a pre-flight gate | The dialog already computes the gate and already disables "Ask me" when the harness does not attest `decisions` (`RunWithAgentModal.tsx:314,321,659`). What it does not do: say where each blocking state is fixed; and the repository and the tools (`:172-179`) are collected as free text at run time. | **Stands, reduced**: P1 in `docs/plans/phase-66.md`. No new tab, route or column. |
| "Related tasks", E2/E3 — "the verification checkbox lives in the configuration" | ADR 0071 decision 4: `[verify]` is a table in the runner's TOML; the board never runs or configures the verifier. | Contradicts the ADR this one says it complements. Dropped. |
| "Related tasks", D1/D2 — "pre-flight prevents asking on a harness that does not support it" | Already true (`:321`). | Dropped. |

The four questions fall away with the Setup tab. If accepted, the ADR is accepted as this
amendment: one finding, two tasks (P0, then P1), scheduled after C3 so the dialog is edited
once (plan question 5).

**2026-10-03 — accepted by the maintainer, as a priority, with this direction.** The
interface and the clarity of the whole flow come first in Phase 66. A capability that is
deferred is still shown in the form, visibly disabled, with the reason and what enables it; it
is never hidden. A verification checkbox exists from the first version of the dialog and stays
disabled until the verifier integration is complete.

What this changes in the review of 2026-10-02: its verdict "Dropped" on the verification
checkbox is reversed (see ADR 0071's amendment of the same day for how the checkbox and the
runner's TOML fit together); its scheduling "after C3" is reversed — the dialog is laid out
first (P1, Wave 2) and C3 fills the brief's slot. What stands from that review: no second
Setup tab is built, because `/projects/:id/settings`, `/settings` and `/agents` already hold
the configuration, and the dialog links to them; the tool list is measured per harness (P0)
before it becomes a checklist. Tasks P0, P1 and P2 of `docs/plans/phase-66.md`. The four
questions are answered by this direction: configuration stays where it is; the model keeps
the four-tier precedence; allowed tools stay a per-run choice with the harness's list as the
source; a brief is optional, and its row in the flow says whether the item has one.
