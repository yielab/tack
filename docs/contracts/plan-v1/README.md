# plan-v1

`tack-plan.json` is the Planner profile's proposal: a list of subtasks, each with a title, a
description, optional acceptance sentences, optional `depends_on` indexes into the same list and
an optional `S`/`M`/`L` estimate. The Planner profile's instructions tell the agent to write it at
the workspace root. The runner reads it after capture, stages it as an artifact of kind `plan`
(media type `application/vnd.tack.plan+json`) and keeps it out of the patch; a file that does not
parse yields no artifact and one `attempt.plan_invalid` event carrying the parse error. The board
creates subtasks from it only when a person accepts them.

`plan.example.json` is the frozen example. `crates/tack-runner/tests/plan_contract.rs` round-trips
it through `tack_runner::evidence::PlanV1` and pins its bytes (FNV-1a 64).
