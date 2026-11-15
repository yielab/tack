# Phase 88: a person is told what needs them

**Status: BUILT 2026-11-04.** All six cards are in `develop`; the ADR's two open questions took the proposed answers (a finished run is info; the desktop card stayed in this phase). Executes ADR 0035 (`docs/adr/0035-a-person-is-told-what-needs-them.md`):
read its decision table first; this file says only how. Phase 10's webhook is **replaced, not kept**:
every card that touches its code deletes it and leaves no compatibility path.

## How a card is worked

- Read only the files its row names, plus `CONTRIBUTING.md` §"Database" when it adds a migration.
  The ADR's evidence table has the line numbers of everything removed.
- One rule, one place: what notifies lives in `tack-core/src/notification.rs`; who writes the
  inbox lives in `tack-app/src/notifications/`; HTTP in `tack-api/src/handlers/notifications.rs`.
  A card that needs a fact from another layer calls it, never copies it.
- Run only the tests of what the card touched (`cargo nextest run -p <crate> <filter>`,
  `cd frontend && npx vitest run <path>`); the full suites and `.githooks/pre-push` run once per
  round, by the integrator.
- Each card writes its `[Unreleased]` line in `CHANGELOG.md` and its guide paragraph in the same
  change (User and Administrator guides carry no source paths or test names). No `Co-Authored-By`.
- `./scripts/regen-generated.sh` after any route or schema change (`docs/openapi.json`,
  `frontend/src/shared/api/schema.gen.ts` are generated, never edited).

## Shapes shared by every card

```rust
// tack-core/src/notification.rs
pub enum Kind { Decision, RunFailed, ChangeSet, RunFinished, DueSoon }
pub enum Level { NeedsYou, Info }
pub struct Key(String);                 // "decision:<id>" "run:<request>" "change_set:<id>" "due:<item>:<yyyy-mm-dd>"
pub struct Notice { kind, level, key, project_id, item_id: Option<Uuid>, request_id: Option<String>, title, cause: String }
pub fn notice(event: &DomainEvent) -> Option<Notice>;   // decision 1, first half
pub fn closes(event: &DomainEvent) -> Option<Key>;      // decision 1, second half
pub fn due_soon(item: &Item, now) -> Notice;             // the hourly loop's one notice
```

```rust
// tack-app/src/notifications/mod.rs
pub struct Inbox { repo, live: broadcast::Sender<Change> }    // the one write path
pub enum Change { Opened(Notification), Closed { id, key }, Read { id } }
impl Inbox { open(&Notice) -> Option<Notification>; close(&Key); read(id); read_all(); open_count(); subscribe() }
```

```sql
-- migration 105_notifications
notifications(id TEXT PK, project_id, kind, level, key, item_id NULL, request_id NULL, title, cause,
              created_at, read_at NULL, resolved_at NULL)
CREATE UNIQUE INDEX notifications_open_key ON notifications(key) WHERE resolved_at IS NULL;
notification_deliveries(notification_id, channel, attempts INT, next_at, delivered_at NULL,
                        last_error NULL, PRIMARY KEY(notification_id, channel))
```

`Inbox::open` is `INSERT … ON CONFLICT(key) WHERE resolved_at IS NULL DO NOTHING RETURNING *` and
emits `Change::Opened` only when a row came back; it also inserts one `notification_deliveries`
row per configured channel with `next_at = now`. Nothing else writes these tables.

## Cards

| Card | Crate(s) | What | Removes | Tests |
|---|---|---|---|---|
| **A** — the rule, the tables, the inbox | `tack-core`, `tack-db`, `tack-app` | `notification.rs` with the enums above and the two functions, one match arm per `Fact` (the ADR's decision 1 is the table; `principal.kind == Operator` → `None` from `notice`). Migration `105_notifications` in `tack-db/src/migrations.rs` beside `104_triggers`. `tack-db/src/repo/notifications.rs`: `insert_open`, `close_by_key`, `mark_read`, `mark_all_read`, `list(state, limit)`, `count_open`, `deliveries_due(now, limit)`, `delivery_done`, `delivery_failed(attempts, next_at, error)`. `tack-app/src/notifications/mod.rs` with `Inbox` as above; `Change` is `Serialize` with `#[serde(tag = "type")]`. Retention: one statement in `tack-app/src/retention.rs`'s tick deleting rows with `resolved_at` or `read_at` older than 30 days | — | `tack-core`: one test per row of decision 1, plus "operator-caused never notices", "resolved closes `decision:<id>`", "started closes `run:<request>`". `tack-db`: the partial index refuses a second open row and allows one after close. `tack-app`: `open` twice returns one `Some`, one `Change` |
| **B** — the subscriber and the reconcile | `tack-app` | `notifications/subscriber.rs`: `spawn(loops, inbox, events)` modelled on `triggers::spawn`/`drain` (`triggers.rs:520-580`): every 2 s drain the receiver, `notice` → `inbox.open`, `closes` → `inbox.close`; on `Lagged` and on the first pass call `reconcile`. `reconcile(repo, inbox, clock)`: pending decisions → open `decision:`; latest attempt Failed per request with no later request → open `run:`; change sets `proposed` → open `change_set:`; every open needs-you row whose situation no longer holds → close. Read with the queries `tack-app/src/digest.rs` already uses (`change_sets_waiting_since`, the decisions and failed-run reads); add none where one exists. New facts: `Fact::ChangeSetProposed { change_set_id, item_id }` and `Fact::ChangeSetSettled { change_set_id }` emitted in `change_sets.rs` `propose`, `apply`, `reject` (add the two `EventKind`s to `tack-core/src/trigger.rs` and `ALL`, so triggers see them too). `due_soon.rs`: `tick` takes `&Inbox` and calls `open(due_soon(&item, now))`; wire it in `loop_config.rs` unconditionally | `DueSoonSink` trait and the `Option<Arc<dyn DueSoonSink>>` parameter of `LoopConfig::start`; the webhook mapping in `tack-cli/src/serve.rs:72-78` | `tack-app`: an event stream opens and closes through a `RecordedEvents`-style fake; `Lagged` triggers reconcile; reconcile on a store with one pending decision and a stale open row opens one, closes one, and is a no-op the second time; `propose` emits `ChangeSetProposed`, `apply` and `reject` emit `Settled` |
| **C** — the channel and the delivery loop | `tack-app`, `tack-api`, `tack-cli` | `notifications/channel.rs`: `trait Channel { fn name(&self) -> &'static str; async fn send(&self, n: &Notification) -> Result<(), Undelivered> }`, `enum Undelivered { Retry(String), Drop(String) }` (5xx/timeout → Retry, 4xx → Drop). `notifications/webhook.rs`: move `WebhookClient`'s transport and `sign` from `tack-api/src/webhook.rs` (keep its redirect test), body `{ "event": "notification.<kind>", "notification": {…} }`, header `X-Tack-Event`. `notifications/delivery.rs`: `spawn(loops, repo, clock, channels: Vec<Arc<dyn Channel>>)` with `spawn_tick` every 10 s **and** woken by a `Notify` the `Inbox` pings on `open`: rows from `deliveries_due`, backoff `30 s · 2^attempts` capped at 6 h, `attempts >= 8` → `last_error` kept, `next_at = NULL` (failed). `loop_config.rs` builds the channel list from `webhook_url`/`webhook_secret`; `Settings → Automation` gets a read-only line per channel: pending, failed, last error (`GET /api/notifications/channels`) | `tack-api/src/webhook.rs`; `AppState.webhook` and its two `tests/common/mod.rs` lines; the five `wh.fire` blocks (`handlers/items.rs:121-131,309-319,492-502`, `handlers/sprints.rs:114-125,170-187`); `docs/book/src/admin-guide/administration.md` §Webhooks rewritten for the new payload and the retry table; `docs/CONFIG.md:59-60` rows reworded; `admin-guide/index.md:34`, `developer/index.md:133`, `developer/crate-tour.md:326,385`, `learning/axum-primer.md:177-183,248`, `user-guide/running-agents.md:18`, `user-guide/troubleshooting.md:120`, `docs/ARCHITECTURE.md:93` updated to name the delivery loop | `tack-app` with `wiremock`: a 500 schedules a retry at +30 s then +60 s; a 404 drops with its error; the 8th failure leaves `next_at NULL`; a 200 sets `delivered_at`; the body carries `notification.decision` and the `id`. `tack-api`: no test references `webhook` (grep) |
| **D** — the API and the live socket | `tack-api` | `handlers/notifications.rs`: `GET /api/notifications?state=open\|all&limit=` (default open, 50), `POST /api/notifications/{id}/read`, `POST /api/notifications/read-all`, `GET /api/notifications/channels`, and `GET /api/notifications/live`: a WebSocket on the same handshake as `handlers/websocket.rs` (`tack.v1`, token subprotocol) that sends `{ "open": n }` on connect and every `Change` with `open` after it. Extract the token handshake and the send-loop skeleton of `websocket.rs` into one helper both sockets use. Routes merged into `api` before `require_token` like the rest (`router.rs:518-539`). `Inbox` lives on `AppState` as `notifications: Inbox`. `utoipa` paths; regenerate | the `DecisionInbox` poll is **not** this card's (see E) | `tests/handlers/notifications.rs`: list/read/read-all with state transitions; `/live` receives `opened` then `closed` for one decision opened and resolved through the existing decision routes (reuse `tests/runner_protocol/decisions.rs` helpers); unauthenticated `/live` is refused like `/boards/live` |
| **E** — the web | `frontend` | `shared/realtime/socket.ts`: generalise `boardSocket.ts` into `createReconnectingSocket(url, onMessage)`; `boardSocket.ts` and a new `notificationsSocket.ts` are thin over it. `shared/state/notifications.ts`: a store holding open rows + count, fed by `GET /api/notifications` on connect and the socket after. `Sidebar.tsx`: a bell with the count; `features/notifications/InboxDrawer.tsx` on `shared/ui/Drawer.tsx`: open first, then recent, each row → the item page (`/projects/:id/items/:item`) or the change-sets panel; Read / Read all. A **needs-you** that arrives while the tab is open → `toast.warning(title)`. `Layout.tsx:51-67`: prefix `(n) ` to `document.title` when `n > 0`. `features/settings/panels/GeneralPanel.tsx`: a switch "Notify me when this tab is hidden" that asks `Notification.requestPermission()`; when on and `document.hidden`, `new Notification(title)` per needs-you (one adapter `shared/notify.ts`, which card F extends). `DecisionInbox.tsx`: delete `LIVE_RELOAD_MS` and its interval; reload on a `Change` whose `request_id` matches | the 3 s poll in `DecisionInbox.tsx:21,…`; the duplicated reconnect code in `boardSocket.ts` | Vitest: `socket.test.ts` keeps `boardSocket.test.ts`'s cases; store applies `opened`/`closed`/`read` and counts; title prefix; the Sidebar count renders and hides at 0; `DecisionInbox` reloads on a matching `Change` and not on another |
| **F** — the desktop | `tack-desktop`, `frontend` | `tauri-plugin-notification = "2"` in `Cargo.toml` and `tauri.conf.json` permissions; `shared/notify.ts` sends through `@tauri-apps/plugin-notification` when `window.__TAURI__` exists, else the browser's `Notification`. `tray.rs`: the poll already reads `/api/health`; add `GET /api/notifications?state=open&limit=0`'s count to the tooltip ("Tack · 3 need you") | — | `supervisor/tests.rs` untouched; `tray.rs` tooltip text from a pure `fn tooltip(status, open: usize) -> String` with three cases; Vitest: `notify.ts` picks the Tauri path when the global exists |

Rounds: **A**; then **B ∥ C ∥ D**; then **E ∥ F**. Each round's integrator runs
`cargo nextest run --workspace`, `cd frontend && npm test`, `.githooks/pre-push`, and
`git grep -nE 'DueSoonSink|wh\.fire|item\.due_soon|sprint\.started|LIVE_RELOAD_MS'` must print
nothing after round 2.

## Acceptance, end to end

On the live suite's docket (`onboarding-lab/e2e/`, local model): a task that asks a question opens
one notification within 2 s (`/live` shows `opened`, the sidebar shows 1, the tab title reads
`(1)`), a second identical ask opens nothing more, answering it in Tack closes it, and a
`TACK_WEBHOOK_URL` pointed at a local `nc -l` receives one signed `notification.decision` body with
the row's `id`. Restarting `tack serve` with a question still pending opens nothing new
(reconcile finds the open row). `git grep -c webhook crates/tack-api/src` is 0.

## Not in this phase

Per-user routing, watchers, quiet hours, grouping, templates, native Slack/email/ntfy channels, an
MCP tool for the inbox, exactly-once delivery (ADR 0035, "Considered and not adopted").
