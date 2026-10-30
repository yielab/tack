# Phase 82: the task goes to docket as fields, and a run says which prompts made it

**Status: proposed 2026-10-29.** ADR 0030 (proposed) holds the decisions and the two JSON shapes;
this plan is the cards. docket's side is closed (its Phases 52–54, 2026-10-29, `develop` at
`ef26faac`); Tack reads what it shipped and edits nothing in docket's or prill's repositories.

Four cards, sized so that one Sonnet or Haiku agent finishes each in one sitting with the files it
names and nothing else. Two rounds. Each card owns its files; the one shared file (the fake docket)
is split by function below.

## How every card runs

- **Model.** A and B are Sonnet (they change a design). C and D are Haiku-sized where said: C follows
  the Execution tab's own patterns; D edits a script and a spec that already exist.
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
| 2 | **C ∥ D** (C after B; D after A, B and C are merged, since it asserts what they show) | full suites once; the local suite of the three, run by D |

| Id | Card | Model | Owns | Waits on |
|---|---|---|---|---|
| A | The item goes to docket as fields | Sonnet | `tack-protocol` (`TaskDocument`), `tack-app/src/runs/start_run.rs`, `tack-runner/src/harness/docket/task.rs`, the fake's request reader, `docket_v5.rs` (request tests), the developer guide's docket contract | — |
| B | A run keeps what each step was made of, and the verifier gets it | Sonnet | `tack-runner/src/harness/docket/wire.rs`, `docket.rs` (settle), `verify/prill.rs`, the fake's hop record, `docket_v5.rs` (record tests), the prill fixtures' README, the developer guide's adapter pages | — |
| C | The Execution tab says which prompts ran and what the agent asked | Haiku | `frontend/src/shared/runWithAgent/` (`ExecutionTimeline.tsx`, `AgentWork.tsx`, `LatestRunSummary.tsx`), `shared/execution/types.ts`, the user guide's Execution tab section | B |
| D | The local suite proves it against the new docket | Sonnet | `onboarding-lab/e2e/` (script, spec), the recaptured fixtures under `crates/tack-runner/src/harness/fixtures/docket/`, `onboarding-lab/skill/scripts/apply.sh`, `TESTING.md`'s live-suite section | A, B, C |

## Left out, and why

- Tack writing prompt files, a pod overlay or `.docket/prompts/`; versioning the profile's
  instructions in Tack; a model class per item type; reading docket's `/metrics`; `claims` to the
  verifier; a new event kind per hop record: each is in ADR 0030 "Considered and not adopted".
- Renaming `fixtures/docket/v5/` to `v6/`: docket's API number did not change; the folder name
  is wrong for a reason Phase 81 already accepted, and D recaptures into it.
- A measurement card. The numbers the audit asks for (tokens per layer and role, verdicts per prompt
  version, verifier results per version and class) exist once B lands; measuring them is a run on
  a model, which is the maintainer's, with the data C shows.

**Done when:** a docket run sends the card as `task` fields and never as a second copy in prose; a
settled run holds, per step, the prompts by name, version and hash with the agent's assumptions and
questions, shown on the Execution tab in words; the verifier's evidence names the family and a
trajectory; the fake docket answers the same shapes as the recaptured docket; and the local suite
passes against docket's current checkout and names its commit.

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
   `class`, `profile`) and `artifact: Option<HopArtifact {summary, verdict, assumptions, questions}>`,
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

### C. The Execution tab says which prompts ran and what the agent asked

**Model:** Haiku. **Waits on:** B (reads the `steps` shape; the hand-written fixture of B is the
test data). Decision 3 of ADR 0030, the screen half.

**Why.** The Execution tab shows what the agent concluded, what it did by tool and by time, and what
it got (sandbox, variables, working directory). It cannot show which prompt a step ran, at which
version, nor what the agent assumed or asked, because the run did not carry it until B.

**Read first:** `ExecutionTimeline.tsx`, `AgentWork.tsx`, `LatestRunSummary.tsx`,
`shared/execution/types.ts` and `attempts.ts` (where the attempt's result is read), their tests;
the user guide `items.md` "Execution tab"; ADR 0030 "The two shapes" (`steps`); Phase 81 card S's
rule for screen text (plain words, no variable names, no commands, the verifier never named).

**Does.**
1. `types.ts`: the `steps` type as ADR 0030 shapes it, optional on an attempt.
2. In the Execution tab, one section **What each step ran** after "What the agent did": one row per
   step with the role in words (Lead, Implementer, Reviewer, Tester, or the custom role's name),
   the model and its class in words ("a small model", "a strong model"), the verdict when there is
   one, and the prompts as "identity · task · hop" chips reading `name version` with the scope in
   plain words ("shipped with docket", "the team's", "this repository's", "your settings"); the hash
   is shown shortened on the chip's title and copied whole on click, never in prose. A step with
   `prompts: []` reads "This step recorded no prompt." An attempt without `steps` shows no section.
3. Under each step, **The agent assumed** and **The agent asked**, each a list, only when present.
   On the Last run card, one line "The agent asked N question(s)" linking to the tab when any step
   has questions; none otherwise.
4. The user guide's Execution tab section gains a paragraph on the new section, in the guide's
   words (no field names). `CHANGELOG.md` line.

**Tests.** `npx vitest run src/shared/runWithAgent/ExecutionTimeline.test.tsx src/shared/runWithAgent/LatestRunSummary.test.tsx src/shared/execution`:
a run with steps, a step without prompts, a run without steps, the questions line.

**Done when:** a person can read on the Execution tab which prompts, at which version, each step of
a run used, what the agent assumed and what it asked, in words; and a run from before this phase
shows the tab exactly as before.

---

### D. The local suite proves it against the new docket

**Model:** Sonnet. **Waits on:** A, B and C merged (it asserts what they send and show).

**Why.** The live suite (`onboarding-lab/e2e/docket-prill-e2e.sh` with `docket-prill.spec.ts`,
fifteen tests) last ran against docket `52c22f89`, before docket's Phases 52–54. Its fixtures under
`crates/tack-runner/src/harness/fixtures/docket/v5/` and the fake were captured from that docket.
The onboarding skill's `apply.sh` runs `docket init`, `docket pod validate` and `docket pod update`
against a home whose layout docket changed (authored configuration under `~/.docket/config/`,
state under `~/.docket/state/`, `applied.lock` per pod; the old layout is read for one minor).

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
4. `apply.sh`: runs against docket's new home layout without relying on the one-minor compatibility
   read (`docket init` on a fresh home writes the new layout; the script's `--dry-run` reads
   `applied.lock` where it read the applied directory). If `docket pod validate` now checks prompt
   references, the skill's team directories must pass it; fix the directories the skill writes, not
   the check.
5. The script's summary names docket's and prill's commits and versions (already) and the count of
   hops with a manifest over the hops run.
6. `TESTING.md`'s live-suite section says what the suite now proves about fields and manifests; the
   roadmap's Phase 82 section gets the run's result, date and commits; the lab's README too.

**Tests.** `bash -n` on the scripts; `npx playwright test -c playwright.docket-prill.config.ts --list`;
then the whole local suite once, at the end, on the local model, and `--check`.

**Done when:** `docket-prill-e2e.sh` and `--check` pass against docket's and prill's current
checkouts and name their commits; every fixture under `fixtures/docket/` is a capture with its
provenance; and the onboarding skill applies a team on docket's new home layout.
