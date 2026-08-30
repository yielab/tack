---
name: tokens
description: Measure token/context usage and compare against the recorded baseline — per-session totals, average context per API call, and cost. Use when asked how much a session/period spent, whether token usage improved, or to re-capture the baseline.
---

# Token usage report

Baseline of record: `.claude/token-baseline.md` (captured 2026-08-19, the day CLAUDE.md
shrank from ~8.8k to ~1.8k tokens). Compare against it; re-capture it (same format, new
date, keep the old file's header note) only when the user asks.

**Where the tokens actually go in this repo** is measured in `.claude/context-budget.md`
(2026-08-30): `TODO.md` whole ~184k, its active boards ~15k, all 48 handoffs ~221k,
`docs/openapi.json` ~88k. When a session's context-per-call looks high, check whether an
agent read one of those whole before looking anywhere else — that is the usual cause, and
it is a finding worth naming, not a rounding error.

## 1. Project-scoped: per-transcript totals (no dependencies)

```bash
for f in ~/.claude/projects/-home-ox-Sites-objetivosMios/*.jsonl; do
  jq -rs '[ .[] | select(type=="object") | .message.usage? // empty ]
    | { out: (map(.output_tokens // 0) | add // 0),
        in:  (map(.input_tokens // 0)  | add // 0),
        cache_r: (map(.cache_read_input_tokens // 0) | add // 0),
        turns: length }
    | "in=\(.in) out=\(.out) cache_read=\(.cache_r) api_calls=\(.turns) avg_ctx_per_call=\(if .turns>0 then (.cache_r/.turns|floor) else 0 end)"' \
    "$f" 2>/dev/null | sed "s|^|$(basename $f .jsonl | cut -c1-8) ($(date -r $f +%m-%d)): |"
done
```

## 2. All projects, per day / per session (ccusage)

```bash
npx -y ccusage@latest daily --since 20260819   # adjust date; drop --since for all
npx -y ccusage@latest session                  # per-session table incl. workflows (wf_*)
```

## 3. How to read the numbers

- **`avg_ctx_per_call` (cache_read / api_calls)** is the context-management metric: the
  average context size each API call carried. This is what CLAUDE.md slimming and
  extract-don't-read discipline push down. ~200k means every call ran a full window
  (and compaction was near).
- **`out`** tracks how much work the model generated — dominated by task size; compare
  only like-for-like tasks (a Wave card vs a similar Wave card).
- Cache-read tokens are ~10× cheaper than fresh input; cost impact of context cuts is
  smaller than the raw counts, but **window headroom** (fewer compactions) is full value.
- Workflow rows (`wf_*`) in ccusage are multi-agent runs — huge totals there are
  fan-out, not waste, but each subagent also pays the standing CLAUDE.md cost.
- A fair before/after verdict needs: same kind of task, compare `avg_ctx_per_call`
  and total tokens. Never conclude from one session.

## 4. Live, in-session

`/context` (window breakdown: how much CLAUDE.md/memory/tools occupy), `/cost` (session
spend), and the statusline (`~/.claude/statusline.sh`) shows `ctx Nk/200k (%)` per turn.
