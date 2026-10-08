# Model ids `opencode` accepts — measured 2026-10-08

Phase 67, M0. One short prompt per id, run from an empty directory with the operator's own
configuration (credentials: Anthropic via OAuth, OpenCode Zen via API key; a local
`llamacpp` provider is the configured default). M3 reads this file.

**Version:** `opencode 1.18.30`. `opencode models` lists 85 ids as `<provider>/<model>`; the
first sixteen are `opencode/claude-*` (the Zen router), then `opencode/deepseek-*`, and so
on. The list is the router's catalogue, not what the account can run.

**Command** (per id):

```bash
opencode run --model <provider>/<id> "Reply with exactly the two letters OK and nothing else." </dev/null
```

| id | Outcome | Wall |
|---|---|---|
| *(none: the configured default `llamacpp/qwen3.6-35b-uncensored`)* | **ran**, answered `OK` | 50s |
| `anthropic/claude-nope` | failed, `UnknownError: Unexpected server error` | 2s |
| `anthropic/claude-sonnet-5-5` | failed, same error | 1s |
| `anthropic/claude-opus-5-5` | failed, same error | 2s |
| `anthropic/claude-haiku-4-5-20251001` | failed, same error | 2s |
| `openai/gpt-5.1` | failed, same error | 2s |
| `opencode/claude-haiku-4-5` | id accepted by the router; failed upstream: `Insufficient account funds` | 3s |

The `anthropic/*` and `openai/*` failures are identical for the wrong id and the right ones,
so they are a provider-path failure (the OAuth session or the `openai` provider without a
key), not model validation. The Zen router validated `opencode/claude-haiku-4-5` and only the
account balance stopped it.

**What M3 takes from this:** opencode's ids are `<provider>/<model>` and the usable set depends
on which providers the operator configured; the CLI cannot tell a wrong id from an
unavailable provider. For opencode the "Specific model" radio offers the output of
`opencode models` filtered to the providers that `opencode auth list` shows as configured,
and "The agent's default" is whatever its config names. No static list is pinned.

Spend: zero on hosted providers (every request failed before a completion); the one run that
answered was the local model.
