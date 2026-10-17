# ADR 0079: merge authority follows the autonomy level you set, and Tack stays one of three tools that work alone

**Status: ACCEPTED 2026-10-13, with the amendments at the end** ("Amendments agreed
2026-10-13"), which win where they disagree with the table. The plan is `docs/closed-cycles/plans/phase-72.md`. Amends ADR 0071
(its parked "automatic merge" and its signature slot), ADR 0069 (a brief has no history) and ADR
0075 (the verifier adapter). Leaves ADR 0059 (one operator) as it is and lists what that costs in
"Questions for the owner".

**Decide:** that the person who runs Tack may choose, per project and optionally per part of the
code, how much of the merge decision Tack takes from them: **L3** (every change is accepted by a
human, as today), **L4** (Tack merges by itself inside areas the person authorised, and asks a
human on failure or outside them) or **L5** (the same everywhere, kept only while the outcomes stay
good). That Tack sends a rejected change back to the agent before it asks a human; that a merge
needs a signed verdict that names the exact commit; that Tack, and only Tack, counts money; that
its budgets cover attention as well as dollars; and that what humans correct becomes a
**directive** written into the repository. That none of this makes Tack depend on the other tools
it names, or them on Tack.

**Why now:** the pieces exist and nothing joins them. The runner already runs prill, translates
its pack and shows "Ready to merge" (`crates/tack-core/src/mrp.rs:357`), a human accepts or
rejects with a reason, and the board opens the pull request when they accept (ADR 0077 decision 9).
Then it stops: no code in `crates/tack-api` or `crates/tack-runner` merges, reads CI, signs, or
sends a rejection back to the agent, so every change costs a human review whatever its risk. The
verifier has just gained what an unattended merge needs (a signed attestation bound to a commit,
a store of verdicts and outcomes, `metrics/v1`; prill ADR 0013 and 0014), and ADR 0071 parked
"automatic merge" until it had its own ADR. Tack has no users yet, so the settings can still be
chosen rather than inherited.

**If you do nothing:** the "lights-out" half of the product stays a slide. Every change waits for
a human, however small; a rejected change waits too, though the agent could fix it in minutes;
"merged and not reverted" is called PQC although it ignores CI, escaped defects and cost; what
reviewers write in their reasons is read once and lost; and the first person to script an
automatic merge around the API does it with an unsigned pack and no record of why it was allowed.

## The principles this rests on

Tack is one of three tools that are each useful alone: an orchestrator and command environment
(Tack), an agent runtime (docket, one of Tack's four harnesses) and an independent verifier that
writes a Merge-Readiness Pack (prill). Together they are meant to run a software factory whose
autonomy rises by level, and each must stay usable on its own at L3, L4 or L5. Five rules keep
them that way. The other tools are named here only as examples.

- **P1. Contracts only.** The tools meet through versioned files, command lines and exit codes.
  None imports another or shares its database.
- **P2. The adapter lives in the consumer.** Tack translates the verifier's documents into its own
  (`crates/tack-runner/src/verify/prill.rs` is the model); the verifier never names Tack.
- **P3. A missing partner means L3.** Without a verifier, Tack still works as today: a human
  decides. Nothing breaks and nothing is faked.
- **P4. The level is policy, not a code path.** One binary; a higher level is stricter rules and
  more evidence, chosen by the operator.
- **P5. One component decides a merge.** In this ecosystem that is Tack, a human, or CI. The
  verifier recommends and the runtime executes.

## Decisions

| # | Decision |
|---|---|
| 1 | **Merge authority is a setting per project and per domain.** `projects.autonomy_level` (`L3`, `L4`, `L5`, default `L3`) and a table `autonomy_domains(id, project_id, name, path_globs JSON, item_types JSON, level, created_at, updated_at, UNIQUE(project_id, name))`. A change belongs to a domain when every path in its evidence file list matches one of the domain's globs and the item's `item_type` is in `item_types` (empty means any); a change that spans domains, or matches none, takes the lowest level of those it touches, and `L3` if none. **L3** is today: a human accepts every pack. **L4**: Tack merges the pull request by itself only when all of these hold, else the item goes to the human queue with the first failing condition named: the pack's decision is `merge` (prill's `auto-merge-eligible`); the pack is signed and its attestation verifies for the exact head commit (decision 3); the change's domain is authorised at L4 or L5; CI on the head commit is green (a new reader of GitHub's combined status for the head, beside the existing poll in `github_sync.rs`); no budget of decision 5 is exceeded; and the verifier ran with the configuration layer the level requires (the stages the operator marked mandatory ran, none was `not_applicable`, because prill treats an unconfigured stage as passing). **L5**: as L4 over every domain, and the level is kept only while the outcomes of decision 4 hold (escaped-defect rate and revert count under limits the operator sets in the same panel). **Default stays `L3`; nothing in an existing project changes on upgrade.** The merge is the board's one new write to GitHub besides the pull request (`PUT /repos/{repo}/pulls/{n}/merge`, with `sha` set to the verified head so GitHub refuses a moved branch), and every automatic merge writes an `autonomy_merges(attempt_id PK, project_id, domain, level, policy_snapshot JSON, pack_artifact_id, attestation_keyid, ci_state, merged_at)` row holding the policy that allowed it. **Demotion is automatic**: a domain whose merged change is reverted or has an escaped defect (decision 4) drops one level and the item that merged is flagged; the event is on the project's activity. **Promotion is a human act** in the Automation panel, which shows the domain's counts first; a later verifier recommendation (`autonomy-policy/v1`, not yet published) may be shown beside the counts as advice and never applies itself. Surfaces: `GET/PUT /api/projects/{id}/autonomy`, `tack project autonomy`, and a read-only `get_autonomy` MCP tool. MCP gets no tool that changes a level. |
| 2 | **A rejected change goes back to the agent before it goes to a human (the locksmith loop).** When a pack's decision is `reject`, Tack creates a follow-up request on the same item with `metadata.rework_of` set (the field the Factory metrics already count as rework) and appends the pack's reasons, one per failed criterion, to the instructions inside the same fenced block ADR 0077 decision 13 uses for card text. The cap is `projects.verify_loop_max_attempts` (default `0`, which is today's behaviour: no loop) with a token and dollar budget from decision 5; when the cap or a budget is reached, or the loop is stopped by decision 5's no-progress rule, the item goes to the human queue with every attempt's pack. `human-review` never loops: it goes straight to the queue. The loop is harness-agnostic because it uses only the request's instructions; with no verifier configured there is no pack and nothing changes (P3). A human can stop it from the item. |
| 3 | **A merge needs signed evidence tied to a commit.** When the project has a signing key configured, the runner passes `--sign KEY` to `prill verify` and stages the envelope (`mrp.intoto.json`) as a second artifact (`application/vnd.in-toto+json`). Before any automatic merge Tack runs `prill attest verify --key PUB --commit <head> ENVELOPE` (a non-zero exit, or a missing envelope, means the merge does not happen). The adapter records the envelope's digest in the pack, so `signature` becomes `{"kind":"in-toto","statement_sha256":...}` (a variant `mrp.rs:197-200` and the published schema already have) instead of `{"kind":"none"}`, which is what `prill.rs:684` writes today. Unsigned packs stay valid and reviewable at L3; at L4 and L5 they are refused for automatic merging and the queue says so. Tack stores only the public key; where the private key lives is a question for the owner. |
| 4 | **Outcomes come from the verifier's store when there is one, and money comes from Tack.** (a) When the runner is configured with a store path, `prill verify` gets `--db FILE`; the board's merge, close and revert events are forwarded with `prill record outcome` (an `outcome/v1` file per event) so the verifier's agreement and escaped-defect figures match what Tack knows; a scheduled `prill poll github` may replace the title heuristic, and the heuristic stays as the fallback. Today a revert is a pull request whose title starts with `Revert` and whose body cites `#n` (`github_sync.rs:492-497`), which misses a revert made by `git revert`. (b) The Factory metrics page reads `prill metrics --db FILE` (`metrics/v1`) for counts, escalation, agreement and escaped defects when the store exists, and computes its own figures as today when it does not. (c) **Tack prices.** `settings` holds a rate card (price per million input and output tokens per model string, CI minutes, a human hour) entered by the operator; with it the page shows total cost of delivery and **PQC per dollar** per task and per repository. With no rate card it shows tokens and minutes only, as it does now (`handlers/metrics.rs:5` states that no dollar is read or summed). The verifier never prices and neither does a harness's own `cost_usd` count unless the operator opts in. (d) **PQC is redefined**: a *production-qualified change* is a pull request that passed every gate Tack applied (the pack, CI, and for an automatic merge the policy), was merged, was not reverted, and had no escaped defect inside a window (default 14 days, per project; open question). Until the window closes a merged change is shown as *pending PQC*. Today `outcomes.pqc` is `prs.merged` (`handlers/metrics.rs:283`), i.e. merged and not reverted at the time of the query, with no CI, defect or window, so the figure moves under the reader; it keeps being returned as `merged_not_reverted` and is labelled that way on the page. |
| 5 | **Budgets are a control plane of three kinds.** Per project: a **compute and money budget** (tokens and dollars per day and per item, enforced where a harness can: `budgets.tokens` reaches docket as `--max-tokens` and `budgets.cost_usd` reaches Claude Code as `--max-budget-usd`; Codex and opencode take neither, so for them Tack stops at the next attempt boundary and says it could not stop mid-run); an **operational risk budget** (the most changes that may merge automatically per day and the most in one domain, so a bad hour cannot merge fifty); and a **human attention budget** (the most items waiting in the human queue, and the most review minutes per day, measured from `mrp_reviews`). When the attention budget is exceeded Tack stops starting new attempts in that project and says why on the board; running attempts finish. The scheduler also kills a loop (decision 2) whose last two verdicts have identical reasons: low marginal progress is the signal, not the attempt count. Stored in `project_budgets(project_id PK, tokens_per_day, usd_per_day, auto_merges_per_day, queue_items_max, review_minutes_per_day)`; every field optional, none set by default. |
| 6 | **Directives: what humans correct becomes a file in the repository.** Review reasons (`mrp_reviews.reason`, `attempt_reviews.note`) and rejection reasons from packs are grouped by the operator's request, not automatically sent anywhere, into **proposed directives** in the Automation panel, each with the reasons behind it. A human approves, edits or drops a proposal; an approved one is written by Tack as a file in the target repository (default `directives/<slug>.md`, or a section of `AGENTS.md`, project setting) and arrives as a normal pull request, so it is reviewed like code and any harness reads it from the checkout. Tack may accept a directive proposal from a verifier as advice, shown in the same list with its source and never approved on its own. Grouping starts as a deterministic text match on the reason codes and the files touched; a model-written summary is a later option that needs its own measurement. Stored in `directive_proposals(id, project_id, title, body, source, state, evidence JSON, created_at, decided_at, decided_by)`. |
| 7 | **The brief stays Tack's, with a history, and its translation is tested against the verifier's schema.** `brief-v1` remains Tack's contract; the adapter translates to the verifier's `briefing/v1` and a contract test in `crates/tack-runner` validates the adapter's output against the schema file published by the verifier (`contracts/briefing-v1/schema.json`, located by a documented environment variable or workspace path, skipped with a message when absent), not against fixtures Tack copied. `item_briefs` keeps one row per item (migration 078); a new `item_brief_versions(item_id, version INTEGER, acceptance, constraints, definition_of_done, risk, created_at, PRIMARY KEY(item_id, version))` receives a row on every change and the request snapshot records the version, so a pack can name the exact brief revision (`brief_version` beside `brief_sha256`) it was checked against. |

If you accept this table, you have accepted the ADR. Everything below is evidence. Each decision is
separate in effect and can be built alone, in this order: 7, 4, 3, 2, 5, 1, 6, because 1 needs 3,
4 and 5.

---

## What this amends

- **ADR 0071**: the "Automatic merge | Parked (plan)" row is replaced by decision 1; the
  "A pull-request check carrying the pack | Deferred" trigger (a signed pack) is met by decision 3.
  The slot decision 3 of that ADR left (`signature`) gets its second variant filled. Its
  working name for the verifier ("Assay") is stale, see the last section.
- **ADR 0069**: "one row per item" holds for the current brief; decision 7 adds a version history
  without changing the entity or the contract.
- **ADR 0075** decision 2: the adapter gains `--sign` and `--db`, and a contract test against
  the published schema.
- **ADR 0059** is not amended. Decision 1 is the first feature that needs to know who acted; see
  the questions.

## Evidence (read 2026-10-13 at `cd70ada2`)

| Fact | Locator |
|---|---|
| The runner calls `prill verify --repo --evidence [--brief] --out --json` and nothing else: no `--sign`, no `--db`; exit 0 and 1 are packs, exit 2 a refusal | `crates/tack-runner/src/verify/prill.rs:126-189` |
| Prill's decision maps `auto-merge-eligible` to `Merge`, `human-review` to `Review`, anything else to `Reject` | `prill.rs:630-634` |
| `MergeDecision::Merge` is rendered "Ready to merge (<tier> risk)"; no code merges | `crates/tack-core/src/mrp.rs:357`; `rg '/merge'` in `crates/tack-api/src` finds none |
| The pack's `signature` is always `SignatureSection::None`; the enum already has `InToto { statement_sha256 }` | `prill.rs:684`; `mrp.rs:196-200` |
| One human review per attempt, reason required; `reviewed_by` is the constant `operator` | `crates/tack-db/src/migrations.rs:1725-1733` (`mrp_reviews`); `crates/tack-api/src/handlers/mrp.rs:25-26` |
| A second table, `attempt_reviews(attempt_id, verdict, note, reviewed_at)`, holds a review of an attempt without a pack | `migrations.rs:1769-1774` |
| `done_on_mrp_accepted` moves the item to Done when a pack is accepted; a rejection never moves it; accepting opens the pull request | `crates/tack-api/src/handlers/mrp.rs:195-210, 213-262`; `crates/tack-core/src/workflow.rs` (`StatusMapPolicy`) |
| `pull_requests(attempt_id PK, repo, number, url, state, opened_at, merged_at, closed_at, reverted_by_number)` | `migrations.rs:1737-1748` |
| A revert is a PR whose title starts with `Revert` and whose body cites `#n`; the cited merged PR becomes `reverted` | `crates/tack-api/src/github_sync.rs:464-498`; `crates/tack-db/src/repo/pull_requests.rs:85-100` |
| Tack reads no CI state from GitHub | `rg 'check-runs\|combined'` in `crates/tack-api/src` and `crates/tack-runner/src` finds nothing relevant |
| Factory metrics are tokens and minutes; "No dollar figure is read or summed"; rework = a request whose `metadata.rework_of` names another request, or an attempt after the first | `crates/tack-api/src/handlers/metrics.rs:1-5`; `crates/tack-db/src/repo/metrics.rs:1-5, 52-60` |
| PQC today is `prs.merged`, with `pqc_rate = merged / opened`; the doc comment says "Merged and not reverted" because a revert moves the row from `merged` to `reverted` (`pull_requests.rs:94-95`), so the count excludes it | `handlers/metrics.rs:88-91, 283-284`; `repo/metrics.rs:228-248`; `frontend/src/features/dashboard/FactoryMetrics.tsx:195-199` |
| `budgets` is a JSON field of the agent profile snapshot; docket takes `budgets.tokens` (`--max-tokens`), Claude Code `budgets.cost_usd` (`--max-budget-usd`); Codex and opencode are passed neither | `crates/tack-orch/src/execution/types.rs:289`; `harness/docket.rs:298-376`; `harness/claude_code.rs:335-338`; `harness/codex.rs:228`; `harness/opencode.rs:346` |
| A decision (consultation pack) is a row in `execution_decisions`, with `recommendation` and `viewed_at` added in 079; it is resolved through its own route and its own secret (`TACK_EXECUTION_DECISION_TOKEN`) | `migrations.rs:1531-1550, 1717-1721`; `crates/tack-api/src/handlers/decisions.rs:188-200`; `docs/book/src/user-guide/agent-runners.md:196-199` |
| `item_briefs` has `item_id` as its primary key: one row, overwritten by `PUT`, no history | `migrations.rs:1707-1715`; `crates/tack-api/src/router.rs:326` |
| Review reasons are stored and shown once on the pack panel; nothing groups, counts or reuses them (the "corrections ledger" of ADR 0071 decision 7) | `frontend/src/shared/runWithAgent/MrpPanel.tsx:258`; `rg mrp_reviews crates` outside tests: only `repo/execution.rs` and `repo/metrics.rs` (waits) |
| The MCP server has 16 tools; it can create and cancel runs and cannot review, merge or read metrics | `crates/tack-cli/src/mcp.rs:589-828` |
| One shared bearer token; every caller is one principal | ADR 0059; `crates/tack-api/src/middleware.rs:269-309` |
| prill: `verify` takes `--sign FILE` and `--db FILE`; `attest verify --key FILE [--commit SHA] ENVELOPE` refuses an envelope about another commit (`subject_mismatch`); `record outcome` takes `outcome/v1`; `poll github` records `merged`, `closed`, `reverted`, `red_ci`; `metrics` prints `metrics/v1` | `/home/ox/Sites/prill/docs/guide/commands.md`, `signing.md`, `metrics.md`; `contracts/{metrics-v1,outcome-v1,envelope-v1}/` |
| prill treats an unconfigured stage as `not_applicable`, which blocks nothing, so an unconfigured run can be `auto-merge-eligible` | `/home/ox/Sites/prill/docs/adr/0014-integration-by-contract-and-autonomy-by-policy.md` (Context) |
| Next migration number | `087` (`rg -o '"[0-9]{3}_[a-z_]+"' crates/tack-db/src/migrations.rs \| sort -u \| tail -1` gives `086_one_model_string`, re-read 2026-10-13 at `11de5f2e`) |

## Alternatives not taken

- **One brief schema shared by the three tools.** It would let a change in one tool's criteria
  reach the others without a translation. It also ties their releases together: a new criterion
  in one needs a version bump in all, and a tool used alone would carry fields for tools it never
  meets. Each tool keeps its own format and the consumer's adapter translates it (P2), tested
  against the published schema (decision 7).
- **Tack calling the verifier in-process.** Faster and one fewer process, and it breaks P1: Tack
  would link the verifier's code, share its failure modes, and could not run without it. The
  runner already runs the verifier as a command near the code and credentials, and that is the
  seam the sandbox and the redaction set (ADR 0077) are built on.
- **Promoting autonomy automatically when the numbers look good.** A verifier's recommendation is
  evidence about a sample; promotion grants a power. The human who owns the repository decides,
  with the counts in front of them. Demotion is automatic because it only takes power back.
- **A single global autonomy switch.** Simpler, and wrong for a repository where documentation can
  merge unattended and a migration folder cannot. Domains are path globs and item types, the two
  things a change already carries.
- **Letting the verifier merge.** It would be one fewer hop and violate P5: the party that judges
  would also act, and the merge would have no policy of ours behind it.

## Consequences

- Tack gains its first write that cannot be undone from the board: a merge. Everything that guards
  it (signature, commit binding, CI, budgets, the recorded policy) exists to make that one write
  explainable afterwards. A project that never leaves L3 carries none of it but a few unused
  columns.
- The board needs an automatic-merge path that runs with no browser open. Today the board acts
  only when a request arrives; the poll in `github_sync.rs` is the only background work. Decision
  1 adds a second, and the reader of CI state sits in it.
- Reviews become data. Decision 6 and the attention budget of decision 5 both read
  `mrp_reviews`; its `reason` stops being write-only.
- The Factory page stops being tokens-only when the operator enters prices, and says plainly when
  it is using Tack's own figures instead of the verifier's.
- Brief edits are no longer destructive; export and import (`handlers/export.rs`) must carry the
  versions or say they drop them.
- Every one of the three tools can still be used alone, because each integration point is a file
  or a command that is absent when the partner is (P3).

## Questions for the owner

1. **Who holds the signing key?** prill signs with an Ed25519 key kept "with the machine that
   runs `prill verify`" (its signing guide). In Tack that machine is the runner, which also runs
   the agent's workspace. Is the key a file in the runner's state directory (the agent cannot
   reach it under ADR 0077's sandbox, but anyone with the user's access can), a hardware key, or a
   separate signing service? The answer decides whether a signature proves more than "a runner of
   mine produced this".
2. **What is the escaped-defect window**, and what counts as a defect? A revert, a red CI on the
   default branch, a bug filed against the item, an incident? The proposal is 14 days and the first
   two. The same number decides when PQC stops being "pending".
3. **Is a second person needed for L4?** ADR 0059 makes Tack single-operator: `reviewed_by` is the
   constant `operator` (`handlers/mrp.rs:26`), and the decision-resolution route has its own shared
   secret. An automatic merge record can say which policy allowed it, but not which human set that
   policy. If the answer must be a name, the identity work ADR 0059 deferred comes first.
4. **Which CI signal is authoritative?** GitHub's combined status, check runs, or one named
   required check? Repositories differ, and a merge that waits on the wrong one is either unsafe or
   stuck.
5. **Should an automatic merge squash, merge or rebase**, and may it delete the branch? The
   runner pushes `tack/` branches (ADR 0071); the proposal merges with the repository's default
   method and leaves the branch.
6. **Where do the rate card's numbers come from?** Entered by hand, they go stale; read from a
   provider's usage API, they are another credential (ADR 0071 parked "dollar ingestion" for that
   reason). The proposal is by hand, dated, with the date on every figure it produces.
7. **How many reasons make a proposed directive?** Two rejections for the same code, or a human
   correction on the second attempt? Too low floods the list; too high never proposes.
8. **Is the loop's cap per item or per project per day?** Decision 2 uses both through decision
   5, which means an item can be stopped by a project limit that has nothing to do with it.
9. **Which part of the verifier's configuration is mandatory at L4?** Decision 1 says "the stages
   the operator marked mandatory ran". The operator needs a way to say which; whether that is a
   Tack setting that refuses packs with a `not_applicable` stage, or a prill configuration layer
   Tack passes with `--config`, is open and touches P4 on both sides.
10. **Does an L5 project need the human queue at all** or only a feed ("informed")? The proposal
    keeps the queue and fills it only on failure; a feed is a view over `autonomy_merges`.

## Related fixes (not decisions)

None of these is made by this ADR. Each is a defect or a stale statement found while reading.

- **The `metric` criterion is dropped and described wrongly.** `prill.rs:433-436` turns every
  `metric` criterion into a `riskHint` that says it "has no prill equivalent and was not checked".
  `briefing/v1` has one: `MetricCriterion` (`$defs/MetricCriterion` in
  `/home/ox/Sites/prill/contracts/briefing-v1/schema.json`) with required `id`, `type: "metric"`,
  `command`, `pattern` (a regular expression with exactly one capture group), `op`
  (`>=`, `<=`, `>`, `<`, `==`) and `threshold`, and optional `description` and `timeoutS`. The
  mapping is `id` to `id`, `title` to `description`, `op` `gte` to `>=`, `lte` to `<=`, `eq` to
  `==`, `threshold` to `threshold`. Tack's `brief-v1` metric (`name`, `op`, `threshold`, `unit`)
  has **no `command` and no `pattern`**, so a correct fix needs an additive change to `brief-v1`
  (optional `command` and `pattern` on a metric criterion), translates the criterion only when both
  are present, and keeps a hint for one that lacks them, worded "has no command to measure it"
  rather than "no prill equivalent". The same function drops a command criterion's `cwd` and a test
  criterion's `runner` without a hint.
- **`max_changed_files` stays a hint, correctly.** Prill has `complexity_budget` (`maxDelta`, a
  limit on added complexity), which measures something else; it has no file-count constraint.
  `prill.rs:446-450` is right to say it is "not enforced by prill".
- **The README says the API always requires a token; the middleware does not.**
  `README.md:136` ("The API requires a token") holds for the default start, which creates one
  (`crates/tack-api/src/config.rs:316-327`, called from `server.rs:73`). It does not hold with
  `TACK_API_ALLOW_UNAUTHENTICATED_NONLOOPBACK=1`, where `ensure_api_token` returns without
  creating one for a non-loopback bind (`config.rs:317`) and `require_token` lets every request
  through (`crates/tack-api/src/middleware.rs:285-292`). The comment there still describes the
  state as "pure-local mode" and says it is reached on loopback, which ADR 0077 decision 1 made
  untrue. Either the README gains the exception or the comment is corrected; ADR 0077's "always"
  is the intent.
- **Stale "Assay" naming.** ADR 0071 calls the verifier "Assay" at its decision (line 9) and in
  decision 3 (line 35); the verifier is prill (ADR 0075). Tack's own `docs/contracts/mrp-v1/`
  fixtures still carry it (`fixtures/ready.json:51,116`, and the same lines in `not-ready.json`,
  `manual-only.json` and `verifier-failed.json`). `docs/contracts/mrp-v1/README.md` does not use the
  name and needs no change; the verifier's own `contracts/mrp-v1/` has none either.

## Amendments agreed 2026-10-13

The owner's rule: each responsibility has exactly one owner among Tack, docket and prill; nothing
is computed twice and nothing is left half-built. Read against prill ADR 0014 (accepted
2026-10-13) and docket ADR 0025 (accepted with amendments 2026-10-13).

| Item | Amended to |
|---|---|
| Ownership | Tack: the brief, the loop across attempts, the merge and whether its evidence is enough (the required stages), every GitHub read and write, observing outcomes, money, the level and its domains, applying directives. prill: the verdict and the stages behind it, next steps when it gives them, signing, the outcomes store and every quality figure, proposing levels and directives. docket: running the agent and its per-run limits. The table "Who owns what" in `docs/closed-cycles/plans/phase-72.md` is the full list. |
| Decision 1, "the verifier ran with the configuration layer the level requires" | **Changed 2026-10-14:** Tack reads it off the pack. The project holds the list (`verify_require`); the runner copies the verifier's stage list into the pack (`stages`); an automatic merge needs every required stage listed as `passed`, and `not_applicable` does not count, because prill treats an unconfigured stage as passing. The merge is Tack's (P5), so whether its evidence is enough is Tack's to check, and any verifier that reports its stages will do. L4 refuses an empty list (answers question 9). prill enforcing `require` itself (ADR 0014 L-1) would make its verdict stricter and changes nothing here. |
| Decision 2, the reasons sent back | **Changed 2026-10-14:** prill's `nextSteps` (ADR 0014 L-2) when the pack has them; otherwise each failed criterion and each failed stage, with the verifier's note. A pack that names nothing to fix goes to a human. Stopping the loop is cancelling the follow-up run. |
| Decision 4(a) | The store lives on the board's host, one per repository; the board records each MRP and each outcome it observes. Tack does not run `prill poll github`: Tack is the one GitHub reader, and its revert rule gains prill's (`This reverts commit <merge sha>`). |
| Decision 4(b) | The Factory page shows prill's figures and computes none of them; without prill on the board it says so. |
| Decision 4(d) | **Cut.** PQC is prill's `qualified` (ADR 0014 L-6). Tack's current figure stays, named `merged_not_reverted`. Question 2's window is prill's. |
| Decision 5 | Four fields: `tokens_per_day`, `usd_per_day`, `auto_merges_per_day`, `queue_items_max`, enforced at the attempt boundary and in the merge check. `review_minutes_per_day` and the per-domain cap are cut. |
| Decision 6 | **Not built.** prill groups corrections and proposes directives (ADR 0014 L-7, its M7); Tack will show them, a human approves, and Tack writes the approved one into `AGENTS.md` (which docket, Codex and opencode read) through a pull request. Trigger: prill publishes `directives-proposal/v1` and a contract to receive the corrections Tack holds. Question 7 is prill's. |
| Decision 7 | No `brief_version` in the pack; the request snapshot records it. The contract test validates against `prill schema briefing` (ADR 0014 L-3). |
| Decision 1, L5 | No limits of its own: L5 is L4 over every domain, because one revert or escaped defect already demotes. |
| Decision 1, who can merge | **Three triggers, one record** (`tack_merges.trigger`): a **Merge** button on a finished task, enabled when its pull request is open and CI is green on the head commit; a project option, "merge when the task moves to Done"; and the automatic merge at L4/L5. The first two are a person's decision, so they need neither signature nor `require` nor domain; all three merge with `sha` set to the head commit. |
| Decision 1, MCP | For now a change from a run that came through MCP is merged only with the button: `tack mcp` marks its calls (`X-Tack-Client: mcp`) and its runs (`origin: "mcp"`), and neither the Done option nor L4/L5 merges them. A move to Done made over MCP never merges. Consistent with ADR 0077 (a run over MCP always asks). |
| Decision 1, when a merge cannot happen | The task card and its activity say why and what to do: CI red or still running, the token lacks `Contents: write`, branch protection, a conflict, a moved branch. Tack never asks GitHub for more permission. |
| Decision 1, where L4 works | Only for an item linked to a GitHub issue (the pull request opens only for those) and on a board whose GitHub poll runs (`lib.rs:33`, the environment token). Elsewhere the merge is held and says which. |
| Decision 1, CI (question 4) | Green means every check run completed `success`, `neutral` or `skipped` **and** the combined status is not `failure` or `pending`; no check and no status at all is not green. The combined status alone misses GitHub Actions. |
| Decisions 1 and 2, prill's M6 | **Changed 2026-10-14:** nothing waits on it. The first version held every L4 merge until a runner reported a prill that enforces `require`, and sent a rejection without `nextSteps` to a person. The owner asked why Tack needed prill when prill is not a dependency; the pack already carried what both need. Without a verifier there is no pack, and the change stays L3 (P3). |
| Decision 7, the contract test | Runs on the maintainer's machine and in `scripts/docket-prill-e2e.sh`; CI has no prill (it is private) and skips it with a line saying so. |
| Questions 1, 3, 5, 6, 8, 10 | Taken as `docs/closed-cycles/plans/phase-72.md` proposes: the signing key is a file in the runner's state; no second person; GitHub's default merge method, branch kept; the rate card by hand and dated, in `app_meta`; the loop cap per item plus the daily budgets; L5 keeps the queue, filled only on failure. |
| Not now | Tack does not fill prill's `trajectory` (ADR 0014 L-4): nothing reads it yet. Trigger: a reader asks. |

