# ADR 0025: the project file is kept in sync, not only imported

**Status: ACCEPTED 2026-11-26 with its amendment and the amendment's four defaults (the maintainer:
"abre la fase 76"); proposed 2026-10-18, postponed 2026-10-27.** Executed by Phase 76
(`docs/plans/phase-76.md`), open. Amends ADR 0022
decision 2 (an import never deletes), decision 6 (the export carries no ids) and its alternative
"a bundle that deletes what it does not mention". Keeps ADR 0022 decision 4 (a file cannot raise
autonomy or switch auto-start on) and ADR 0017 (items are always added, never matched).

**Amended 2026-11-26 (accepted the same day): the plan lives in the repository.** The section of that name at
the end widens what is kept from the configuration to the items and their briefs, in a `.tack/`
folder beside the code with one file per item. Where each item is, who answers for it and
everything a run leaves stay on the board. The amendment replaces decision 1, extends decisions 2,
3, 4, 5, 7 and 8, withdraws ADR 0022's alternative "Briefs in the bundle", and records what a
re-read of Phase 76's plan against the tree found.

**Decide:** that a project's configuration — its name, description and vocabulary, its fields,
the profiles its roles use, its roles, its sprints and its automation — can be **kept** in a file
the way Drupal keeps configuration in a sync directory: an export writes the board's configuration
and marks it as the last known state; an import compares the file, the board and that last known
state, applies what changed in the file, keeps what changed on the board, deletes what the file
removed, shows the cost of every deletion before writing, and stops on a field both sides changed.
Every configured thing gets a **key** that survives a rename. Every export and every import leaves
a **revision**, which the board can show and go back to. Preview and apply are **one plan**: the
apply carries the preview's hash and is refused if the board moved. Everything an import writes
lands in **one transaction**. Items stay content: a file adds them and never changes or deletes
them.

**Why now:** the project file of ADR 0022 is a one-way door. It creates and updates by name and
never deletes, so a role renamed in the file becomes a second role, a role removed from the file
stays on the board for ever, and a file imported twice into one project doubles its items. Keeping
My SaaS's project beside its code — the reason ADR 0022 gave for the file — means editing the
file and expecting the board to follow, which today it cannot. The sections are written one by one
outside a transaction, so a failure halfway leaves a half-applied project that the answer can only
describe.

**If you do nothing:** the file is good for the first import and for templates, and for nothing
after; every later change is made by clicking, and the file in the repository drifts from the
board with no way to tell which is right.

## Decisions

| # | Decision | What a person sees |
|---|---|---|
| 1 | **Configuration and content are two halves of one file.** The configuration is every section of the project file but `items`. `GET /api/projects/{id}/export?scope=config` writes it (the same `to_bundle`, items left out); `scope=bundle` keeps writing both halves. Only the configuration is kept in sync. Items keep ADR 0017's additive import. | **Export configuration (YAML)** beside the export buttons; `tack config export`. |
| 2 | **Every configured thing has a key.** Roles, custom fields, sprints and agent profiles gain a `key`: lowercase letters, digits and hyphens, made from the name when the thing is created, never changed afterwards. The file carries it; a file entry with a `key` is matched by it and may rename; one without a key is matched by name, as today, and gets a key from its name when created. An export always writes keys. | `key: contenido` under each name in the file; the key shown in grey under the name in Settings → Roles and Fields. A rename is "Contenido → Content", not a new role. |
| 3 | **An import is a three-way comparison against the last revision.** For each key and each field: changed only in the file → applied; changed only on the board → kept; changed on both to different values → a conflict. A thing the file no longer has is deleted only when the board has not touched it since the last revision; a thing created on the board after the last revision is never deleted. Conflicts stop the import unless the person says which side wins for all of them (`prefer=file` or `prefer=board`). With no revision yet nothing is deleted, and the preview says to export once to make the file authoritative. | A plan in three colours: new, changed, removed; a fourth line "kept: changed on the board since the last revision"; conflicts listed with both values. |
| 4 | **A deletion shows its cost, and some are refused.** A role: how many cards lose it. A field: how many values are deleted with it. A sprint: how many cards leave it. A profile: refused when a role or the default of another project points at it, or when a run of it is open; built-in profiles are never in the plan. | "Removes field Plan and its 37 values" with a box to tick before **Apply**. "Profile My SaaS writer is used by project Matrix: not removed." |
| 5 | **Every export and every import leaves a revision.** A revision is the configuration as the file writes it, with a number, its origin (the board, a file, a revert) and its hash; one is written when the configuration is exported, one before an import is applied (the board as it was) and one after (what was written); two equal ones are one. The last revision is the base of decision 3. Going back is importing a revision through the same plan and apply. | Settings → Data → **History**: "#7 · from a file · 2026-10-18 14:03 · Revert to this…". `tack config history`, `tack config revert 7`. |
| 6 | **Preview and apply are one plan, written in one transaction.** The preview answers with a `plan_hash`; the apply sends it back and is refused (409) when the board's configuration changed in between. The apply, and from now on every import of a project file, writes in one transaction: the deletions, the sections, the items, the revision. A failure writes nothing and says which section failed. The "what landed stays, import again" answer of ADR 0022 goes away. | "The project changed since the preview; preview again." Never a half-imported project. |
| 7 | **What the file still cannot do.** Switch auto-start on or raise the autonomy level (ADR 0022 §4). Change the workflow or the project type: both stay in Settings, because a removed status has cards and the Workflow tab is where that is handled. Change or delete an item. The preview says so each time. | "project.workflow is kept; change it under Settings → Workflow." |
| 8 | **Every door reads the same plan.** `GET /api/projects/{id}/config/status`, `POST …/config/plan`, `POST …/config/apply`, `GET …/config/revisions`; `tack config status \| export \| import \| history \| revert`; the MCP tool `sync_config`; Settings → Data → Configuration with the plan dialog and the history. A plan with deletions is applied from the CLI only with `--yes`. | One dialog for an import, a revert and "what changed on the board". |

If you accept this table, you have accepted the ADR. Everything below is evidence.

---

## Evidence (read 2026-10-18 on `develop` at `cebaef77`)

| Fact | Where |
|---|---|
| The import matches by name only; a renamed name is a new row and the old one is counted "not mentioned" | `crates/tack-core/src/outline/bundle.rs` `match_names` (line 203) |
| "Importing into an existing project adds and updates, never deletes" is a decision, and "a bundle that deletes what it does not mention" a rejected alternative, because "a file written last month would delete this week's role" | ADR 0022 decision 2 and its alternatives |
| The sections are written one by one on the pool; a failure halfway answers `SectionFailed { kept: true }` and the text "import the same file again to finish" | `crates/tack-api/src/handlers/outline.rs` `write` (line 685), `crates/tack-api/src/error.rs` line 45; `docs/book/src/user-guide/import-export.md` "If a section fails to write" |
| A new project is deleted again by hand when a section fails | `outline.rs` `create_new`, line 379 |
| The items are already written in one transaction, through `insert_item(conn, …)` on "whatever connection the caller holds" | `crates/tack-db/src/repo/items.rs` `create_outline_items` (line 122), `insert_item` (line 1041) |
| The export writes no ids on purpose, so a project can be moved and the file written by hand | ADR 0022 decision 6 |
| A profile is matched across the whole board; the export writes only the profiles the project's roles and default point at | `bundle.rs` `check` (line 444), `to_bundle` (line 850) |
| Deleting a role or a field is one `DELETE` with no look at what uses it; the Fields tab asks "This will delete all values" with no count; there is no `delete_sprint` | `crates/tack-db/src/repo/roles.rs` line 180, `custom_fields.rs` line 291, `sprints.rs`; `frontend/src/features/settings/panels/FieldsPanel.tsx` line 100 |
| Deleting a profile is inline SQL in the handler, duplicated from nothing in the repository | `crates/tack-api/src/handlers/runner_admin.rs` `delete_profile`, line 1183 |
| The `BundleAutonomy` mapping from `Settings` exists twice | `handlers/export.rs` `bundle_response` (line 195), `handlers/templates.rs` `save_project_as_template` (line 440) |
| The only versioned configuration today is a brief; `items.version` is an optimistic lock | `item_brief_versions` and `items.version` (`crates/tack-db/src/migrations.rs` line 1137) |
| `BoardConfigUpdated { project_id }` already exists on the board socket and nothing sends it for roles, fields or sprints | `crates/tack-api/src/handlers/websocket.rs` line 35 |
| A migration's statements run in one transaction | `crates/tack-db/src/migrations.rs` `apply_migrations`, line 391 |
| `sha2` is a dependency of `tack-api` and `tack-db` | the two `Cargo.toml` |

## Alternatives not taken

- **Carry the row's id in the file instead of a key.** It would make renames work with no migration, and ADR 0022 §6 rejected it for a reason that still holds: a person writes `key: contenido`, nobody writes `id: 8f3e…`, and an id travels badly to another board. A key is Drupal's machine name: short, stable, human.
- **Watch the file and apply it by itself.** The board does not see the project's folder (the runner does, and the board has no channel to it — Phase 73's gap); a `git pull` or an agent can change the file, and a deletion applied without a person seeing its cost is the one thing this ADR exists to prevent. The status line says the file and the board differ; a person applies.
- **A revision on every change made from the UI.** It would give a complete history at the cost of a hook in every handler that touches a role, a field, a sprint, a profile or a project setting (fifteen call sites), and the three-way comparison does not need it: the board's side of the comparison is read live. Git is the history of the file; the revisions are the sync points, which is what reverting and comparing need.
- **Resolve conflicts field by field in the dialog.** Useful and not needed to ship: one side for all conflicts covers the two real cases (the file is authoritative; the board is). The per-field control is shown disabled with that reason, and listed in the roadmap.
- **Match items by key and prune them.** Items carry runs, evidence, comments and attachments; "deleting" one is archiving, which Tack does not have yet, and a title is not a key. ADR 0017's rule stays; the next phase can take it up with its own decision.
- **Make `tack import` the sync.** `tack import` and `POST …/outline` keep ADR 0022's meaning (add and update, never delete) so a file written for them keeps doing what it did. The sync is a second verb with a second name, as `drush cim` is not `drush content:import`.
- **Sync the workflow too.** A removed status is a deletion with cards behind it, and the Workflow tab already owns moving them. It can join the file when that rule has one owner.

## Questions for the maintainer, with the default built

Blocking: none.

1. The key's alphabet: lowercase letters and digits of any script, and hyphens; a run of anything else becomes one hyphen; at most 64. Built so: "En revisión" → `en-revisión`.
2. Where the file lives: the documentation recommends `.tack/project.yaml` in the project's folder; Tack reads and writes only the path it is given.
3. An export records a revision although it is a `GET`: it is how the board learns what the file last knew. Built so, deduplicated by hash.
4. Revisions are kept for ever; My SaaS's whole configuration is about six kilobytes.

---

## Amendment of 2026-11-26: the plan lives in the repository (accepted)

**Decide:** that the file kept beside the code holds the whole plan, not only the configuration.
A project folder, `.tack/`, holds the configuration (`project.yaml`) and one file per item
(`items/<key>.md`). The item's file says what the item is and what makes it done: its title,
description, type, parent, blockers, roles and brief. The board keeps where the item is (its
status), who answers for it and when (assignee, priority, sprint, due date), and everything that
happens to it (runs, evidence, decisions, comments, change sets). The keys, the three-way
comparison, the revisions and the one plan of this ADR apply to the items as they do to the roles.

**Why now:** several boards built for agents closed as products in 2026, while the task formats
that live in the repository kept growing (Spec Kit, OpenSpec, Beads, Backlog.md, AGENTS.md). None of
those formats carries a check a machine runs, a limit or an autonomy level. Tack's brief does. As
written, this ADR keeps the configuration beside the code and leaves the brief in the board's
database. An assistant working in the repository then cannot read what makes an item done, a
branch cannot carry a change to the plan, and the plan does not outlive the board.

**If you do nothing:** Phase 76 keeps how the project is set up beside its code, and what the
project is doing only on this board. The file in the repository can make a second board look like
the first, but it cannot say what any item is for or when it is done.

| # | Decision | What a person sees |
|---|---|---|
| 9 | **The repository says what the work is; the board says where it is.** The kept half grows from the configuration to the plan. The plan is the configuration plus, for each item, its title, description, type, parent, blockers, roles, tags, estimate and unit, recurrence, field values and brief (definition of done, acceptance criteria, constraints, risk). Status, assignee, priority, sprint and due date are the board's. A file may give them for a new item, as an outline does, and they are not read for an item that exists. An item's run settings (harness, model, network, tools, approvals) never enter the file: like the autonomy level, they widen what an agent reaches, and a file is text anyone can edit. Runs, evidence, decisions, comments, attachments, change sets and links never enter the folder either | The plan lists an item's changes field by field. When a file changes one of the board's keys, the plan says "status, assignee, priority, sprint and due date are the board's: kept". Moving a card never changes a file |
| 10 | **Every item has a key, and the key is its file name.** Items gain a `key` by decision 2's rule. An item created on the board gets it from its title, and it never changes. An item written in the folder takes its file name as its key. A new title under the same key is a rename. In the single-file project file an item without a key is added, as today | `items/login-con-github.md`; the key in grey under the title on the item's page. Renaming the item in its file renames the card |
| 11 | **The folder is the project file split into files.** `.tack/project.yaml` holds every section but `items`. `.tack/items/<key>.md` holds one item: its keys and its brief (brief-v1's shape, without `item_id`) as YAML between two `---` lines, and its description as Markdown below them. `parent` and `depends_on` name keys. Reading the folder gives the same outline the single file gives, checked by the same rules. An export writes the whole folder again, so the file of an item the board no longer has goes away | Two branches that change two items do not touch the same file. A diff names the item. An assistant reads or writes one item |
| 12 | **A brief the folder changes is a new brief version.** It goes through the brief's own write, with its version and its hash, as the Brief tab does. A run already started keeps the brief its request carried. The single-file project file carries the brief too, so `tack import` gives a new item its brief. This withdraws ADR 0022's alternative "Briefs in the bundle" | The Brief tab shows a new version, as after an edit made there |
| 13 | **An item the folder no longer has is removed only when nothing happened to it.** Decision 3's rule for removals applies. On top of it, a file never removes an item that has a run, a comment, an attachment, a change set, a link or children on the board: removing it would delete that history with it, and Tack has no archive. Such an item is kept, with the reason. What a removal costs is shown first: the blockers that go with it | "Kept: 'Login con GitHub' has 2 runs and 3 comments; close it on the board." "Removes 'Borrador' and 1 blocker" |
| 14 | **A run never changes its own contract.** The board reads the folder only from what a person applies, never from a run's work tree. A change whose paths include `.tack/` is governed at L3, whatever its domains and the project's level say, so a person merges it | Merge readiness: "Changes the project folder: a person merges this" |
| 15 | **A person applies; an agent previews.** `tack config` is already the command for the CLI's own settings. The verbs are therefore `tack folder status \| export \| import \| history \| revert`. They read and write `.tack/` in the repository where the CLI runs; the board never reads a repository. The API takes the folder as one object of paths and texts, under `/api/projects/{id}/folder/*`. The MCP tool `plan_folder` previews for any credential. Applying needs a person's token, and an agent's credential is refused, because a change set cannot carry configuration or a brief. Settings → Data → **Project folder** holds the card, the plan dialog and the history | "An agent may preview the project folder; a person applies it" |
| 16 | **The sections added after this ADR was written.** `types` is compared like the keyed sections: a type already has a key, and the catalogue already refuses to remove a type in use. `teams` is not compared, because it is what the harness reports (ADR 0036). It is written on export and listed as missing on import, as today | — |

If you accept this table, you have accepted the amendment. What follows is evidence.

### What the re-read of Phase 76's plan found (`develop` at `6e05b93a`)

The plan's decisions hold. These facts have moved, so the plan is rewritten from this ADR when the
phase is opened:

| The plan says | The tree says |
|---|---|
| The migrations are `095_config_keys` and `096_config_revisions` | The last migration is `114_reviewer_verdicts` (`crates/tack-db/src/migrations.rs` line 178); the next free numbers are 115 and 116 |
| Roles, sprints, fields, projects and settings get `*_in(conn, …)` twins | None exist. Since Phase 79 the repository's methods take `&self` and answer `StoreError`. The one-connection precedents are `insert_item(conn, …)` (`crates/tack-db/src/repo/items.rs` line 678) and the repository's own `BEGIN IMMEDIATE` (`crates/tack-db/src/repo/change_sets.rs` line 377). The one-transaction task stays, in that shape |
| `runner_admin::delete_profile` deletes with inline SQL | It calls `repo.delete_agent_profile` (`crates/tack-api/src/handlers/runner_admin.rs` line 1503). That part is done |
| There is no `delete_sprint` | Still none (`crates/tack-db/src/repo/sprints.rs`) |
| The sections are written one by one, with a compensating `delete_project` and `SectionFailed { kept }` | Unchanged (`crates/tack-api/src/handlers/outline.rs` lines 419 and 562, `crates/tack-api/src/error.rs` line 64) |
| The `BundleAutonomy` mapping exists twice | Unchanged (`crates/tack-api/src/handlers/export.rs` line 200, `crates/tack-api/src/handlers/templates.rs` line 399) |
| Nothing sends `BoardConfigUpdated` | Unchanged (`crates/tack-api/src/handlers/websocket.rs` line 39) |
| `match_names` is at line 203 | Line 220; `check` line 552; `to_bundle` line 1079 (`crates/tack-core/src/outline/bundle.rs`) |
| The file has six optional sections | Eight: `teams` and `types` came later (`crates/tack-core/src/outline.rs` lines 66 and 70) |
| The CLI verbs are `tack config export \| import \| …` | `tack config` sets the CLI's URL and token (`crates/tack-cli/src/main.rs` line 458) |
| The MCP tool goes beside `create_outline` in `crates/tack-cli/src/mcp.rs` | The tools live in `crates/tack-cli/src/mcp/` (`projects.rs`, `items.rs`, `changes.rs`, `tools_list.json`) |
| The profile list is in `RunnerFleetSection.tsx` | `AgentProfilesPanel.tsx`, in the same folder |

### Evidence for the amendment

| Fact | Where |
|---|---|
| An item in the project file carries no brief and no key | `crates/tack-core/src/outline.rs` `OutlineItem` (line 423) |
| An item has no key and no number, only its id; the screen shows the id's first six characters | `frontend/src/shared/shortItemId.ts` |
| A brief write records a version and its hash in the same transaction | `crates/tack-db/src/repo/briefs.rs` `upsert_item_brief` (line 93) |
| A run's request carries the brief as it was when the request was created | `crates/tack-protocol/src/execution/types.rs` line 508 |
| The brief's wire shape is brief-v1 | `docs/contracts/brief-v1/` |
| Deleting an item cascades over every table that references it; its children lose their parent | `crates/tack-db/src/repo/items.rs` `delete_item` (line 424); `ON DELETE CASCADE` and `parent_id … ON DELETE SET NULL` in `crates/tack-db/src/migrations.rs` |
| Only projects can be archived | `crates/tack-db/src/migrations.rs` line 565 |
| A change set has no operation that edits a title, a description or a brief | `crates/tack-core/src/change_set.rs` `Operation` (line 147) |
| A domain matches only when every path of the change matches it, and no match gives L3 unless the project is L5. A change that mixes `.tack/` and code in an L5 project is governed at L5 today | `crates/tack-core/src/autonomy.rs` `level_for` (line 75) |
| The catalogue refuses to remove an item type in use | `crates/tack-db/src/repo/item_types.rs` `set_item_types` |
| The board now has a channel to the runner, so the alternative "watch the file" below no longer rests on its absence | `crates/tack-protocol/src/execution/operations.rs` `TeamOperation` (line 24) |

### Alternatives the amendment did not take

- **Everything in the repository, status included.** Some repository formats keep the status in
  the file. Here the status moves many times a day, by runs and by people. Every move would be a
  commit, two branches would disagree about where an item is, and nothing could say which branch
  is the board. The board is the state machine; the folder is the contract.
- **One file for the whole plan.** That file stays for moving or copying a project
  (`scope=bundle`). Kept beside the code, it would make every change to any item touch the same
  file, and two branches would conflict on it.
- **The board reads the folder from the repository by itself**, through the runner's channel. It
  is possible now, and it is not taken. Decision 8's rule that a person applies stays, and the CLI
  or an assistant already runs beside the repository. A line saying "the repository and the board
  differ", read by the runner, can come later as a card.
- **An agent applies the folder through a change set.** A change set holds operations on items.
  Making it carry configuration and briefs would turn its panel into a second Settings. An agent
  previews and proposes the change as a commit; a person applies it.
- **Archiving what the folder removes.** Tack has no archive for items, and adding one is a
  decision of its own. Until then an item with history is kept, and the plan says why.

### The amendment's questions for the maintainer, with the default built

Blocking: none.

1. The names: "project folder", `.tack/`, `tack folder …`, the MCP tool `plan_folder` and the
   routes `/folder/*`. Built so.
2. Which keys are the board's: status, assignee, priority, sprint and due date, while the
   estimate and the field values travel in the file. A team that plans its sprints in git would
   want the sprint in the file. Built without it.
3. The key of an item with a long title: decision 2's rule, cut at 64 characters. Built so.
4. Items created on the board reach the folder at the next export, and the status line counts them
   ("4 items created on the board since revision #7"). Built so.
