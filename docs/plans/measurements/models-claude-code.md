# Model ids `claude` accepts — measured 2026-10-08

Phase 67, M0. One short prompt per id, run from an empty directory with the operator's own
login. M3 reads this file.

**Version:** `claude 2.1.291 (Claude Code)`.

**Command** (per id):

```bash
claude -p "Reply with exactly the two letters OK and nothing else." --model <id> --max-turns 1 --output-format json
```

| id | Outcome | Wall | Cost (USD, as reported) |
|---|---|---|---|
| `claude-x-nope` | **rejected**, exit 1, `[claude-code:unrecognized_model]` | 5s | — |
| `claude-sonnet-5-5` | ran, `end_turn` | 5s | 0.068 |
| `claude-opus-5-5` | ran, `end_turn` | 6s | 0.130 |
| `claude-fable-5-1` | ran, `end_turn` | 7s | 0.291 |
| `claude-haiku-4-5-20251001` | ran, `end_turn` | 4s | 0.023 |
| `claude-sonnet-4-5` | ran, `end_turn` | 7s | 0.185 |
| `sonnet` (alias) | ran, `end_turn` | 4s | 0.048 |
| `opus` (alias) | ran, `end_turn` | 2s | 0.090 |
| `haiku` (alias) | ran, `end_turn` | 4s | 0.015 |

Total spend: about 0.85 USD.

**What M3 takes from this:** the CLI rejects an unknown id at once with a typed error
(`unrecognized_model`), exit 1, before any spend; the full ids and the three aliases both
work; the agent's own default (no `--model`) is what a terminal `claude` uses. The list for
the "Specific model" radio is the eight ids above; the aliases are the friendlier labels.
