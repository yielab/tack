# ADR 0022: a whole project goes in and out through one file

**Status: ACCEPTED 2026-10-17 (the maintainer: "continua", on the schema shown 2026-10-14);
executed as wave 7 of Phase 73 (`docs/plans/phase-73.md`, tasks B1–B3).** Extends ADR 0017
(the outline) and reads ADR 0021 (roles, profiles, teams, auto-start) as what the file
describes. ADR 0025 amends decisions 2 and 6 (an import never deletes; the export carries no ids).

**Decide:** that one file holds a whole project — its vocabulary and workflow, its custom
fields, the agent profiles its roles use, its roles, its sprints, its automation settings and
its items — and that Tack reads it and writes it through the same rules as the outline of ADR
0017: the whole file is checked before anything is written, every problem is reported at once,
nothing is written unless all of it can be, and a dry run shows the plan. The same file comes
back out of `GET /api/projects/{id}/export?scope=bundle`, so a project can be moved, copied,
versioned in git or written by an assistant and imported. A project template becomes the same
file without items.

**Why now:** My SaaS is five roles, five profiles, five teams, four fields, two sprints and
twenty items. Setting that up by hand is eleven screens; ADR 0017's outline carries the items
and nothing else, so the project it creates still needs every role, profile, field and
automation setting typed in afterwards. The snapshot export (`POST /api/projects/import`) is
a database dump — ids, timestamps, internal JSON — that nobody should write by hand.

**If you do nothing:** every new project that uses roles and teams is set up by clicking,
twice when the second instance of Tack needs it, and there is no way to hand a project's
whole shape to an assistant or to keep it beside the code.

## The file

The outline of ADR 0017 with six optional sections, applied in this order:

```yaml
project:      # name, type, description; plus vocabulary (any of the 16 keys) and workflow
              # (preset: scrum|kanban|simple|construction|legal|research|event, OR statuses with
              # a category each, plus optional transitions)
fields:       # custom fields by name: type, required, options, default, validation, description
profiles:     # agent profiles by name: kind, instructions, tool_policy, limits, fleet (by name)
roles:        # roles by name: color, icon, profile (a name: this file first, then the board)
sprints:      # by name: goal, start, end, status (planning, the default, or closed)
automation:   # code_origin, repository, default_branch, workspace_mode, push_after_run,
              # default_harness, default_profile (a name), default_model, on_finish_status,
              # definition_of_done, autonomy {level, domains, verify_require, budgets,
              # merge_on_done, verify_loop_max_attempts}, auto_start {enabled, status, max_open}
items:        # ADR 0017's items, plus roles (names), sprint (a name), fields ({name: value}),
              # recurrence and estimate_unit
```

`crates/tack-api/tests/fixtures/bundles/my-saas.yaml` is the whole thing for My SaaS, with a
comment on every rule.

## Decisions

| # | Decision | What a person sees |
|---|---|---|
| 1 | **One file, one set of rules.** The bundle is `tack_core::outline::Outline` with the six sections; an unknown key anywhere is refused, every reference is a name, and the file is checked whole (`tack_core::outline::bundle`) before anything is written. The item rules of ADR 0017 do not change. | The same preview and the same refusal, now per section. |
| 2 | **Importing into an existing project adds and updates, never deletes.** Fields, profiles, roles and sprints are matched by name: a name the project has is updated with the keys the file gives, a new name is created, and the project's own names the file does not mention are counted ("3 roles not mentioned") and left alone. Items are always added (an outline never updates an item, ADR 0017). The automation section replaces the keys it names and leaves the rest. | "Roles: 2 new, 5 updated, 3 not mentioned". |
| 3 | **Profiles are the board's, not the project's.** A profile is matched by name across the whole board; a built-in profile (Implementer, Reviewer, Researcher, Planner) may be named by a role but is never changed by a file, with a warning. A `fleet` names a fleet; one the board does not have is a warning and the profile runs on any machine. `tool_policy.agent` and `tool_policy.docket.team` pass the same checks as `POST /api/agent-profiles`. | "Profiles: 5 new, 1 updated". |
| 4 | **The file can ask for more autonomy than an import may grant.** `autonomy.level` above L3 and `auto_start.enabled: true` are accepted and applied as L3 and off, each with a warning that names what was asked and what was applied: a file is text anyone can edit, and both are a person's act on the Merge and Start rungs (ADR 0021). The attestation public key is never in a bundle. Status names in `automation` and in items must be statuses of the workflow the file defines or the project has; `default_harness` must be an agent the runner knows. | "auto_start asked: true; applied: false. Turn it on from Settings → Automation". |
| 5 | **A field value is checked like a form.** A value under `fields` is checked against the field's type and options (a new field's from the file, an existing one's from the board) and a wrong one is refused; an item that lacks a required field is a warning, as a card created from the board would be. | "item 7 (\"La luna\"): Edad \"12\" is not one of 3-5, 6-8, 9-12". |
| 6 | **The export is the import.** `GET /api/projects/{id}/export?scope=bundle&format=yaml|json` writes the same file: items in written order with refs `1..n`, parents first; only `blocks` dependencies; only the profiles the project's roles and default point at; no ids, no dates, no key. Exporting, importing into a new project and exporting again gives the same text. | "Export project (YAML)" beside the snapshot buttons; `tack export <project> --bundle`. |
| 7 | **A template is a bundle without items.** `project_templates` gains `roles`, `profiles` and `automation` (migration `091_template_bundle`); saving a project as a template stores its bundle, and creating a project from a template applies it through the same code as an import. Three built-ins ship with roles and profiles: "Web product with content", "Library or CLI", "Static content site", at L3, with Legal, Security and Ops as read-only reviewer profiles and no repository set. `tack init <name> --template <id or name>` creates from one; `tack template save <project> --name <name>` stores one. | A template that comes with its roles. |
| 8 | **Every door reads the same file.** The outline routes, `tack import`, the MCP tool `create_outline` (which forwards every section) and Settings → Data → Import, whose preview lists each section before anything is written. | One preview: "Fields 4 · Profiles 5 new, 1 updated · Roles 7 · Sprints 2 · Automation set, 2 warnings · Items 20". |

## Writes

Items are written in one transaction as today. The configuration sections are a handful of
rows each and are written one by one, in the file's order, after the whole file passed its
check; a new project is deleted if any write fails. On an existing project a database failure
halfway leaves what landed, and the result names the section that failed; the file can be run
again, since every section matches by name.

## Alternatives not taken

- **Nesting items under roles or sprints.** One shape for the items, flat, as ADR 0017 chose;
  a role or a sprint is a name on the item.
- **A bundle that deletes what it does not mention.** A file written last month would delete
  this week's role. Deletion stays a person's act; the count of unmentioned names is the
  signal.
- **Letting a file switch auto-start on or set L4.** Both are the two acts ADR 0021 keeps on
  the ladder, with the ceiling and the blockers shown next to them. A file cannot show them.
- **A per-role ceiling in the built-ins.** Tack has no such thing; the ceiling comes from the
  team at merge time (ADR 0021 decision 5). The built-ins reach the same end with read-only
  profiles for Legal, Security and Ops and the project at L3.
- **Briefs in the bundle.** The snapshot export keeps them; a bundle is a plan, not an
  archive. A definition of done is prose; a command the agent must pass is an acceptance
  criterion on the item's brief and runs from the run's checkout, which the runner's evidence
  tests already prove.

## Questions for the maintainer, with the default built

1. `tack template create-from <id> <name>` keeps its meaning (a project from a template); the
   new verb that stores one is `tack template save`.
2. `estimate_unit` stays per item, as the data model has it.
3. A bundle imported twice into one project adds its items twice; the preview says how many
   items will be added, and items are never matched by title.
4. `repository` is stored as given, with the same check as the Automation tab (an absolute
   path for a folder, a URL for a url origin); whether the folder exists is the runner's
   finding at run time, as today.
