# Closed cycles

Nothing under this directory is current — the live board is `TODO.md`, whose "Which board
is live" table at the top says the state of every Part today.

- `boards/part-<n>.md` — the full board section for a closed Part whose text has left
  `TODO.md` entirely (today: Parts I, II, III), moved verbatim with a one-line notice
  prepended. Numbering inside each file (`§0`…`§6` for Part I, `§II.*`, `§III.*`) is
  unchanged from when it lived in `TODO.md`.
- `handoffs/part-<n>/` — the per-card handoffs for a closed Part, moved out of
  `docs/agent-handoffs/part-<roman>/`.

**Parts IV through VIII are only partly here.** Their own board sections stay in
`TODO.md` itself — VIII still has one open card (VIII-C3), and none of IV–VII have had
their `TODO.md` text extracted to `boards/` — but their handoffs have already moved, to
`handoffs/part-4/` through `handoffs/part-7/` (Part VIII's handoffs stay in
`docs/agent-handoffs/part-viii/`, since it still has an open card; Part IX's stay in
`docs/agent-handoffs/part-ix/`, since it is the live board). Do not assume a Part's
handoffs being here means its board section moved too, or the reverse.
