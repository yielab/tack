# Security Policy

## Scope

Tack is a **self-hosted, local-first** tool designed to run on `127.0.0.1`. It has
one shared token and no per-user identities, and is not intended to be exposed to the
public internet without an authenticating reverse proxy.

## Reporting a Vulnerability

**Please do not open a public issue for security vulnerabilities.**

Report privately through **GitHub Security Advisories**, which keeps the report
confidential until a fix is released:

1. Go to <https://github.com/yielab/tack/security/advisories/new>
   (repository **Security** tab → **Report a vulnerability**).
2. Describe the issue, affected versions, and reproduction steps.

If you cannot use GitHub Security Advisories, email
**[info@yielab.com](mailto:info@yielab.com)** with the subject `[tack] security`.

Please include, where possible:

- A description of the vulnerability and its impact.
- Steps to reproduce, or a proof-of-concept.
- The affected version / commit and your configuration (bind address, whether
  you set `TACK_API_TOKEN`, reverse proxy, etc.).

### What to expect

- **Acknowledgement** within 7 days.
- An assessment and, for confirmed issues, a remediation plan with a target
  timeline shared with you.
- A coordinated disclosure: a fix is released before public details, and you are
  credited in the advisory and `CHANGELOG.md` unless you prefer to remain
  anonymous.

Please give us a reasonable window to release a fix before any public disclosure.

## Supported Versions

Only the latest release is actively maintained. Fixes land on `develop` and ship in
the next release; older tags are not backported.

| Version | Supported |
| --- | --- |
| Latest release | ✅ |
| Older releases | ❌ |

## What can reach Tack, and what the agent can reach

Tack is built for one operator on one machine, so the rules below are about keeping
other people's text and other websites away from your board and your keys. They are
checks in the code, each with a test in a suite CI requires. The full account is in the
book page [What the Agent Can Reach](docs/book/src/user-guide/what-the-agent-can-reach.md).

| # | Rule | In practice |
| --- | --- | --- |
| 1 | The API always has a token | On a loopback bind with no `TACK_API_TOKEN`, the server creates `api-token` beside the database (mode `0600`) and requires it on every `/api` request but `/api/health`. A non-loopback bind without a token is refused at startup unless `TACK_API_ALLOW_UNAUTHENTICATED_NONLOOPBACK=1`. |
| 2 | The browser holds a token it cannot read | The page sets it as the `tack_session` cookie (`HttpOnly`, `SameSite=Strict`, `Path=/api`). The CLI finds the token file on the same machine. |
| 3 | A request must name this server | A `Host` that is not loopback, the bind address or an allowed origin gets `421`. |
| 4 | Another site cannot change anything | A non-read request marked cross-site, or from a foreign `Origin`, gets `403`; a body with no declared type gets `415`; an import is never marked trusted. |
| 5 | A request does not choose the run's environment | `HOME`, `PATH`, `GIT_*`, `SSH_*`, `LD_*`, `DYLD_*`, `PYTHON*`, `NODE_OPTIONS`, `PERL5OPT`, `RUBYOPT`, `BASH_ENV`, `ENV` and `*_BASE_URL` are refused. |
| 6 | A request cannot name a secret you did not list | `env:` references are refused; a `store:` name must be in the runner's `[secrets] request_may_reference` (empty by default). |
| 7 | The server decides the profile and the repository | A request cannot supply its own agent profile, or a repository other than the project's. |
| 8 | The agent is sandboxed where the machine allows | With `bwrap`, a harness sees the workspace, a scratch home and the system directories read-only, not `~/.ssh`, `~/.tackrc`, the runner's state or the token. Where it cannot, the run says `sandbox: none` and why, and a harness that cannot ask before it acts does not run. The network is not restricted. |
| 9 | The runner's git runs none of the repository's code | No hooks, no fsmonitor, plain `ssh`, no `ext::` transport, no system config, no external diff or text conversion. |
| 10 | What is pushed is what was verified | The push goes to the URL recorded at provisioning, sends exactly the verified commit, and is refused if the repository's git configuration changed. |
| 11 | The evidence is computed from a pinned base | The base is a commit id fixed at provisioning; prill reads `prill.yaml` from it; edits to files that decide how work is checked are named in the pack. |
| 12 | The pull request waits for you | It opens when you accept the attempt, never when the run completes. |
| 13 | MCP runs ask, and card text is data | A run started through MCP always asks before each action; ids are checked; card text reaches the agent between markers saying it is data. |

Also worth knowing:

- **Web tools** off removes the agent's own web-fetch and web-search tools. It does not
  stop a shell command from reaching the network.
- Binding beyond loopback exposes the board to your network: put it behind TLS.
- The database is a single unencrypted SQLite file. Uploads are limited to 50 MB, stored
  with UUID filenames and not served as active content.
- CORS is set by `TACK_ALLOWED_ORIGINS` and defaults to localhost only.

For hardening guidance, see the
[Security checklist](docs/DEPLOYMENT-GUIDE.md#security-checklist) in the deployment
guide.
