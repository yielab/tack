# plan-v1

`tack-plan.json` is the one format in which anything proposes items: a list of subtasks, each with
a title, a description, optional acceptance sentences, optional `depends_on` indexes into the same
list, an optional `S`/`M`/`L` estimate and four optional keys that say where the entry goes:
`type` (the item type, when the project has it), `role` (a project role, by name), `team` (a team,
by name) and `parent` (the index of an earlier entry this one is a sub-item of). `v` stays `"1"`.

The format belongs to no harness. It reaches the board in one of three ways, and the accept
endpoint cannot tell which: a file an agent writes at the root of its work tree (the Planner and
the Researcher profiles' instructions say so), a team's records, or a file a person uploads on the
item. The runner reads the file after capture, stages it as an artifact of kind `plan` (media type
`application/vnd.tack.plan+json`) and keeps it out of the patch; a file that does not parse yields
no artifact and one `attempt.plan_invalid` event carrying the parse error.

The board creates items from it only when a person accepts them. Accepting creates the chosen
entries in list order, each under its `parent`'s new item or under the item; `type` and `role`
that the project has are set, `team` becomes the one profile that names it in the new item's run
settings, and `depends_on` becomes blockers between the new items. An unknown type, role or team,
or a team that zero or several profiles name, is reported on its entry in the response and never
refuses the request.

`plan.example.json` is the frozen example. `plan.nested.example.json` carries the four keys.
`crates/tack-runner/tests/plan_contract.rs` round-trips both through
`tack_runner::evidence::PlanV1` and pins their bytes (FNV-1a 64).
