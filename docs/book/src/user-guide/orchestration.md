# Orchestration & the Fleet View

Tack can act as the **control center for a factory of products** built by governed
agent fleets — today, that means [docket](https://github.com/yielab/docket), a
separate tool that runs pods of Lead/Implementer/Reviewer/Tester agents against a
project, with its own budgets, approvals, and audit log. This page covers what a
**control plane** is, how to register one, what the **Fleet view** shows, and — this
is the part worth reading carefully — why every dollar figure on the page says
"estimated."

This feature is **off by default** and, as shipped today, **read-only**: Tack watches
your agent fleet and mirrors what it sees. It does not yet dispatch work to it — see
[What's not here yet](#whats-not-here-yet) below.

## What a control plane is

A **control plane** is an agent-fleet backend Tack can poll for status — a running
`docket serve` instance, in practice. You can register more than one (for example,
one per environment), and you can link a control plane to a specific Tack project so
Tack knows which pod's agents are working on that project's board.

Tack never guesses at a control plane's state. A background task called the
**reconciler** polls each registered plane on an interval and records what it finds —
whether it's reachable, what version it reports, and (once you've linked a project to
it) its roster and activity. If a plane goes quiet, the Fleet view will say so rather
than showing you stale numbers as if they were current.

## Enabling orchestration

The whole feature is gated behind one setting:

| Variable | Default | Effect |
|---|---|---|
| `TACK_ORCH_ENABLE` | `false` | Turns on the reconciler background task and every `/api/control-planes`, `/api/projects/{id}/orch-link`, and `/api/fleet` route. Unset, none of it exists — the routes 404 exactly as if they were never built. |
| `TACK_ORCH_POLL_SECS` | `10` | How often (in seconds) the reconciler polls each registered plane, before per-plane backoff and jitter are applied. |

See [Configuration](configuration.md) for the full list, including the retention and
approval-token settings that later phases of this feature will use.

Set `TACK_ORCH_ENABLE=true` and restart the server to turn the feature on. Nothing
else changes until you register a control plane.

## Registering a control plane

Registration is API-only in this release — there is no settings-panel form yet, so
you'll use `curl` or a similar tool:

```bash
curl -X POST https://tack.test/api/control-planes \
  -H "Content-Type: application/json" \
  -d '{
    "name": "docket-prod",
    "base_url": "https://docket.example.com",
    "token": "<bearer token, optional>"
  }'
```

A few things worth knowing about that request:

- `kind` defaults to `"docket"` — the only backend Tack understands today — so you can
  omit it.
- `token` is the Bearer credential the control plane expects on its authenticated
  routes. It's optional: an unauthenticated docket instance works fine without one.
- **The token is write-only.** Once stored, it never comes back out of the API. Every
  response — including the one for this very request — shows `token_set: true/false`
  instead of the value, the same way Tack already handles the S3 backup secret key.
  To change it later, `PATCH` the control plane with a new `token`; to clear it,
  `PATCH` with `"token": null`; omitting the field entirely on a `PATCH` leaves the
  stored token untouched.

Once registered, the reconciler picks the plane up on its next poll tick — no restart
needed.

## Linking a project

A control plane on its own just tells Tack "this backend exists." To connect it to a
specific project's board, create a link:

```bash
curl -X PUT https://tack.test/api/projects/<project-id>/orch-link \
  -H "Content-Type: application/json" \
  -d '{
    "control_plane_id": "<control-plane-id>",
    "remote_project": "myapp"
  }'
```

`remote_project` is the project name as the control plane knows it (docket's own
project identifier, not Tack's UUID). A project has at most one link.

The link body also accepts `budget_usd` (a cap you set — not a derived figure, see
below), `blueprint`, `pipeline_file`, `auto_dispatch`, and a `status_map` object. The
`status_map` describes which of your project's own statuses should trigger dispatch
and which remote states should move an item — every status name in it is checked
against your project's actual workflow when you save, so a typo is rejected
immediately rather than silently becoming a no-op. In this release the link is a
configuration surface you can set up ahead of time; nothing yet *acts* on
`status_map` or `auto_dispatch` — that's the dispatch feature, still to come.

## The Fleet view

Once a control plane is registered and at least one project is linked, the **Fleet**
page (in the sidebar, under Workspace) shows one row per linked project:

| Column | What it shows |
|---|---|
| **Project** | The Tack project name and which control plane / kind it's linked to. |
| **Pod health** | `healthy`, `degraded`, or `unreachable`, driven entirely by the reconciler's poll history — see below. |
| **Roster** | The agents (role + model) working that pod. |
| **Last activity** | When something last happened on this project's agent tasks. |
| **Burn vs budget** | Tokens used against your configured cap, with an estimated dollar figure underneath. |
| **Gateway** | Whether the control plane's own messaging gateway is active. |
| **Approvals** | How many gated actions on this project are waiting for a human decision. |

### Reading pod health

Health is not a live check — it's the reconciler's running verdict, updated on its own
schedule:

- **Healthy** — the plane answered its last poll normally.
- **Degraded** — three consecutive poll failures. The row keeps showing its last-known
  numbers, distinguished only by the amber health chip and a "last seen" caption.
- **Unreachable** — ten consecutive poll failures. At this point the row visibly
  changes: token counts, cost, gateway state, and approval count are replaced with a
  dash and a caption explaining why, rather than a confident-looking (and wrong) zero.
  Roster is still shown, but greyed and labelled "may be out of date."

Recovery is immediate — a single successful poll takes a plane straight back to
healthy, however long the outage was.

If you kill the control plane entirely, expect this exact sequence in the Fleet view
over the next several poll intervals: healthy → degraded → unreachable, with **no**
failed request or error on the Tack side at any point. A control-plane outage is
never allowed to turn into a broken page.

### The empty and disabled states

- If `TACK_ORCH_ENABLE` is unset, the Fleet route doesn't exist, and the page says so
  plainly rather than showing an empty table.
- If orchestration is enabled but nothing is registered yet, the page explains how to
  register a control plane (the `curl` command above, in short form).

## Why every dollar figure says "estimated"

This is the single most important thing to understand about this page, so it's worth
stating directly rather than burying it in a tooltip: **Tack never sees a bill.**

docket doesn't report real spend either — no agent-fleet tool this integration talks
to does. What Tack actually receives is **token counts**: how many tokens a task used,
in and out. Tack then multiplies those counts by a pricing table it has on file and
labels the result an estimate. It is never more accurate than that table, and the
table can go stale the moment a model's price changes.

Concretely, that means:

- **Token counts are the primary figure.** They're a plain count, always trustworthy
  whenever the plane is reachable — that's real, measured data.
- **The money figure is a derived estimate**, stored as `cost_usd_estimated` and
  rendered with the literal word "estimated," plus the date of the pricing snapshot it
  was computed from, whenever that date is known.
- **A missing or stale cost figure is shown as unavailable, never as `$0.00`.** A
  confident-looking zero next to a dead control plane would read as "this pod cost
  nothing," which is worse than admitting Tack doesn't currently know.

`budget_usd` — the cap you set on a linked project — is the one dollar figure on this
page that is **not** an estimate. It's a number you typed in, not something derived
from token counts, so it's shown plainly without the "estimated" qualifier.

If you're building anything that consumes these numbers (a report, a dashboard, an
export), carry the same discipline: never present `cost_usd_estimated` as if it were
an invoiced amount.

## What's not here yet

This feature ships in stages. As of this release:

- **Dispatch does not exist.** Tack cannot yet send work to a control plane — no
  "dispatch this item" button, no auto-dispatch on status change, even though the
  `status_map`/`auto_dispatch` fields already exist on a link so you can configure
  them ahead of time. Everything on this page is Tack *observing* your fleet, not
  *driving* it.
- **Gateway status always reads "unknown."** The control plane's messaging-gateway
  bit isn't mirrored into Tack yet.
- **Roster is always empty.** Tack doesn't yet store or display individual agents —
  the column exists in the UI for when it does.
- **The cost estimate's pricing-snapshot date is always blank.** There's no pricing
  mechanism wired up yet to say *when* a rate was captured, only the estimate itself
  once dispatch history exists.
- **Run history and traces aren't mirrored into Tack yet.** The run/trace timelines
  shown elsewhere in this guide's roadmap are a later phase.
- **The Approvals column reads `0` today, honestly.** The column and its underlying
  table exist, but nothing yet ingests docket's pending approvals into them — until
  that ingestion lands, every row's approval count is a real, current `0`, not a
  placeholder masking a nonzero number.

None of these are bugs — they're honestly-reported gaps. If you see a `0`, an
`unknown`, or an empty list in one of these spots, that's Tack telling you the data
doesn't exist yet, not that your fleet is idle.

## A known gap: the control-plane token is not yet scrubbed from backups

`GET /api/backup` already strips the cloud-backup secret key and the install identity
from the downloadable snapshot before it leaves the machine. **The docket Bearer token
you register on a control plane is not yet included in that scrub** — as of this
release, a local database backup can still contain it in the `control_planes` table.
This is a known, tracked gap (the same class of issue the cloud-backup secret key had
before its own exclusion shipped), not a silent one. Until it's closed, treat any
`GET /api/backup` snapshot as sensitive if you've registered a control plane with a
token, and store/share backups accordingly.

## See also

- [Configuration](configuration.md) — every `TACK_ORCH_*` environment variable.
- [Administration & Security](administration.md) — token handling for the API token
  and the cloud-backup secret key; the control-plane token follows the same
  write-only-over-the-API discipline (see the gap noted above for backups
  specifically).
- [Roadmap](../roadmap.md) — the full Agent-Factory Control Center plan, including the
  dispatch and unit-economics phases this page will grow into.
- For the internals — the reconciler, the `ControlPlane` trait, and how to add a new
  control-plane backend — see the [developer guide](../developer/orchestration.md).
