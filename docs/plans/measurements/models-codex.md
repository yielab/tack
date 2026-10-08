# Model ids `codex` accepts — measured 2026-10-08

Phase 67, M0. One short prompt per id, run from an empty directory with the operator's own
login (a ChatGPT account). M3 reads this file.

**Version:** `codex-cli 0.149.1`. Configured default in `~/.codex/config.toml`: `gpt-6-astra`.

**Command** (per id):

```bash
codex exec --skip-git-repo-check -m <id> "Reply with exactly the two letters OK and nothing else." </dev/null
```

| id | Outcome | Wall |
|---|---|---|
| `gpt-nope` | rejected, exit 1 | 21s |
| `gpt-5` | rejected, exit 1 | 19s |
| `gpt-5-codex` | rejected, exit 1 | 18s |
| `gpt-5.1` | rejected, exit 1 | 25s |
| `gpt-5.1-codex` | rejected, exit 1 | 23s |
| `gpt-5.6-sol` | rejected, exit 1 | 15s |
| `o3` | rejected, exit 1 | 9s |
| *(none: the configured default `gpt-6-astra`)* | rejected, exit 1 | — |

Every id, including the configured default, failed with the same server reply:

```
ERROR: {"type":"error","status":400,"error":{"type":"invalid_request_error","message":"The '<id>' model is not supported when using Codex with a ChatGPT account."}}
```

and each run first logged `failed to refresh available models: timeout waiting for child
process to exit`. The CLI never rejects an id itself: it warns `Model metadata for '<id>' not
found. Defaulting to fallback metadata` and sends the request.

**What M3 takes from this:** no accepted list could be measured on this account today, and
the CLI validates nothing locally. For codex the "Specific model" radio offers no list; only
"The agent's default" and "Other…" (typed, Advanced). When the account works again, re-run
the command above with the ids the server names in its own error, and extend the table.

Spend: zero (every request was refused before a completion).
