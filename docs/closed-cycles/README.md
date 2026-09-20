# Closed cycles

Nothing under this directory is current. The live plan is Phase 65,
`docs/plans/phase-65.md` (opened 2026-09-20); the roadmap points at it.

- `boards/part-1.md`, `part-2.md`, `part-3.md` — the full board sections for the closed
  Agent-Factory Control Center and Agnostic Control Plane cycles (Phases 33–49), moved
  verbatim from the old `TODO.md` with a one-line notice prepended. Section numbering
  inside each file is unchanged from when it lived there.
- `boards/part-4-9.md` — the rest of `TODO.md` (Phases 58–63), moved here whole when
  `TODO.md` left the repository (2026-09-18). Its own "Which board is live" table
  records how each cycle closed. `TODO.md` no longer exists anywhere in the tree.
- `boards/roadmap-phases-0-57.md` — the Phases 0–57 sections of
  `docs/book/src/roadmap.md` (the audit-driven status board, `Completed`, four shipped
  phases from `Planned`, three "Next" cycles, and the Harness-Agnostic Runner Fleet
  chapter), moved verbatim with a one-line notice.
- `boards/roadmap-phases-58-63.md` — the Phases 58–63 sections of
  `docs/book/src/roadmap.md` (standalone single-binary packaging, the first public
  release, agent onboarding & provider UX, the desktop app and background service, and
  human maintainability), moved verbatim with a one-line notice.
- `boards/roadmap-phase-64.md` — the Phase 64 chapter, the old "Planned" section (Phase
  21, Phase 22 Task 2) and the Phases 26–32 "Known Gaps" table, moved verbatim
  2026-09-20. `roadmap.md` itself keeps only what is ahead.
- `plans/` — a plan whose phase has landed or whose design was replaced: the harness
  maintainability audit, the human-maintainability (Phase 63) specification, the
  agnostic control plane plan whose bridge Phase 64 retired, and Phase 64's own two
  plans (`phase-64.md`, `harnesses.md`), whose open rows became Phase 65's tasks.
- `handoffs/` — the per-card handoffs for every closed cycle, moved out of the old
  `docs/agent-handoffs/`: `part-3/` through `part-7/`, `part-viii/`, `part-ix/`, plus
  `deps/` and `test-architecture/`. `docs/agent-handoffs/` no longer exists anywhere in
  the tree.

Everything here is frozen — boards, plans and handoffs are moved as-is and not edited
going forward.
