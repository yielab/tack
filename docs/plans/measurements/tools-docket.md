# Tools: docket

Measured 2026-10-05, scratch `HOME`, `DBUS_SESSION_BUS_ADDRESS=unix:path=/nonexistent/tack-no-keychain`, zero spend (no prompt sent, no model endpoint called). The `capabilities.json` fixture (`docs/contracts/runner-v1/capabilities.json`) has no per-harness tool field: its harness rows carry only `harness_kind`, version, probe state and model combinations.

No scratch venv existed at `/var/tmp/tack-measure/docket-venv`; the installed launcher `~/.local/bin/docket` (execs `~/.local/lib/docket/venv/bin/python -m docket`) was run with the scratch HOME, and only `--help` output was read.

Version: `docket --version` -> `docket 0.2.0b1`

## `docket --help`, `docket tools --help`

`docket tools` -> "No such command 'tools'". No command prints the agent's tool list. Related: `docket gates` (the tool-call gate status), `docket policies` (`test pre_tool_call <role> <text> --tool <name>`, default `bash`, simulates one named tool), `docket harness run ...` (runs a turn; not invoked).

## `docket mcp --help`

- `mcp serve` exposes docket's control plane, 13 tools: status, pods, queue, delegate, dispatch, runs, approvals_list, approvals_grant, approvals_deny, task_answer, task_pregrant, inbox, cost. These are fleet-management tools, not the agent's. (Its `tools/list` was not run: the help says it needs an optional extra.)
- `mcp servers` text names the built-in agent tools: "a remote server can never shadow bash/read/write/edit/glob/grep"; external tools register as `mcp__<name>__<tool>`.

So the built-in names come from help prose, not a machine-readable list. Stability across versions: only 0.2.0b1 measured. The source (`core/tools.py`) was not read, per the brief's limits.

exposes no list; the names below come from `docket mcp --help`: `bash`, `read`, `write`, `edit`, `glob`, `grep`
