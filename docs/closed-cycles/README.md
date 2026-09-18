# Closed cycles

Nothing under this directory is current. The current plan is Phase 64 in
`docs/book/src/roadmap.md`; `TODO.md`'s "Which board is live" table records how every
Part closed.

- `boards/part-<n>.md` — the full board section for a closed Part whose text has left
  `TODO.md` entirely (today: Parts I, II, III), moved verbatim with a one-line notice
  prepended. Numbering inside each file (`§0`…`§6` for Part I, `§II.*`, `§III.*`) is
  unchanged from when it lived in `TODO.md`.
- `boards/roadmap-phases-0-57.md` — the Phases 0–57 (Parts I–III) sections of
  `docs/book/src/roadmap.md` (the audit-driven status board, `Completed`, four shipped
  phases from `Planned`, three "Next" cycles, and the Harness-Agnostic Runner Fleet
  chapter), moved verbatim with the same one-line notice; `roadmap.md` keeps only what
  is ahead.
- `plans/` — a plan whose phase has landed or whose design was replaced (today: the
  harness maintainability audit that specified IX-M5).
- `handoffs/part-<n>/` — the per-card handoffs for a closed Part, moved out of
  `docs/agent-handoffs/part-<roman>/`.

**Parts IV through VIII are only partly here.** Their own board sections stay in
`TODO.md` itself — none of IV–IX have had
their `TODO.md` text extracted to `boards/` — but their handoffs have already moved, to
`handoffs/part-4/` through `handoffs/part-7/` (Part VIII's handoffs stay in
`docs/agent-handoffs/part-viii/`, since it still has an open card; Part IX's stay in
`docs/agent-handoffs/part-ix/`, since it is the live board). Do not assume a Part's
handoffs being here means its board section moved too, or the reverse.
