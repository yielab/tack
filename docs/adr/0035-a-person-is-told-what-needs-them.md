# ADR 0035: a person is told what needs them — one inbox, fed by the board's events, delivered live and through a webhook

**Status: ACCEPTED 2026-11-04 and built.** Executed by Phase 88 (`docs/plans/phase-88.md`). Builds on
ADR 0034 decision 8 (the board's facts are events on one bus; a trigger is the bus's first
subscriber) and on decision 6 (the digest knows what waits on a person). **Replaces the outbound
webhook of Phase 10** (`item.*`, `sprint.*`, `item.due_soon`, fired from the handlers): nothing of
it is kept beside the new design.

**The ask.** When an agent asks a question, a run fails, or a change set waits, the person finds out
— on the open tab, on another tab, on the phone through a receiver of their choosing — without
watching the screen. Today nothing tells them: the web polls, the webhook never speaks of runs or
decisions, and the desktop app shows one dialog, about the server.

**The rule behind every decision.** A notification is a fact the board already announced, kept so
a person can see it later and resolved by the fact that answers it. Tack produces no new fact for
it: the bus of ADR 0034 d8 is the one source, the inbox is its second subscriber, and every way out
(the screen, the desktop, a webhook) reads the inbox. Tack stays single-operator (ADR 0004): there
is one inbox, with no routing by user.

**Decide:**

| # | Decision | What changes for whoever uses Tack |
|---|---|---|
| 1 | **One pure rule says what notifies and what closes it.** In `tack-core`, `notice(&DomainEvent) -> Option<Notice>` and `closes(&DomainEvent) -> Option<Key>`. It notifies: a decision opened (**needs you**), a run failed (**needs you**), a change set proposed (**needs you**), a run finished (**info**), an item due within the hour (**info**, from the hourly loop). An item created, edited, moved or deleted is not a notification: the board is live already. A fact the operator caused never notifies the operator. A decision resolved closes its decision's notification (also when it was answered in docket's own tool); a run started closes the failed run it continues or reruns; a change set applied or rejected closes its proposal | The inbox holds what needs you and what happened, nothing you did yourself |
| 2 | **The inbox is a table, and a key makes it exact.** `notifications(id, project_id, kind, level, key, item_id, request_id, title, cause, created_at, read_at, resolved_at)`, with a unique index on `key` where `resolved_at IS NULL`. A **needs-you** notification is open until the fact that closes it; an **info** one is unread until read. The same situation cannot be open twice, whatever the bus repeats. Resolved and read rows are forgotten by the existing retention sweep after 30 days | Answer the question anywhere — Tack, docket's CLI — and the notification closes itself |
| 3 | **The inbox is fed by the bus and checked against the store.** A subscriber (`tack-app::notifications`) applies decision 1 to every event and writes. Because the bus is in memory and lossy (a slow reader is told `Lagged`), the same module **reconciles** at start and after any `Lagged`: it reads what waits on a person from the store — the same reads the digest makes — opens what has no notification and closes what no longer waits. One function, idempotent by the key. No outbox is added to the request path | An upgrade opens a notification for every question already waiting; a lost event is repaired within a tick |
| 4 | **The person reads it on the screen, live.** `GET /api/notifications` (open, or recent), `POST …/{id}/read`, `POST …/read-all`, and one WebSocket, `GET /api/notifications/live`, that says what opened or closed and how many are open. The web shows a count in the sidebar, an inbox drawer, a toast for a needs-you that arrives while the tab is open, the count in the tab's title, and — when the person turns it on — the browser's own notification while the tab is hidden. The desktop app sends the same through the system's notifications and shows the count on the tray. The live socket replaces the 3-second poll of the questions list | You hear the question the moment it is asked, on whatever Tack is open |
| 5 | **A channel is how the inbox leaves the machine, and the webhook is the first.** `Channel { name, send(&Notification) -> Result<(), Undelivered> }` in `tack-app`. A delivery row per (notification, channel): tried when the notification opens, retried with a backoff of 30 s doubling to 6 hours, at most 8 times, then **failed** with its last error, shown in Settings. The webhook channel keeps Phase 10's transport and signature (`TACK_WEBHOOK_URL`, `TACK_WEBHOOK_SECRET`, `X-Tack-Signature`) and sends `notification.<kind>` with the notification's `id`, so a receiver deduplicates on it. **Phase 10's events are removed**: `item.created`, `item.updated`, `item.deleted`, `sprint.*` and `item.due_soon` are no longer sent, the handlers fire nothing, and `DueSoonSink` goes | A webhook receiver (ntfy, Slack, n8n) gets what needs you, at least once, signed; it no longer gets every edit |

**Why now.** Phases 85 to 87 made a run stop and wait on a person in more ways (a decision, a change
set, a step's budget), and the audit of 2026-11-03 found that the person learns of none of them
unless the tab is open and the right view is showing: the bus already emits `DecisionOpened`,
`RunFailed` and `RunFinished` (`crates/tack-app/src/runs/announce.rs`), yet its only subscriber is
the triggers; `board_message` drops every run and decision fact before the WebSocket
(`crates/tack-api/src/ports.rs`); the questions list reloads every 3 s and the runs every 4 s; the
webhook speaks only of items and sprints, from five hand-wired calls that skip the bus.

**If you do nothing.** The person is the poller. A manager profile (ADR 0034) is told of a failed
run within seconds by its trigger, and the person who must answer it is not.

## Open questions for the maintainer

1. **Does a finished run notify at all?** Proposed: yes, as **info**, because review already shows
   on the board; the alternative is **needs you**, which keeps review in the count.
2. **Is the desktop card (F) in this phase or the next?** Proposed: this phase, last round.

## Evidence (read 2026-11-03 on `develop` at `c7b84acf`)

| Fact | Where |
|---|---|
| The bus emits `RunStarted/Finished/Failed/Waiting`, `DecisionOpened/Resolved` and the four item facts; nothing emits a change set fact | `crates/tack-app/src/events.rs:28-69`, `runs/announce.rs:58-125`, `change_sets.rs` (no `emit`) |
| The only subscriber is the triggers | `crates/tack-app/src/triggers.rs:554`; `crates/tack-api/src/ports.rs:90-101` |
| The board socket drops every run and decision fact | `crates/tack-api/src/ports.rs:129-135` |
| The webhook fires from five handler sites, not from the bus; the hourly due-soon loop has its own sink trait | `handlers/items.rs:122,310,493`, `handlers/sprints.rs:115,176`, `crates/tack-app/src/due_soon.rs:26` |
| Delivery is one `tokio::spawn`, 10 s timeout, no retry, no record | `crates/tack-api/src/webhook.rs:65-92` |
| The web polls: questions every 3 s, runs every 4 s; the latter says in its own comment that no push channel exists | `frontend/src/shared/runWithAgent/DecisionInbox.tsx:21`, `frontend/src/shared/execution/realtime.ts:8-20` |
| Toasts confirm the person's own action; nothing arrives | `frontend/src/shared/ui/toast.ts` |
| The desktop app has one dialog, about the server; no notification plugin | `crates/tack-desktop/src/supervisor.rs:391-455`, `crates/tack-desktop/Cargo.toml:27-31` |
| The digest already computes what waits on a person and since when | `crates/tack-core/src/digest.rs:88-93,233-240`; `crates/tack-app/src/digest.rs` |
| The ADRs say nothing of the webhook or of telling a person | `grep -il webhook docs/adr/` → only ADR 0013, as "an outbound client, not a receiver" |

## Considered and not adopted

- **Keeping Phase 10's `item.*`/`sprint.*` events beside the new ones.** Two payload shapes on one
  URL, one of them a firehose nobody asked for since 2026. A receiver that wants every edit has the
  board socket and the API.
- **An outbox table written in the same transaction as each fact.** Exactly-once at the cost of a
  write per request and a change to every handler. The bus plus a reconcile from the store gives
  at-least-once with the key as the deduplicator, and the store is already the truth.
- **Notifications per user, watchers per item, quiet hours, grouping into digests, templates.** Tack
  is single-operator; the digest already groups; a title is one sentence the rule writes.
- **Email, Slack or ntfy channels in this phase.** The webhook reaches all three through their
  receivers. Each native channel is one file later, behind the same trait.
- **An MCP tool for the inbox.** An agent reads the digest (ADR 0034 d6) and wakes on triggers (d8);
  the inbox is the person's.
