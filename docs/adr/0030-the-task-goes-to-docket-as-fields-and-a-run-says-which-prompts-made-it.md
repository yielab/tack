# ADR 0030: the task goes to docket as fields, and a run says which prompts made it

**Status: PROPOSED 2026-10-29.** The maintainer asked what Tack must change now that docket has
closed its Phases 52, 53 and 54 (2026-10-29): a prompt is a versioned file with a hash, a hop
closes with a `handoff` tool call, the message a model reads is a `## Task` section and a `## Hop`
section, `POST /tasks/<pod>` takes a structured `task`, every hop records a `promptManifest`, and
the built-in Anthropic provider reports its cache reads. docket's API version stays 6; every change
is additive. Nothing here waits on docket or on the verifier.

**Decide:**

| # | Decision | What changes for whoever uses Tack |
|---|---|---|
| 1 | **The item goes to docket as fields.** A docket run sends `task` (`objective`, `doneWhen`, `acceptance`, `constraints`, `risk`, `kind`, `paths`, `answers`) beside a `description` that holds only the profile's instructions and the card's fenced text. The brief and the definition of done are no longer rendered into prose for a docket run, and the adapter no longer re-parses the prose it composed. The other three agents keep the composed prose: it is what they can read | Nothing to configure. docket's agents read the card as labelled fields instead of a second copy in prose; the Lead's intake starts from them |
| 2 | **A rerun carries the answers already given.** `task.answers` holds the questions docket asked in the previous attempt of the same item and the option a person chose, read from the run events Tack already stores | A person is not asked the same question twice when they press Run again |
| 3 | **A run keeps what each step was made of.** When a docket task settles, Tack reads each hop's record (`promptManifest`, `artifact`) and keeps, per step: role, model, model class, family, the prompts by layer (`name`, `version`, `sha256`, `scope`, `tokens`), the verdict, and the agent's `assumptions` and `questions`. The fake docket writes the same shape | The Execution tab shows, per step, which prompts ran at which version, what the agent assumed and what it asked. Two runs of one item can be compared by prompt version |
| 4 | **The verifier gets the whole generator.** Attempt evidence names the model's family and carries a `trajectory` (the steps of decision 3, stored as one artifact, referenced by URI and sha256 digest) | A verifier's verdict can be read by prompt version and model class later; Tack still depends on no verifier (ADR 0020) |
| 5 | **Prompts for a docket run are docket's.** Tack writes no prompt file, no `.docket/prompts/` and no pod overlay; the one place Tack's text enters a docket run is the profile's instructions in `description`. What a role's prompt says, at which version, is read back through the manifest, never set from Tack | The profile's instructions stay the one text a Tack user writes; the team's prompts are edited where docket says (`.docket/` in the repository, applied by `docket pod update`) |

**Why now.** The prompt audit of 2026-10-28 (private document "Auditoría de prompts") found that
Tack sends the same card twice (as prose inside `description` and as `brief`), re-parses its own
prose to find the title, and that the hop record Tack forwards names only a model, so no run could
be told apart by the prompt that made it. docket removed the causes on its side the next day; this
ADR is Tack's half, scoped to what docket now offers.

**If you do nothing.** Every docket run keeps paying for the card twice in the `## Task` section,
the adapter keeps a string search over a heading it wrote itself (`BRIEF_HEADING`,
`objective()` in `crates/tack-runner/src/harness/docket/task.rs`), and the Execution tab cannot say
which prompt version a step ran, so the measurements the audit asks for (tokens per layer and role,
unparsed verdicts per prompt version, verifier results per version and class) have no data.

## What docket shipped that this reads (2026-10-29, `develop` at `ef26faac`)

Every locator is docket's; Tack reads them and edits none.

- `POST /tasks/<pod>` takes `task` beside `description` (`specs/api/operator-api.spec.md`
  "Arguments"; `src/docket/tasks/domain/request.py::TaskRequest`): strings `objective`,
  `doneWhen`, `risk`, `verify`, `kind`; lists of strings `acceptance`, `constraints`, `paths`; a list
  of objects `answers`. `extra="forbid"`: an unknown key or a wrong type is `400` naming it and
  nothing is enqueued. With both present, `description` is the title. The first non-Lead hop's
  `## Task` section renders the fields in a fixed order and `paths` bound that hop's repository
  map. The text is screened by `pre_input` with the description.
- Each hop of `GET /tasks/<pod>/<id>` carries `promptManifest`
  (`{"layers": [{"layer", "name", "version", "sha256", "scope", "tokens"}], "model", "family",
  "class", "profile"}`), absent on a hop recorded before the field existed. `sha256` is the hash
  of the prompt's text as its file holds it; `tokens` is the compiler's estimate. The `task` layer
  has `version ""` and scope `task`. The hop's `artifact` may carry `assumptions` and `questions`
  (lists of strings, absent when empty) from its `handoff` call; a question never parks the task.
- The `verdict` tool is gone; the `handoff` tool closes a hop (`summary`, `files_changed`,
  `verdict`, `assumptions`, `questions`). The `hop_finished` trace event is unchanged
  (`role`, `model`, `cycle`, `ok`, `verdict`, `gate`, `usage`, `durationMs`, `error`), so Tack's
  timeline lines stay as they are; the manifest is on the record, not on the trace.
- A hop message no longer carries an absolute path, and the map is bounded by the brief's and
  the request's `paths` (`pod-dispatch.spec.md` "Bounded hop prompts").
- The built-in `anthropic` provider speaks the Messages API: `cachedTokens` and cache writes are
  in the measured usage. Tack already reads `cached` and `cacheWrite` on a hop and a task
  (`harness/docket/wire.rs::Tokens`) and leaves cache reads out of the counted total (ADR 0024);
  nothing changes there.
- `GET /metrics` gained `docket_cache_hit_ratio{role}` and `docket_tokens_per_accepted_task`.
  Tack does not read docket's metrics endpoint, and this ADR does not start.

## The two shapes

What Tack sends, built from the item, its brief and the project (card A):

```json
{
  "description": "<profile instructions>\n\n## The item\n\n<fenced title, description, comments>",
  "task": {
    "objective": "<the item's title>",
    "doneWhen": "<the brief's definition of done; else the project's>",
    "acceptance": ["`cargo nextest run …` exits 0", "src/x.rs exists"],
    "constraints": ["Do not touch paths matching docs/**", "At most 5 changed files"],
    "risk": "medium",
    "kind": "<the item's type, as the project's vocabulary names it>",
    "paths": ["src/x.rs", "crates/y/"],
    "answers": [{"question": "<what docket asked>", "answer": "<the option's label or the text given>"}]
  },
  "gate": {"command": "…", "timeoutS": 600}
}
```

`paths` are the paths the acceptance criteria name (`file`, `absent`, a `command`'s `cwd`), relative
to the repository; nothing is invented. A field with nothing to say is left out, never sent empty.
`brief` is sent only if docket's `pod-dispatch.spec.md` "Task brief" says a caller's brief pre-empts
the Lead's intake; the card reads that section and decides once.

What Tack keeps on the attempt, from the task record after it settles (cards B and C). It is one
JSON value, `steps`, in the run's structured result, beside what the Execution tab already reads:

```json
{
  "steps": [
    {
      "role": "implementer", "model": "qwen2.5-coder:7b", "class": "cheap", "family": "qwen",
      "verdict": null, "ok": true,
      "prompts": [
        {"layer": "identity", "name": "implementer", "version": "1.0.0", "sha256": "…", "scope": "shipped", "tokens": 410},
        {"layer": "task", "name": "task", "version": "", "sha256": "…", "scope": "task", "tokens": 1280},
        {"layer": "hop", "name": "implementer", "version": "1.0.0", "sha256": "…", "scope": "shipped", "tokens": 12}
      ],
      "assumptions": ["…"], "questions": ["…"]
    }
  ]
}
```

A hop with no manifest has `prompts: []`; the screen then says the step recorded no prompt, never
an empty list. The same value is the `trajectory` artifact of decision 4.

## Considered and not adopted

- **Tack writing prompt files or a pod overlay.** ADR 0012 on docket's side makes the repository's
  `.docket/` the configuration of record, applied by an operator command; Tack's "Apply the
  repository's teams" is disabled with its reason (roadmap, Cancelled). A second author of the same
  files is the DRY problem the audit found in docket, moved into Tack.
- **A Tack-side prompt registry or versioning of the profile's instructions.** The instructions are
  one text per profile, edited on one screen; the manifest records them inside docket's `task`
  layer by hash. Versioning them separately would be a second copy of what the hash already says.
- **Choosing a model class per item type from Tack.** docket picks the prompt variant from the
  model the pipeline step resolves, and the step's `model: cheap|strong` is pipeline configuration.
  Tack already names the pipeline per type (`pipeline_by_type`); that is the lever.
- **Reading docket's `/metrics`.** Tack's factory metrics are its own; a cache-hit ratio per role
  belongs to docket's dashboard row (its ADR 0033).
- **Sending `claims` to the verifier.** prill's `attempt-evidence/v0` leaves `Claim` unspecified
  (`contracts/attempt-evidence-v0/schema.json`); nothing can be sent against an empty schema.
- **A new event kind per hop record.** The timeline already has a line per hop from the trace;
  the record is read once, at settle, into `steps`. A second line per hop would say the same thing
  twice.
