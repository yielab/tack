# ADR 0075: what the first docket + prill run found, and what Tack changes for it

**Status: ACCEPTED 2026-10-12 (proposed 2026-10-10); decisions 1–9 executed in `develop` on 2026-10-10, decisions 10–14
on 2026-10-11 after the second run.**
Amends ADR 0074 (a run a normal person can start) and ADR 0071 (where a run's work ends up).
The test that found everything here is `frontend/e2e/docket-prill.spec.ts`, run through
`scripts/docket-prill-e2e.sh`.

**Partly superseded 2026-10-13 by ADR 0078:** decisions 3 and 7 (`[provider.openai_compatible]`, the Automation panel's `{provider, model_id}`). The rest stands.

**Decide:** that Tack treats **docket** and **opencode** as first-class harnesses everywhere it
lists harnesses, speaks **prill** as its verifier natively, lets a runner point those two
harnesses at any OpenAI-compatible endpoint, reports a run that is asking a question as
exactly that, and keeps a request's state in step with its attempt.

**Why now:** the first end-to-end run of the whole workflow with docket as the agent and prill
as the verifier, on 2026-10-10, could not be completed through the UI without six changes,
and two of them hide real state from the person watching. Every finding below was observed
on a live run, not inferred.

**If you do nothing:** docket cannot be chosen in the run dialog, never appears on the Agents
page, cannot be run against a local model at all, every verification fails with "the
verifier's pack does not parse", and a run that stops to ask reads "Running" for an hour.

## Decisions

| # | Decision | What was observed |
|---|---|---|
| 1 | **The run dialog offers every harness the chosen runner reports**, labelled by `HARNESS_KINDS` when known and by its wire name otherwise; `HARNESS_KINDS` itself gains `docket` and `opencode`, so the Agents page lists four rows and a harness without a vendor login reads "uses a provider endpoint" instead of "Not signed in — run `docket`". | A runner reporting docket; the dialog's Agent select showed codex and claude-code only, and the form was forced back onto codex. |
| 2 | **The runner speaks prill.** `[verify]` has `kind = "prill"` (default) and `kind = "command"` (the old contract). For prill the runner snapshots the workspace with its own markers kept out, writes prill's `attempt-evidence/v0` and `briefing/v1`, runs `prill verify … --json`, and translates prill's `mrp/v1` into `docs/contracts/mrp-v1`. prill's exit 1 is a pack whose decision is reject, not a failure. | The default program was `assay`, a name the verifier no longer has; the two contracts diverged after prill forked (camelCase, different decisions, `--repo … --out`), so every verification failed. A Python adapter proved the translation end to end before it was ported. |
| 3 | **A third provider, `openai_compatible`**: `[provider.openai_compatible]` with `base_url`, optional `secret`, environment equivalents, reaching the Chat Completions harnesses (docket, opencode), its catalog read from `<base>/v1/models`. | docket refuses to run without a Tack-configured endpoint; the dialog sends no environment and profiles carry none; the only configured providers were the Vercel gateway and Anthropic. A person with llama.cpp on this machine could reach docket only through `TACK_RUNNER_VERCEL_AI_GATEWAY_TEST_BASE_URL`, a test knob. |
| 4 | **Opening a decision moves the attempt and its request to `waiting_decision`**; resolving or expiring the last pending one moves them back to `running`. | A pending "Allow bash?" decision with the attempt `running` and the request `leased`; the lifecycle allowed the transition, nothing produced it; "Asking you" had no producer. |
| 5 | **The request's state follows its live attempt** through `preparing`, `running` and `waiting_decision`, not only at the end. | `GET /api/executions/{id}` answered `leased` for the whole run while the attempt was `running`. |
| 6 | **The embedded runner takes a configuration file**: `tack serve --runner-config <path>` / `TACK_LOCAL_RUNNER_CONFIG`, the same file `tack runner start --config` reads. | `EmbeddedRunnerControl` loaded its config with no file, so `[verify]`, `[git]` and `[provider.<name>]` could never apply to `tack serve --with-runner`; `docs/CONFIG.md` documented `[verify]` without saying so. |
| 7 | **The Automation panel's model list merges what runners measured** (`model_combinations`, any provider) with the static per-harness list, saving the `{provider, model_id}` pair. | For docket the list was empty and the provider was derived from `native_provider` only, so no default model could be set there. |
| 8 | **Project creation is one request**: `POST /api/projects` accepts `code_origin`, `repository`, `workspace_mode` and `default_branch` with the update route's validation. | The modal created the project, then patched the folder; a read in between saw a project with no folder. |
| 9 | **`docs/contracts/mrp-v1/schema.json`'s `signature` keeps only its `oneOf`.** | The outer `additionalProperties: false` without `properties` rejected every pack, Tack's own `ready.json` included. The Rust parser was right; the published schema was not. |

## Found by the second run (2026-10-11)

The same test, rerun against the day's docket and prill, found three more things Tack did not
show or say, one it lost, and, once the timeline was live, one it undid. All five are executed in
`develop` the same day.

| # | Decision | What was observed |
|---|---|---|
| 10 | **A running agent's steps reach its timeline as it takes them.** A grammar may report a line of its CLI's stdout as a step (`StreamSignal::Progress`); a harness whose descriptor sets `reports_progress` is read line by line on every run, not only one that can ask. The runner sends the steps as runner events every two seconds and once more when the run ends; the timeline reloads while the attempt is live. docket reports `agent.model_turn`, `agent.tool_call` (a shell command shown as the command) and `agent.tool_refused`. | A run's timeline was empty until it ended and then held one event, `attempt.terminal`: the runner had no way to carry anything else, for any harness. |
| 11 | **Every runner event takes the next sequence of its attempt**, counted in the runner's journal. | Every event went out as `sequence: 0`, and the server keeps one event per sequence, so an attempt that recorded `attempt.plan_invalid` or `attempt.evidence_failed` lost the `attempt.terminal` that followed. |
| 12 | **docket's question names the command and the reason**: `Allow bash to run "sh -c '…'"? docket's policy: 'sh' is not on the curated allowlist.`, then the model's own reason when it gave one. | The question read `Allow bash (call vdNDuh46…)?`; the person answering could not see what they were allowing. docket sends the call and its reason in `approval_requested`'s `action`. |
| 13 | **The e2e hands prill the sandbox image by its id**, which prill counts as pinned. | Every pack carried `image_not_pinned` because the script passed a local tag. |
| 14 | **A refresh updates an attempt's row in place**: the Execution tab mounts its attempt list once and keys the rows by attempt id. | Each event moves the attempt's `event_checkpoint`, so every poll brought a "new" attempt and remounted its row: "Show timeline, questions & files" closed by itself mid-answer. A heartbeat did the same every 20 seconds before this. |

Other harnesses report no steps yet: their grammars return none, and their timelines stay as they
were. Adding codex, claude-code and opencode is reading their own stream formats, not a change to
the seam.

## Not decided here

- prill's own findings (a local image can never be digest-pinned; `riskHints` always count as
  medium; `snapshot` has no exclude flag; refs left under `refs/prill/snapshots/`) belong to
  prill.
- docket's (`echo … > file` blocks a run outright without `--answers`; `stop_reason:
  invalid_output` on a policy block; a process that waits forever on stdin once the runner
  is gone) belong to docket.
- A way for a *project* to carry an endpoint or environment for its agent. Decision 3 gives
  the runner one; whether the project should choose among several is a later ADR.
