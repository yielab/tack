# Phase 82: the task goes to docket as fields, and a run says which prompts made it

**Status: BUILT 2026-10-30** (all six cards, merged into `develop` on 2026-10-31, PR #66; proposed 2026-10-29, widened 2026-10-30). ADR 0030 (proposed) holds the decisions and
the two JSON shapes of the prompt work; ADR 0031 (proposed) holds what docket's Phases 55 and 56 ask
of Tack (a team that works in a folder, a wait, an action that leaves, a team's records, no shape
named). This plan is the cards. docket's side is shipped (its Phases 52–56 closed, `develop` at
`15e19ea9` on 2026-10-30; the live suite and `--check` ran against it); Tack
reads what it shipped and edits nothing in docket's or prill's repositories.

Six cards, sized so that one Sonnet or Haiku agent finishes each in one sitting with the files it
names and nothing else. Three rounds. Each card owns its files; the shared files (the fake docket,
`harness/docket.rs`) are split by function below.

## How every card runs

- **Model.** A, B, E and F are Sonnet (they change a design). C is Haiku: it follows the Execution
  tab's own patterns. D is Sonnet: it runs the local suite and fixes what it finds.
- **Budget.** Read only the files the card names before starting (about 25k tokens). Stop and report
  at about 150k tokens with what is done and what is left.
- **Tests while working: only what the card touched.** Each crate has one integration-test binary,
  `it`, and each file under its `tests/` is a module of it (ADR 0029):
  `cargo nextest run --workspace -E 'package(<crate>) & test(/^<module>::/)'` for a file, `test(/<name>/)`
  for a unit test. Always `--workspace` with a filterset, never `-p`; always
  `--build-jobs 4 --test-threads 4`; `CARGO_INCREMENTAL=0`; `cargo check --workspace` between
  edits; `npx vitest run <paths>` for the frontend. Never the whole suite, never `.githooks/pre-push`
  inside a card. Fix the tests the change breaks; add a test for a behaviour that changes.
- **The full suites run once per round**, on the merged tree, by whoever integrates:
  `.githooks/pre-push`, `cargo nextest run --workspace`, `npx vitest run`, Playwright on Chromium.
  A card that changes the HTTP API runs `./scripts/regen-generated.sh` before it finishes.
- **The local suite of the three** (`onboarding-lab/e2e/docket-prill-e2e.sh` and `--check`) runs
  once, inside card D, at its end. CI never needs docket or prill (Phase 81's rule): the fixtures a
  card needs in CI are written by hand from docket's spec and recaptured by D.
- **Load.** At most two cards build at once, each in its own worktree with its own build folder on
  the partition with room; builds are reniced from outside. No card opens a window.
- **Docs go with the change.** Each card updates the guide paragraph its change makes true and adds
  its line to `CHANGELOG.md`'s `[Unreleased]`. There is no docs card. Guides carry no source paths,
  test names or phase numbers.
- **Commits.** One per card, `Type: what is now true`, no attribution trailer of any kind.

## Rounds

| Round | Cards | Gate |
|---|---|---|
| 1 | **A ∥ B** (two worktrees; the fake's `mod.rs` is split: A edits only `check_task_body` and `post_task`, B only `Script`, `finish` and `task_record`; whoever merges second rebases) | full suites once on the merged tree |
| 2 | **E ∥ F** (`harness/docket.rs` is split: E edits the escalation functions and `finish`'s `blocked` arms, F `finish`'s `done` arm, the run's start and the settle; the fake: E the escalation routes, F the pod and records routes; whoever merges second rebases) | full suites once on the merged tree |
| 3 | **C ∥ D** (D's fixtures, skill and script first; its local run last, after C is merged, since it asserts what C shows) | full suites once; the local suite of the three, run by D |

| Id | Card | Model | Owns | Waits on |
|---|---|---|---|---|
| A | The item goes to docket as fields | Sonnet | `tack-protocol` (`TaskDocument`), `tack-app/src/runs/start_run.rs`, `tack-runner/src/harness/docket/task.rs`, the fake's request reader, `docket_v5.rs` (request tests), the developer guide's docket contract | — |
| B | A run keeps what each step was made of, and the verifier gets it | Sonnet | `tack-runner/src/harness/docket/wire.rs`, `docket.rs` (settle), `verify/prill.rs`, the fake's hop record, `docket_v5.rs` (record tests), the prill fixtures' README, the developer guide's adapter pages | — |
| E | A wait is not a question, and an action that leaves is seen before it is approved | Sonnet | `harness/docket.rs` (escalation functions, `finish`'s `blocked` arms), `wire.rs` (`Escalation`), `client.rs` (`escalation`), `tack-protocol` `event_kind.rs`, `tack-db` decisions (settle and expire), `DecisionInbox.tsx`, `EventTimeline.tsx`, the fake's escalation routes, `harness/docket/tests.rs`, the user guide's decisions section | — |
| F | A team may work in a folder, and a run keeps the records it wrote | Sonnet | `harness/docket/probe.rs`, `client.rs` (`ensure_pod`, records), `harness/docket.rs` (`finish`'s `done` arm, run start, settle), `engine/outcome.rs` (`fail_without_change`), `tack-protocol` `capabilities.rs` (`TeamReport`), `Teams.tsx`, the fake's pod and records routes, `docket_v5.rs` (folder and records tests), the user guide's teams section | B |
| C | The Execution tab says which prompts ran, what the agent asked and what the team recorded | Haiku | `frontend/src/shared/runWithAgent/` (`ExecutionTimeline.tsx`, `AgentWork.tsx`, `LatestRunSummary.tsx`), `shared/execution/types.ts`, the user guide's Execution tab section | B, F |
| D | The local suite proves it against the new docket | Sonnet | `onboarding-lab/e2e/` (script, spec), the recaptured fixtures under `crates/tack-runner/src/harness/fixtures/docket/`, `onboarding-lab/skill/` (`scripts/apply.sh`, `references/`), `TESTING.md`'s live-suite section | A, B, C, E, F |

## Left out, and why

- Tack writing prompt files, a pod overlay or `.docket/prompts/`; versioning the profile's
  instructions in Tack; a model class per item type; reading docket's `/metrics`; `claims` to the
  verifier; a new event kind per hop record: each is in ADR 0030 "Considered and not adopted".
- Configuring services, secrets, send quotas, MCP lifetimes or a team's network from Tack; showing
  routines; sending `variables`; a "Resume now" for a wait; reading or writing every record of a
  team; creating a folder team from Tack: each is in ADR 0031 "Considered and not adopted".
- Renaming `fixtures/docket/v5/` to `v6/`: docket's API number did not change; the folder name
  is wrong for a reason Phase 81 already accepted, and D recaptures into it.
- A measurement card. The numbers the audit asks for (tokens per layer and role, verdicts per prompt
  version, verifier results per version and class) exist once B lands; measuring them is a run on
  a model, which is the maintainer's, with the data C shows.

**Done when:** a docket run sends the card as `task` fields and never as a second copy in prose; a
settled run holds, per step, the prompts by name, version and hash with the agent's assumptions and
questions, shown on the Execution tab in words; the verifier's evidence names the family and a
trajectory; an item given to a folder team ends Succeeded with what the team did and the records it
wrote; a wait is shown as a wait; an outward approval shows its call while pending and Tack keeps no
copy after; Tack creates a team without naming a shape; the fake docket answers the same shapes as
the recaptured docket; and the local suite passes against docket's current checkout and names its
commit.

---

## Cards

### A. The item goes to docket as fields

**Model:** Sonnet. **Waits on:** nothing. Decisions 1, 2 and 5 of ADR 0030.

**Why.** `compose_instructions` (`tack-app/src/runs/start_run.rs`) renders the title, description,
comments, brief and definition of done into one prose text for every agent. For docket,
`harness/docket/task.rs::body` then sends that prose as `description`, cuts the "## The brief"
section back out by searching for the heading it knows `compose_instructions` wrote
(`BRIEF_HEADING`), re-parses the fenced `Title:` to find the objective (`objective()`), and sends
the brief a second time as `brief`. docket now takes the same facts as fields
(`task`: `objective`, `doneWhen`, `acceptance`, `constraints`, `risk`, `kind`, `paths`, `answers`),
renders them itself in a fixed order, bounds the repository map by `paths`, and refuses an unknown
key with `400`. A rerun today also re-asks what a person already answered: Tack stores the
questions and answers as run events (`escalation.asked`, `escalation.resolved`) and sends none of
them.

**Read first:** ADR 0030 ("The two shapes"); `start_run.rs` `compose_instructions`,
`with_item_context` and the `brief` field; `harness/docket/task.rs` whole; `tack-protocol`
`execution/types.rs` `ExecutionRequestSnapshot` and `AgentProfileSnapshot`; `tack-core`
`models.rs` `ItemBrief`, `AcceptanceCriterion`, `Constraint`, `Risk`; the fake's
`check_task_body` and `post_task`; `tack-db/src/repo/execution/events.rs`
`list_events_for_attempt_number`; docket's `specs/api/operator-api.spec.md` "Arguments" rows
`description`, `task`, `brief` and `pod-dispatch.spec.md` "Task brief" (read only).

**Does.**
1. `TaskDocument` in `tack-protocol`: `title`, `description` (plain text), `comments` (newest
   first, the kept ones), `brief` (the typed `ItemBrief` fields: acceptance, constraints, risk,
   definition of done), `definition_of_done` (the project's), `kind` (the item's type as the
   project's vocabulary names it), `source`. It lives on `ExecutionRequestSnapshot` as `task`, beside
   `brief`, which stays until nothing reads it (check; if `brief` is read only by `task.rs`, remove it).
2. `compose_instructions` becomes two functions: one builds the `TaskDocument` from the item, the
   comments, the brief and the project (the comment budget and the fencing rule move with it), and
   one renders a document to the prose the three other agents read, unchanged byte for byte for them
   (the existing tests of `compose_instructions` prove it).
3. `task.rs::body` builds from the document: `description` is the profile's instructions and the
   fenced item (title, description, comments) and nothing else; `task` is the document as ADR 0030
   shapes it. `objective` is the title. `doneWhen` is the brief's definition of done, else the
   project's. `acceptance` reuses `criterion()`'s sentences; `constraints` reuses `constraint()`'s.
   `risk` is the brief's. `paths` are the paths the criteria name (`file`, `absent`, a `command`'s
   `cwd`), repository-relative, deduplicated, nothing invented. A field with nothing to say is
   omitted. `BRIEF_HEADING`, `objective()` and the string surgery are deleted. `gate` is unchanged.
4. `answers`: the runner receives, in the request snapshot's `metadata.previous_answers` (set by
   `start_run` for a "Run again" of the same item), the pairs `{question, answer}` read from the
   previous attempt's `escalation.asked` and `escalation.resolved` events (the question's text and
   the label of the option chosen, or the free text given). `body` sends them as `task.answers`.
   An item with no previous attempt sends none.
5. `brief` on the POST: read docket's "Task brief" section once. If a caller's brief pre-empts the
   Lead's intake, keep sending it as today beside `task`; if it is only validated and stored, stop
   sending it. Write which in the body builder's doc comment, in one sentence, with the spec's
   section name.
6. The fake: `check_task_body` accepts `task` with exactly docket's keys and types and answers
   `400` naming an unknown key or a wrong type, as docket does; `post_task` stores it on the task
   record (`task_record` already dumps the record: B owns that function, so A adds the field to
   the stored `Task` and B's dump carries it; if B has not merged, A adds the one line to
   `task_record` and says so in its report).
7. `docket_v5.rs`: a test that the body carries `task` with the fields a full brief produces and
   omits the empty ones; a test that `description` holds no brief section and no definition of
   done; a test that a rerun sends `answers` and a first run does not; the tests of the old
   `objective()` re-parse are deleted with it.
8. The developer guide's docket contract page (`runner-contract.md` "docket") says what is sent
   as fields and what as text; `CHANGELOG.md` gets its line. `./scripts/regen-generated.sh` if a
   response type changed (the snapshot is part of the runner protocol, so expect yes).

**Tests.** `cargo nextest run --workspace -E 'package(tack-protocol) | (package(tack-app) & test(/start_run|compose/)) | (package(tack-runner) & test(/^docket_v5::/))'`.

**Done when:** a docket run's `description` holds no brief and no definition of done; `task`
carries them as fields with nothing invented and nothing empty; the three other agents receive the
same prose as before; a rerun carries the previous answers; and the fake refuses what docket refuses.

---

### B. A run keeps what each step was made of, and the verifier gets it

**Model:** Sonnet. **Waits on:** nothing. Decisions 3 and 4 of ADR 0030.

**Why.** Tack reads two keys of each hop on the task record (`wire.rs::TaskHop`: `role`, `model`)
and the trace's `hop_finished` for the timeline line. docket now records on every hop a
`promptManifest` (prompts by layer with name, version, sha256, scope and token estimate; the
model's family, class and profile) and an `artifact` whose `handoff` call may carry `assumptions`
and `questions`. None of it reaches Tack, so a run cannot be told apart by the prompt version that
made it, and the verifier's evidence names only a model (`verify/prill.rs::attempt_evidence`:
`generator.model`), while prill's `attempt-evidence/v0` takes `generator.family` and a
`trajectory {uri, digest}`.

**Read first:** ADR 0030 ("The two shapes", decisions 3 and 4); `wire.rs` `TaskView`, `TaskHop`,
`Tokens`; `docket.rs` `settle_trace`, `TraceRelay`, the place the run's report is built after the
task settles (search `RunReport`), `CacheFigures`; `verify/prill.rs` `attempt_evidence` and how an
artifact is stored and given a URL (search the artifact store's `put`); the fake's `Script`, `finish`
and `task_record`; `fixtures/docket/v5/README.md`; docket's `operator-api.spec.md` paragraph on
`promptManifest` and `pod-dispatch.spec.md` "Prompt manifest" (read only); prill's
`contracts/attempt-evidence-v0/schema.json` `Generator` and `Trajectory` (read only).

**Does.**
1. `wire.rs`: `TaskHop` gains `ok`, `verdict`, `prompt_manifest: Option<PromptManifest>`
   (`layers: Vec<PromptLayer {layer, name, version, sha256, scope, tokens}>`, `model`, `family`,
   `class`, `profile`) and `artifact: Option<HopArtifact {summary, verdict, assumptions, questions}>`
   (the step's `summary` goes into `steps`: F reads the last one as a folder team's result),
   every new field `#[serde(default)]` because docket omits them on an older hop and an empty
   manifest. `#[serde(deny_unknown_fields)]` nowhere: docket adds keys in minor versions.
2. `docket.rs`: when the task is seen ended and the trace has settled, the adapter reads the record
   once and builds `steps` as ADR 0030 shapes it (one entry per hop, in order; `prompts: []` for a
   hop with no manifest), and puts it in the run's structured result beside what is there. The
   timeline lines from the trace are unchanged.
3. `verify/prill.rs`: `generator` gains `family` from the last implementer hop's manifest when it
   has one; `trajectory` is `steps` written as one JSON artifact of the attempt (the same store the
   pack and the diff use), referenced by its URL as `uri` and its `sha256:` digest. No `claims`.
   A run with no step recorded sends neither and the evidence stays as today.
4. The fake: a `Script`'s `hop_finished` entries may carry `promptManifest` and `artifact`
   (`assumptions`, `questions`), and `finish` copies them onto the record's hop; `task_record`
   dumps them when present and omits them when absent, as docket does. A fixture
   `task-with-manifest.json` written by hand from docket's spec paragraph, with its `.provenance`
   saying "from the spec, recaptured by D".
5. `docket_v5.rs`: a test that a settled run's result holds `steps` with the prompts by layer and
   the questions; a test that a hop without a manifest gives `prompts: []`; a prill test that the
   evidence carries `family` and a `trajectory` whose digest matches the stored artifact, and one
   that a run with no steps sends neither.
6. The developer guide: `runs-and-teams.md` "The docket adapter" says what is read from the record
   at settle and "The fake docket" says the script may carry a manifest; the prill fixtures'
   README says which commit of prill's schema the evidence follows. `CHANGELOG.md` line.
   `./scripts/regen-generated.sh` if a protocol type changed.

**Tests.** `cargo nextest run --workspace -E 'package(tack-runner) & test(/^docket_v5::|prill/)'`.

**Done when:** a settled docket run's result holds every hop's prompts by name, version, hash and
scope with the agent's assumptions and questions; the verifier's evidence carries the family and a
trajectory whose digest is the stored artifact's; and the fake writes the same shape as docket's spec.

---

### E. A wait is not a question, and an action that leaves is seen before it is approved

**Model:** Sonnet. **Waits on:** nothing (Round 2 so that it does not share `docket.rs` with B).
Decisions 2 and 3 of ADR 0031.

**Why.** `KNOWN_KINDS` in `harness/docket.rs` lists `approval`, `gate`, `check_changed`, `consult`
and `input`. docket's Phase 55 added a sixth, `wait`: a pipeline step parks the task until a time,
the one option is `resume`, nobody is asked. Tack turns it into an `escalation.unreadable` line
saying "answer it in docket", the opposite of what docket says. The same phase made `send` a tool
kind: an approval of a call that leaves the machine is marked `outward`, and its whole call, with
credentials removed, is readable only from `GET /escalations/<id>` and only while pending. Tack reads
`GET /escalations`, which never carries it, so a person approves an email or a write to a service
seeing only a redacted, cut prompt.

**Read first:** ADR 0031 (decisions 2 and 3, "What docket shipped"); `harness/docket.rs`
`KNOWN_KINDS`, `REASON_KINDS`, `escalation_question`, `read_escalations`, the loop that sends
questions and reports `unaskable`, `waiting_on_a_person`, `finish`'s `blocked` arms;
`harness/docket/tests.rs`; `wire.rs` `Escalation`; `client.rs` `escalations`; `tack-protocol`
`event_kind.rs`; `tack-db/src/repo/execution/decisions.rs` (`settle_execution_decision`, the two
expiry updates); `DecisionInbox.tsx` and `EventTimeline.tsx` with their tests; the fake's
escalation routes; docket's `operator-api.spec.md` "Wait" and the `GET /escalations/<id>` paragraph,
`operator-loop.spec.md` area 11 items 2, 6 and 16 (read only).

**Does.**
1. `wire.rs`: `Escalation` gains `outward: bool` (`#[serde(default)]`); a `client.rs` method
   `escalation(id)` reads `GET /escalations/<id>` into the view plus `call: Option<String>`.
2. A pending `wait` is never a question and never `unaskable`. The first read of it sends one
   progress line of a new kind `escalation.waiting` (`event_kind.rs`), "The team waits until <local
   time of `expiresAt`>. Nobody needs to answer.", once per escalation id. The deadline already
   stops while docket's task is `waiting_input`; a test proves it for a wait. `blockedReason:
   "wait_expired"` ends the attempt Failed with code `wait_expired` and "The team's wait ran out and
   its pipeline does not say what comes next." `EventTimeline.tsx` renders the new kind as a plain
   line with a clock icon, as it renders `escalation.expired`.
3. An `approval` with `outward` is read once more by id before it is asked. With a `call`, the
   question's `metadata` carries `call` and `outward: true`; `DecisionInbox.tsx` titles it "This
   action leaves the team" (else as today) and shows the call in a monospaced block above the
   options, uncut, scrolling past 20 lines. Without a `call` (an error, a `404`, or docket returned
   none) it is `unaskable` with "docket did not show the call that would leave; answer it in docket,
   where it is shown".
4. `decisions.rs`: every update that settles or expires a decision also removes `call` from its
   `metadata` (`json_remove(metadata, '$.call')`) in the same statement. The `escalation.asked`
   event's text is the question's prompt, never the call. A test reads the decision row and the
   attempt's events after an answer and after an expiry and finds no byte of the call.
5. The fake: `GET /escalations/<id>` answers the record plus `call` while pending and without it
   after; a `Script` may open a `wait` and an `outward` approval. Fixtures `escalation-wait.json` and
   `escalation-outward.json` written by hand from docket's spec, `.provenance` "from the spec".
6. Tests in `harness/docket/tests.rs` (a wait is no question and gives one line; an outward approval
   with a call carries it; one without a call is unaskable with the sentence) and in `docket_v5.rs`
   (a run through a wait is not cut by its time limit; `wait_expired`'s reason; the call gone after
   the answer). `DecisionInbox.test.tsx`: the title and the block. The user guide's decisions section
   says what an action that leaves shows and that Tack keeps no copy; `CHANGELOG.md` line.
   `./scripts/regen-generated.sh` if the decision's API type changed.

**Tests.** `cargo nextest run --workspace -E '(package(tack-runner) & test(/^docket_v5::|harness::docket::tests/)) | (package(tack-db) & test(/decision/))'`;
`npx vitest run src/shared/runWithAgent/DecisionInbox.test.tsx src/shared/runWithAgent/EventTimeline.test.tsx`.

**Done when:** a wait step reads as a wait with its time and never as something to answer; a run is
not cut while it waits; a person approving an action that leaves sees the whole call; and once the
decision settles, no row, event or file of Tack holds the call.

---

### F. A team may work in a folder, and a run keeps the records it wrote

**Model:** Sonnet. **Waits on:** B (it reads the `steps` B builds and adds `records` beside them).
Decisions 1, 4 and 5 of ADR 0031.

**Why.** docket's Phase 55 lets a pod work in a folder instead of a repository (`codebase` is `""`
on `GET /pods/<p>`, the task record has no `worktree`). Tack's `finish` reads the branch from the
worktree, finds none, and fails every such run with `branch_not_applied`. Phase 56 gives a pod
records (declared tables, each row stamped with the `_task` that wrote it, read over
`GET /pods/<p>/records[/<set>]`), which Tack shows nowhere. And `client.rs::ensure_pod` sends
`blueprint: "software"`, a name docket's P56-21 deletes.

**Read first:** ADR 0031 (decisions 1, 4, 5, "What docket shipped"); `harness/docket/probe.rs`
(`team`, `teams`, `PodReport`); `client.rs` `ensure_pod`, `pod`; `harness/docket.rs` `finish`
(the `done` arm), where the run's start info is built (`RunInfo`), the settle B added;
`engine/outcome.rs` `fail_without_change` and its tests; `tack-app/src/autonomy.rs` where a merge is
proposed from `head_commit`; `tack-protocol` `capabilities.rs` `TeamReport`; `Teams.tsx` and its
test in `panels.test.tsx`; the fake's pod routes; docket's `pod-records.spec.md` requirements 33 and
50-53 and `serve-read-api.spec.md` "POST /pods" (read only).

**Does.**
1. `ensure_pod` sends no `blueprint`. A `docket_v5.rs` test asserts the body has no such key.
2. `TeamReport` gains `works_in: Option<WorksIn>` (`repository` | `folder`, from `codebase`; absent
   when docket did not say) and `records: Option<Vec<TeamRecordSet { name, description, rows }>>`
   from `GET /pods/<p>/records` (absent on a `404`, an older docket). `Teams.tsx`: a folder team's row
   says "Works in a folder, not in this repository"; a team with record sets lists them as
   "leads · 2 rows". `./scripts/regen-generated.sh`.
3. The run's start reads the pod once (`GET /pods/<p>`) and keeps whether it has a repository. In
   `finish`, `done` with no worktree on a folder team is Succeeded with `{"code": "done",
   "outside_repository": true, "team", "task", "summary": <the last step's summary>}`; on a team with
   a repository it stays `branch_not_applied`. `fail_without_change` leaves an outcome with
   `outside_repository` succeeded; a test proves no merge and no pull request is proposed for it.
4. At settle, beside `steps`: for each record set the pod declares, `GET
   /pods/<p>/records/<set>?limit=50&sql=SELECT * FROM <set> WHERE _task = '<task>'`, only after
   checking the task id and the set name are docket's id characters (letters, digits, `-`, `_`), else
   that set is skipped. Kept as `records: [{set, columns, rows, cut}]`, sets with no row left out,
   `records` absent when none. A set the store refuses (`400`) is skipped with a warning in the log.
5. The fake: a pod may have `codebase: ""` and a task done with no worktree; `GET /pods/<p>/records`
   and `/records/<set>` answer from rows a `Script` declares, honouring the `_task` filter. Fixtures
   `pod-folder.json`, `records.json`, `records-rows.json` by hand from the spec, recaptured by D.
6. Tests in `docket_v5.rs`: a folder team's run succeeds with the summary and proposes no merge; a
   repository team's run without a worktree still fails; `records` holds only the task's rows and a
   cut set says `cut`; `TeamReport` on an older docket has neither key. `panels.test.tsx`: the two
   lines. The user guide's teams section says what a folder team is and what its records show;
   `CHANGELOG.md` line.

**Tests.** `cargo nextest run --workspace -E '(package(tack-runner) & test(/^docket_v5::|outcome/)) | package(tack-protocol) | (package(tack-app) & test(/autonomy/))'`;
`npx vitest run src/features/settings/panels/panels.test.tsx`.

**Done when:** an item given to a folder team ends Succeeded with what the team did and nothing to
merge; a run's result holds the rows the team wrote in it and no other; the Teams panel says which
teams work in a folder and what they keep; and Tack creates a team without naming a shape.

---

### C. The Execution tab says which prompts ran, what the agent asked and what the team recorded

**Model:** Haiku. **Waits on:** B and F (reads the `steps` and `records` shapes; their hand-written
fixtures are the test data). Decision 3 of ADR 0030 and decisions 1 and 4 of ADR 0031, the screen half.

**Why.** The Execution tab shows what the agent concluded, what it did by tool and by time, and what
it got (sandbox, variables, working directory). It cannot show which prompt a step ran, at which
version, nor what the agent assumed or asked, because the run did not carry it until B.

**Read first:** `ExecutionTimeline.tsx`, `AgentWork.tsx`, `LatestRunSummary.tsx`,
`shared/execution/types.ts` and `attempts.ts` (where the attempt's result is read), their tests;
the user guide `items.md` "Execution tab"; ADR 0030 "The two shapes" (`steps`); Phase 81 card S's
rule for screen text (plain words, no variable names, no commands, the verifier never named).

**Does.**
1. `types.ts`: the `steps` type as ADR 0030 shapes it and `records` as F stores it, both optional
   on an attempt.
2. In the Execution tab, one section **What each step ran** after "What the agent did": one row per
   step with the role in words (Lead, Implementer, Reviewer, or the custom role's name, capitalised;
   docket deleted `critic` and `tester`, so no word is written for them),
   the model and its class in words ("a small model", "a strong model"), the verdict when there is
   one, and the prompts as "identity · task · hop" chips reading `name version` with the scope in
   plain words ("shipped with docket", "the team's", "this repository's", "your settings"); the hash
   is shown shortened on the chip's title and copied whole on click, never in prose. A step with
   `prompts: []` reads "This step recorded no prompt." An attempt without `steps` shows no section.
3. Under each step, **The agent assumed** and **The agent asked**, each a list, only when present.
   On the Last run card, one line "The agent asked N question(s)" linking to the tab when any step
   has questions; none otherwise.
4. **What the team recorded**, after "What each step ran", only when `records` has rows: one small
   table per record set, its columns as docket names them minus the ones starting with `_`, and
   under a table whose `cut` is set "More rows are kept by the team in docket." On the Last run card
   of a run whose result says `outside_repository`, the line "This team works in a folder: nothing to
   merge" and the last step's summary as what it did, in place of the branch and the change.
5. The user guide's Execution tab section gains a paragraph on the new sections, in the guide's
   words (no field names). `CHANGELOG.md` line.

**Tests.** `npx vitest run src/shared/runWithAgent/ExecutionTimeline.test.tsx src/shared/runWithAgent/LatestRunSummary.test.tsx src/shared/execution`:
a run with steps, a step without prompts, a run without steps, the questions line, a run with
records and a cut set, a folder team's run.

**Done when:** a person can read on the Execution tab which prompts, at which version, each step of
a run used, what the agent assumed and asked, and what the team recorded, in words; a folder team's
run reads as done with nothing to merge; and a run from before this phase shows the tab exactly as
before.

---

### D. The local suite proves it against the new docket

**Model:** Sonnet. **Waits on:** A, B, E and F merged; C merged before its last step, the local run
(it asserts what they send and show).

**Why.** The live suite (`onboarding-lab/e2e/docket-prill-e2e.sh` with `docket-prill.spec.ts`,
fifteen tests) last ran against docket `52c22f89`, before docket's Phases 52–54. Its fixtures under
`crates/tack-runner/src/harness/fixtures/docket/v5/` and the fake were captured from that docket.
The onboarding skill's `apply.sh` runs `docket init`, `docket pod validate` and `docket pod update`
against a home whose layout docket changed (authored configuration under `~/.docket/config/`,
state under `~/.docket/state/`, `applied.lock` per pod). docket's Phase 56 deletes the old layout
with no migration (P56-16), deletes `critic` and `tester` for one judge, `reviewer` (P56-15,
merged), and makes the blueprints four shapes, `build`, `respond`, `operate`, `routine` (P56-21);
the skill's references still write `critic`, `tester` and the five old shapes.

**Read first:** the script and the spec (tests 06, 07, 09, 11, 15), `scripts/capture-docket-fixtures.sh`,
`onboarding-lab/skill/scripts/apply.sh`, `docs/TESTING.md`'s live-suite section; docket's
`CHANGELOG.md` entries of 2026-10-29 and `docs/CONFIGURATION.md` (read only).

**Does.**
1. `--check` recaptures docket's answers from its current checkout into the fixtures folder (the
   capture script's technique) and diffs their keys against the fake's; a key docket now sends that
   the fake does not, or the reverse, is a failing line naming it. The hand-written fixtures of A
   and B are replaced by captures, their `.provenance` updated to docket's commit.
2. Tests 06 and 07 assert, through docket's `GET /tasks/<pod>/<id>`, that the task record carries
   `task` with the objective, acceptance and done-when the card holds, and that `description`
   carries no brief; and on screen, after the run, that the Execution tab shows "What each step
   ran" with a prompt chip per non-Lead step.
3. Test 11 (Run again at L3): the second run's record carries `task.answers` with the question of
   the first; the check is through docket's record, in one assertion.
4. `apply.sh`: runs against docket's new home layout and nothing else (`docket init` on a fresh home
   writes it; the script's `--dry-run` reads `applied.lock` where it read the applied directory). If
   `docket pod validate` now checks prompt references, the skill's team directories must pass it;
   fix the directories the skill writes, not the check.
5. The skill's references (`rules.md`, `flow.md`, `blueprint-format.md`): one judge, `reviewer`,
   where they say `critic` or `tester` (a content team is writer → reviewer, a research team
   researcher → reviewer; the "judge of another family" rule stays, now on the reviewer); the shape
   names as docket's checkout has them when D runs (the four, if P56-21 merged; else the skill names
   none and writes the roster and pipeline). A team that works outward or keeps records is a shape
   plus the operator's services, records and policy: the skill names them as docket documents them
   and writes no service or secret itself (they are the operator's, ADR 0031).
6. `--check` also captures, with no model, `GET /pods/<p>/records` and a set's rows after
   `docket pod update` declares one on a scratch pod, and a pod with no codebase on `GET /pods/<p>`,
   replacing F's hand-written fixtures. E's `wait` and `outward` fixtures stay hand-written from the
   spec (they need a model to park), their `.provenance` naming docket's commit read.
7. The script's summary names docket's and prill's commits and versions (already) and the count of
   hops with a manifest over the hops run.
8. `TESTING.md`'s live-suite section says what the suite now proves about fields and manifests; the
   roadmap's Phase 82 section gets the run's result, date and commits; the lab's README too.

**Tests.** `bash -n` on the scripts; `npx playwright test -c playwright.docket-prill.config.ts --list`;
then the whole local suite once, at the end, on the local model, and `--check`.

**Done when:** `docket-prill-e2e.sh` and `--check` pass against docket's and prill's current
checkouts and name their commits; every fixture under `fixtures/docket/` is a capture with its
provenance, or says why not; and the onboarding skill applies a team on docket's new home layout
with no `critic`, no `tester` and no deleted shape.
