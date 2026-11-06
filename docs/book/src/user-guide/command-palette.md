# Command Palette & Search

Tack has two keyboard-first ways to get around: a **command palette** for jumping
to views and running actions, and a **search bar** for finding items by text.

---

## Command palette — `Ctrl+K`

Press `Ctrl+K` (or `⌘K`) anywhere to open the command palette. It opens centered,
focused, and ready for input.

- **Type to filter.** Results are grouped into **Actions** (New Item, New Project), **Project**
  (Board, Orchestration, List, Calendar, Timeline, Sprint, Change sets, Threads, Overview, Factory metrics, Project settings) and
  **Workspace** (All projects, Templates, Agents, Settings). The available commands depend on
  context — item and view commands appear only while you're inside a project.
- **Items appear as you type.** Two or more letters also search the items of the project you are in
  (of every project when you are not in one). They are listed after the matching commands, under
  **Items**, each with its type, the path of what it sits under ("Checkout › Payments"), priority and
  status. `↵` opens the item over its board; pressing a name in the path opens that item instead.
- **Navigate** with `↑` / `↓`, **run** the highlighted command with `↵`, and
  **dismiss** with `esc`.
- The palette is also reachable from the **Search…** button in the sidebar and the
  `⌃K` button in the top bar.

On the Board, `n` adds an item and `r` refreshes the board.

## Search — `Ctrl+/`

Press `Ctrl+/` (or `⌘/`), or click the **Search items…** field in the top bar, to
search items by text.

- Searches run as you type, against the project you're in, or across every project when
  you're not inside one.
- Each result shows the item's **type**, the **path** of what it sits under (an epic or a building, then
  the item inside it, each a link), its **priority** and its current **status**. An item at the top of
  the tree shows no path.
- `↑` / `↓` to move through results, `↵` to **open** the highlighted item — this
  deep-links straight to its detail drawer over the project board. `esc` closes the
  results.

> Search matches an item's title, description and tags.
