# Tools: codex

Measured 2026-10-05, scratch `HOME`, `DBUS_SESSION_BUS_ADDRESS=unix:path=/nonexistent/tack-no-keychain`, zero spend (no prompt sent, no model endpoint called). The `capabilities.json` fixture (`docs/contracts/runner-v1/capabilities.json`) has no per-harness tool field: its harness rows carry only `harness_kind`, version, probe state and model combinations.

Version: `codex --version` -> `codex-cli 0.149.1`

## `codex --help`, `codex debug --help`, `codex features list`

No tools subcommand. `debug` offers `models`, `app-server`, `prompt-input`; `features list` lists feature flags, not tools. `codex debug prompt-input` renders the model-visible input list as JSON: it holds skills and instruction messages and no `"tools"` key (count 0).

## MCP `tools/list` against `codex mcp-server`

Command: `(initialize; notifications/initialized; {"jsonrpc":"2.0","id":2,"method":"tools/list"}) | codex mcp-server` (server `codex-mcp-server` `0.149.1`; it prints "`codex mcp-server` is deprecated and will be removed in a future release").

```
codex, codex-reply
```

These are the tools Codex offers an MCP client (start a session, reply to one), not the tools the agent uses inside a run (shell, apply_patch, ...). They do not answer the question. Stability across versions: only 0.149.1 installed, not measured.

exposes no list; the names below come from `codex mcp-server` and are not the agent's tools: `codex`, `codex-reply`
