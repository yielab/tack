# Tools: opencode

Measured 2026-10-05, scratch `HOME`, `DBUS_SESSION_BUS_ADDRESS=unix:path=/nonexistent/tack-no-keychain`, zero spend (no prompt sent, no model endpoint called). The `capabilities.json` fixture (`docs/contracts/runner-v1/capabilities.json`) has no per-harness tool field: its harness rows carry only `harness_kind`, version, probe state and model combinations.

Version: `opencode --version` -> `1.18.30`

## `opencode --help`, `opencode debug --help`, `opencode agent list`

No tools subcommand (`debug` has config, lsp, rg, file, skill, agent, paths, ...; `mcp` manages external servers only).

## `opencode debug agent build`

Prints the agent's permission rules as JSON. The distinct `permission` keys: `*`, `doom_loop`, `external_directory`, `plan_enter`, `plan_exit`, `question`, `read` (default rule `* -> allow`; `read *.env` -> ask). These are permission names, which for `read` coincide with a tool name; the output is not a list of tools, and no tool such as bash, edit or write appears.

Stability across versions: only 1.18.30 installed, not measured. No opencode doc was read for this task.

exposes no list; the names below come from `opencode debug agent build` (permission keys only): `read`, `external_directory`, `doom_loop`, `question`, `plan_enter`, `plan_exit`
