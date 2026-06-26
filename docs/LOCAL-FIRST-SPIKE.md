# Spike: Local-First / Offline Support

_Status: spike (recommendation only — no implementation) · Date: 2026-06-25 · Phase 25, Task 2_

The competitive research flagged **local-first / offline** as a trend the Rust and
self-hosting community values. This spike evaluates whether Tack should invest in it
this cycle, and if so, how far. **Deliverable: a go/no-go recommendation, not code.**

## Where Tack is today

- **Server-authoritative.** The SPA talks to a single `tack serve` over HTTP +
  WebSocket. State lives in one SQLite file; the server is the only writer.
- **Online-only browser UI.** If the local server is down, the SPA can't load or
  mutate anything (README "Status & Limitations" already says so).
- **Already "local"** in the sense that matters most to the target user: your data
  is a file on your machine, no cloud required. What's missing is *offline operation
  of the browser UI* and *multi-device sync*.

## Options considered

| Option | What it buys | Cost / risk |
| --- | --- | --- |
| **A. Do nothing** | — | Zero. The current model already satisfies "your data, your machine." |
| **B. Read-only offline PWA** | SPA shell + last-loaded board viewable when the server/network is down (service worker caches the app bundle + a snapshot in IndexedDB; mutations disabled offline) | Bounded: a service worker, a cache-versioning story, and clear "offline, read-only" UI states. No server changes. |
| **C. Full local-first (CRDT) sync** | Offline read **and** write across multiple devices, merged automatically | Large. Needs a CRDT layer (e.g. Automerge/Yjs), a sync protocol, conflict semantics, and a rethink of the SQLite single-writer model — effectively a second persistence engine. |

## Analysis

- **Tack's positioning fights option C.** The product is explicitly *single server,
  single database, one active writer, solo-dev / small-team* (see PROJECT-STATUS and
  the cloud-backup design, which is snapshot replication, not live sync). CRDT
  multi-device sync solves a problem this audience mostly doesn't have, while adding
  the single biggest source of complexity a PM tool can take on.
- **SQLite is single-writer.** Real multi-device write-sync would likely mean moving
  to libSQL/Turso or layering a CRDT store beside SQLite — a strategic change, not a
  feature. Out of scope for an additive phase.
- **Option B is cheap and on-message.** "Open your board even when the server is
  briefly down" is a believable, bounded win that reinforces the local-first story
  without touching the data model. It degrades gracefully (read-only) and needs no
  conflict resolution.
- **The plaintext angle is already addressed.** Phase 25 Task 1 shipped YAML
  round-trip export/import, which delivers the *git-diffable, plaintext, portable*
  value the community asked for — independently of offline UI.

## Recommendation

- **Option C (CRDT sync): NO-GO this cycle.** High cost, misaligned with the
  single-writer / small-team positioning. Revisit only if multi-device collaboration
  becomes an explicit product goal (it would be its own epic, not a phase).
- **Option B (read-only offline PWA): CONDITIONAL-GO, low priority.** A reasonable
  small future enhancement when there's appetite. Suggested scope if pursued:
  1. Add a service worker (e.g. Vite PWA plugin) caching the app shell + static assets.
  2. Cache the last-loaded project items in IndexedDB; render them read-only when the
     API is unreachable, with an explicit "offline — viewing last sync" banner.
  3. Disable mutating UI (drag, inline edit, create) while offline; re-enable on
     reconnect. No queueing of offline writes (that's the slippery slope toward C).
- **Option A is an acceptable default.** Doing nothing here is defensible; the
  local-first promise is largely met by "single binary + your SQLite file + YAML
  export." Offline *UI* is a polish item, not a gap in the core promise.

**Net:** ship nothing further for local-first now. The YAML round-trip (Task 1)
captures the high-value, low-cost part of the trend; the offline PWA is parked as a
documented, bounded option for a future cycle, and CRDT sync is explicitly declined.
