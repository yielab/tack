# Tack MCP Server

`tack mcp` exposes a Tack instance to AI agents (Claude Code, Codex, etc.) via the
[Model Context Protocol](https://modelcontextprotocol.io). An agent can list
projects, search and read items, and create/update/move items and add comments —
all through the same HTTP API the CLI uses, so **workflow validation, WIP limits,
and parent-auto-completion still apply** and the live board updates over WebSocket.

## Transport decision (Phase 20, Task 1)

Two options were considered:

| Option | How | Verdict |
| --- | --- | --- |
| **(a) stdio sidecar** — `tack mcp` | A subcommand spawned per-agent; speaks JSON-RPC 2.0 over stdin/stdout; reaches `tack serve` over HTTP using the existing CLI client. | **Chosen for v1.** |
| (b) HTTP/SSE endpoint in `tack serve` | Mount an MCP transport inside the server. | Deferred — adds an auth surface and SSE plumbing to the server for no v1 benefit. |

**Decision:** ship the **stdio sidecar**. It is the simplest thing to wire into
Claude Code's MCP config, adds no new server-side attack surface, and reuses the
blocking `reqwest` client already in `tack-cli`. The protocol layer is a thin,
hand-rolled JSON-RPC 2.0 loop (newline-delimited messages per the MCP stdio
transport) — no heavyweight async MCP SDK is pulled into the otherwise-blocking
CLI, keeping the single binary small.

If a remote/multi-agent HTTP transport is needed later, option (b) can be added
without changing the tool definitions.

## Usage

`tack mcp` reads JSON-RPC from stdin and writes responses to stdout. It needs a
running Tack server; point it at one with the same flags/env as any CLI command:

```sh
# Defaults to http://127.0.0.1:3210, no token
tack mcp

# Explicit server + token
TACK_API_URL=http://127.0.0.1:3210 TACK_API_TOKEN=secret tack mcp
```

> **stdout is the protocol channel** — the MCP server prints only JSON-RPC. Do not
> pipe anything else into its stdin or expect human-readable output.

## Wiring into Claude Code

Add Tack to your project's `.mcp.json` (or the global Claude Code MCP config):

```json
{
  "mcpServers": {
    "tack": {
      "command": "tack",
      "args": ["mcp"],
      "env": {
        "TACK_API_URL": "http://127.0.0.1:3210",
        "TACK_API_TOKEN": "your-token-if-set"
      }
    }
  }
}
```

Then start the Tack server (`tack serve`) and the agent can call the tools below.

## Tools

| Tool | Kind | Arguments | Maps to |
| --- | --- | --- | --- |
| `list_projects` | read | — | `GET /api/projects` |
| `list_items` | read | `project_id`*, `status`, `item_type`, `assignee` | `GET /api/projects/{id}/items` |
| `get_item` | read | `id`* | `GET /api/items/{id}` |
| `search_items` | read | `query`*, `project_id` | `GET /api/search` or `/projects/{id}/search` |
| `create_item` | write | `project_id`*, `title`*, `item_type`, `priority`, `parent_id`, `assignee` | `POST /api/projects/{id}/items` |
| `update_item` | write | `id`*, `title`, `description`, `priority`, `assignee`, `status`, `due_date` | `PATCH /api/items/{id}` |
| `move_item` | write | `id`*, `status`* | `PATCH /api/items/{id}` |
| `add_comment` | write | `item_id`*, `content`*, `author` | `POST /api/items/{id}/comments` |

`*` = required. Read tools return a compact projection (id, title, type, status,
priority, assignee) to keep agent context small; `get_item` returns full detail.

## Security

The MCP server inherits the CLI's `TACK_API_TOKEN`. It has the **same access as
any API client** — there is no per-tool scoping in v1. Run it against a server you
control, and treat the token as a secret in the MCP config. Writes are validated
server-side, so an agent cannot bypass workflow rules, but it **can** create and
modify items within the projects the token can reach.
