# Tools: claude-code

Measured 2026-10-05, scratch `HOME`, `DBUS_SESSION_BUS_ADDRESS=unix:path=/nonexistent/tack-no-keychain`, zero spend (no prompt sent, no model endpoint called). The `capabilities.json` fixture (`docs/contracts/runner-v1/capabilities.json`) has no per-harness tool field: its harness rows carry only `harness_kind`, version, probe state and model combinations.

Version: `claude --version` -> `2.1.289 (Claude Code)`

## `claude --help`

No tools subcommand. It only takes tool names as input: `--tools <tools...>` ("Specify the list of available tools from the built-in set", `""` disables all, `"default"` all), `--allowedTools`, `--disallowedTools`. None prints the set.

## MCP `tools/list` against `claude mcp serve`

Command: `(initialize; notifications/initialized; {"jsonrpc":"2.0","id":2,"method":"tools/list"}) | claude mcp serve` (stdio, newline-delimited JSON-RPC; server `claude/tengu` `2.1.289`).

```
Agent, Bash, Read, Edit, Write, NotebookEdit, WebFetch, ReportFindings, WebSearch, TaskStop,
Skill, DesignSync, Artifact, EnterWorktree, ExitWorktree, SendMessage, ListAgents, Workflow,
CronCreate, CronDelete, CronList, ScheduleWakeup, ToolSearch
```

Caveats: this is the set the MCP server exposes in an empty scratch HOME and cwd, not proof of what an interactive run gets (that set varies with settings, plugins, MCP servers and feature flags; Glob and Grep, for one, are absent here). Stability across versions: only 2.1.289 is installed, so not measured. The names are the `--tools` / `--allowedTools` vocabulary.

lists its tools by `claude mcp serve` (an MCP `tools/list` handshake)
