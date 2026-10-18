# docket — what the harness is tested against

The files in this directory are **captures from a real docket**, plus a few answers that are
marked `constructed`. Captured from `docket 0.2.0b6` (operator API version `5`), source
checkout `a9f6f81c`, on 2026-10-18.

`scripts/capture-docket-fixtures.sh` makes them. It starts `docket start --dispatch` in a
throwaway `HOME` and `DOCKET_HOME` under `/var/tmp`, with no model, no provider key and an
unreachable proxy, and sends every request Tack's docket client sends. It refuses to run when
`DOCKET_HOME` is unset or is `~/.docket`, and stops when `GET /status.json` does not answer
`apiVersion: "5"`. Nothing is spent: the one task it queues names a pipeline that does not
exist, so docket refuses it at claim, before any agent turn. Rerun it when docket changes and
commit what differs.

## Files

For each answer, `<name>.json` is the body docket sent and `<name>.json.provenance` says where
it came from. A request with a body has `<name>.request.json` (or `.request.txt` when the body
is not JSON).

A provenance file starts with `captured` or `constructed`, then `key: value` lines: `route`,
`status`, `request`, `tack-sends` (whether this is a request Tack's client sends or a refusal
of one it never sends), `optional-keys` (request keys Tack does not always send), the
`command`, the `docket` version, the `date`, and `redacted`: the scratch folder, timestamps,
commit ids, task ids and the claim's process id are rewritten to fixed values. The bearer
token is read from the file docket writes and is never printed or recorded; the script fails if
it finds the token in a capture.

A `constructed` file is an answer docket only sends while a model runs or a person answers (a
finished task, a trace event, an inbox with something in it, an approval). Its keys are the ones
docket's code writes, built from a capture where one exists; its values are made up.

## What `tests/fake_docket/` does with them

- It answers each route from its file, with this run's names and numbers put in. It has the
  keys docket has and no others; `the_fake_answers_with_the_keys_of_the_captures` compares them.
- It checks a request body against the captured requests Tack sends. A key none of them has,
  or one every one of them has that this one lacks (apart from `optional-keys`), is a `400`
  naming it. docket itself ignores unknown keys, so the fake is stricter than docket here, on
  purpose: a field docket does not read is a bug in Tack that docket would not report.
- `every_route_the_client_sends_has_a_captured_file` fails when `client.rs` sends a route with
  no capture.

## What docket showed that a hand-written fake had wrong

- `POST /pods` reads `name` and `location`; the old `project` and `path` answer `400 name is
  required`. The answer still names the team `project`.
- A task's `GET /tasks/<pod>/<id>` carries the sentence in `reason` and the kind in
  `failureKind`; it has no `error` key (only a hop has).
- Every error body is `{"ok": false, "error": "<sentence>"}`; `GET /health` is
  `{"status": "ok", "apiVersion", "dispatch"}`.
- docket accepts a blank description, any priority and unknown keys without comment, and fills a
  brief out to the whole document.

## What it leaves out

- `GET /tasks/<pod>` and its `externalRef`/`status` filters, `POST .../retry`,
  `POST /pods/<p>/apply`, `GET /pods/<p>/corrections`, `/metrics`, `/runs`, `GET /approvals`,
  `POST /tasks/<id>/pregrants`, `POST /dispatch/<pod>`, `GET .../evidence`.
- The one-second hold-back of `GET /traces` (the fake's timestamps are all in the past), the
  `413` for bodies over 1 MiB, and `409` for a task that ran in place.
- `restrictions.policies` validation, `baseRef` failures other than an unresolvable ref, and
  `pipeline` resolution: the fake stores the name and never fails a task with
  `failureKind: "pipeline"` on its own; a test scripts that.
- A task's full lifecycle (running, parked on an approval or a question, done) is constructed;
  no capture runs a model.

## docket's cache figures

Read in docket's source on `develop` at `6fba7057` (2026-10-18); the keys arrived with docket
commit `da556114` ("the turn budget counts new tokens and accepts a paid final reply"). No
capture reaches a hop, so `hop-finished-cached`, `task-usage-by-model` and `task-usage-cached`
are `constructed`, each naming the source it was built from.

- A hop's `usage` is `{"input", "cached", "cacheWrite", "output"}`
  (`core/dispatch.py::_hop_usage`), and the `hop_finished` event's payload carries it as `usage`
  (the `"hop_finished"` trace write in the same file). The task's `hops[].usage` is the same block.
- `cached` is the part of `input` a cache served and `cacheWrite` the part written to one. Both are
  subsets of `input` (`core/llm.py::TokenUsage`: "`cached_tokens` is a subset of `input_tokens`
  where the endpoint reports one, and 0 where it does not ... `cache_write_tokens` is a subset
  too"), so `input` includes reads and writes. docket writes 0 both for an endpoint that
  reports no figure and for a real zero; the wire cannot tell them apart.
- docket counts `input - cached + output` (`TokenUsage.counted_tokens`): a write is counted, a
  read is not. Tack's `Usage::counted` is `tokens_in - cache_read_tokens + tokens_out` with
  `tokens_in` holding reads and writes, so mapping `input` to `tokens_in`, `cached` to
  `cache_read_tokens` and `cacheWrite` to `cache_write_tokens` makes the two counts equal.
- A finished task's `usage.byModel.<model>` sums its hops' four keys
  (`core/dispatch.py::_finalize_task`, committed at `da556114`). The task's own top-level
  `usage` carried only `input` and `output` there. Operator API 5.7.0 adds `cached` and
  `cacheWrite` to it, committed at `e187be8a`; `task-usage-cached` is that shape and
  `task-usage-by-model` the earlier one. Tack reads the top-level keys when they are there and sums `byModel` when they are not.
- Tack keeps a 0 as 0, so it never claims "not reported" for what may be a real zero; an attempt
  says "cache not reported" only when docket sends no key at all (a docket before `da556114`).
