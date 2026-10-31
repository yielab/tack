# ADR 0024: limits in tokens, and nothing sent to an agent twice

**Status: ACCEPTED 2026-10-18.** The maintainer took decisions 1 to 4 on 2026-10-18 and asked for
the work to start the same day. Executed by Phase 75 (`docs/plans/phase-75.md`). Amends ADR 0020
decision 5 (the money budget), ADR 0021 decision 7 (the dollar budget priced per model) and
ADR 0012 decision 6 (which limits reach docket).

**Amended 2026-10-19 by ADR 0027 decision 5:** decision 1's "nothing has one by default" no longer
holds. A profile with no limit runs under a provisional limit (2 000 000 tokens by default, changed in
Settings → Automation) until the board proposes one from its own runs and the person applies it.

**Decide:** that every limit Tack sets is **in tokens**, and **nothing has one by default**; that
a token counted is a **new** one (uncached input, cache writes, output), and **cache reads are
shown, never counted**; that **dollars per day goes away** and dollars are only **an estimate**;
that **every limit says who applies it and where it is changed**; that the daily limit is
**reserved when a run is admitted**; and that what Tack sends an agent **carries nothing twice**.

**Why now:** My SaaS's first runs (2026-10-18) ended on docket's per-step limit. A profile's
token limit, set in the project file, never reached the run. Cache reads were 70% to 92% of the
input and counted at full weight. A model with no price counts as free, so a dollar limit lets it
through. And the brief reached docket's model three times, while each rework round piles another
fix section on the last.

**If you do nothing:** a limit typed into a profile does nothing; a day's token count is mostly
cache that cost a tenth of its weight; the dollar limit protects only priced models; nobody can
tell from a screen which brake stopped a run; every rework round is longer than the one before.

## Decisions

| # | Decision | What a person sees |
|---|---|---|
| 1 | **A limit is in tokens, and nothing has one by default.** A profile can set tokens per run and a time limit (`limits.budgets.tokens`, `limits.timeout_seconds`, already stored); a run takes its profile's unless the run sets its own; a project can set tokens per day. An empty field is no limit, never zero. | "Token limit: 300 000 (from the profile Implementer)" or "No limit". |
| 2 | **A counted token is a new one:** uncached input + cache writes + output. Cache reads are shown beside them and never count, in the day, the run and the Factory page. One SQL fragment owns the sum; the runner counts the same way for a live limit. An agent that does not report its cache reads counts its input whole, and the screen says "cache not reported". | "48 210 tokens (+ 412 000 read from cache, not counted)". |
| 3 | **Dollars per day goes away, and dollars are an estimate.** The column, the screen field and the project file's key are removed (a file that still has it imports with a warning in the preview); a request whose `budgets` carries `cost_usd` is refused `field_removed`; Tack stops passing `--max-budget-usd`. A dollar figure reads "≈ $0.42 (estimated)", a model with no price reads "not priced", and the Factory page says how many of the models used have a price. | "≈ $3.10 (estimated) · 2 of 3 models priced". |
| 4 | **Every limit says who applies it and where it is changed.** "No limit" always names the brake that still applies; an agent that only reports its tokens says "measured, not enforced". docket applies the run's limit to the whole task; its own per-step limit is docket's setting, named as such. | "No limit set in Tack · docket stops each step at 100 000 tokens (docket's setting)" *(since ADR 0027 decision 5 a profile with no limit shows the provisional one)*. |
| 5 | **The daily limit is reserved when a run is admitted.** A run starts only if today's counted tokens, the limits of the project's open runs and its own limit fit in the day's. A run with no limit of its own, in a project with a daily limit, runs with what is left of the day as its limit. A run whose agent cannot enforce a limit can still cross the day, and its admission says so. | "Waiting: today's limit is reserved by 2 open runs (1.2M of 2M)". A bar on the Automation screen, amber at 80%. |
| 6 | **What Tack sends an agent carries nothing twice.** To docket, the brief goes once, as the structured brief docket already renders; the description no longer repeats it. A rework replaces the previous round's fix section instead of adding one. A card's comments go newest first up to 4 000 characters, with one line counting the ones left out. The sentence saying a card's text is data is said once per message, before the first fence. Claude Code gets only the tools its role names, once a capture shows what each costs. | Nothing on a screen; each rework round is the same size as the first. |
| 7 | **What docket sends its model is docket's.** Tack does not cap, trim or rewrite docket's context. It reads docket's per-class usage (cache reads and writes per model) when docket reports it, from a capture, and shows docket's per-step limit when docket's API publishes it. | The run's tokens split by kind for docket runs too. |

If you accept this table, you have accepted the ADR. Everything below is evidence.

---

## Evidence (read 2026-10-18 on `develop` at `973dcd33`)

| # | What was measured | Where |
|---|---|---|
| 1 | The project file sets `limits: { budgets: { tokens: 300000 } }` on seven profiles; the request snapshot copies the request's own `budgets` (the run dialog sends `{}`), never the profile's `limits`. | `handlers/executions.rs` (the profile snapshot and `snapshot_value`), `frontend/src/shared/runWithAgent/shared.ts` (`budgets: {}`) |
| 2 | `tokens_in` already includes cache reads and writes for Claude Code (`claude_code.rs`, "already counted in `tokens_in`"); every token query sums `tokens_in + tokens_out`. The four R0 steps: cache reads 70%–92% of the input; counted as new tokens, three of four used 19k–31k, not 100k. | `repo/budgets.rs::spent_today`, `repo/metrics.rs::attempt_totals`, `repo/rate_card.rs` |
| 3 | `usd_per_day` is priced by the rate card; a model without a price adds nothing, so the limit never stops it. `budgets.cost_usd` reaches only Claude Code. | `scheduler/wiring.rs::budget_reached`, `harness/claude_code.rs` |
| 4 | The run detail shows tokens with no limit and no word on who applies one; docket's per-step limit (`AGENT_LOOP_TOKEN_BUDGET`, 100 000, summing every call with the cache at full weight) is invisible from Tack. | docket `config.py`, `core/llm.py` |
| 5 | The daily check runs only before a claim, against what finished runs already used; three runs admitted together can each pass and together cross the day. | `scheduler/wiring.rs::budget_reached` |
| 6 | To docket the brief travels in the description's `## The brief`, again in the structured `brief`, which docket renders as `## Pre-brief`, and the title a third time as its objective. Each rework appends "What the verifier asks you to fix" to the previous round's instructions, which already carry the last one. Every comment goes in whole, each behind its own data sentence. | `handlers/executions.rs::compose_instructions`, `harness/docket/task.rs::body`, docket `core/dispatch.py`, `handlers/runner_protocol.rs::enqueue_rework` |
| 7 | 64% of the 369 263 tokens of the four steps were old tool results sent again by docket; one listing with absolute paths was 84% of a step; Sonnet through the gateway read no cache. Tack's own text was 170–270 tokens per run. | docket's `session.json` of the four steps, `agent_loop.py` |

## Alternatives not taken

- **Keep dollars per day beside tokens.** It protects only priced models and every provider
  prices differently; a token limit stops every model.
- **Count cache reads at a discount.** The discount differs by provider and changes; a token
  either costs a new read of the model or it does not.
- **A default limit for every run.** A default nobody chose is a run that stops for no reason
  the person knows; the screen names the brakes that already exist instead. *(Reversed by ADR 0027
  decision 5: a provisional limit, shown as such, replaces "no limit".)*
- **Tack trims docket's context.** Tack does not see it; the owner of the loop owns its context.

## Questions for the maintainer, with the default built

Blocking: none.

1. The comment budget: 4 000 characters, newest first.
2. The daily bar's warning: at 80%.
3. A project file that still says `usd_per_day`: imported without it, with a warning in the preview.
