# ADR 0025: the project file is kept in sync, not only imported

**Status: PROPOSED 2026-10-18; postponed 2026-10-27, future and not planned.** To be executed by Phase 76 (`docs/plans/phase-76.md`), which has no date. Amends ADR 0022
decision 2 (an import never deletes), decision 6 (the export carries no ids) and its alternative
"a bundle that deletes what it does not mention". Keeps ADR 0022 decision 4 (a file cannot raise
autonomy or switch auto-start on) and ADR 0017 (items are always added, never matched).

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
| 4 | **A deletion shows its cost, and some are refused.** A role: how many cards lose it. A field: how many values are deleted with it. A sprint: how many cards leave it. A profile: refused when a role or the default of another project points at it, or when a run of it is open; built-in profiles are never in the plan. | "Removes field Dialecto and its 37 values" with a box to tick before **Apply**. "Profile My SaaS writer is used by project Matrix: not removed." |
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
