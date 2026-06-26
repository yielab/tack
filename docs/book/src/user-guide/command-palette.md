# Command Palette & Search

Tack has two keyboard-first ways to get around: a **command palette** for jumping
to views and running actions, and a **search bar** for finding items by text.

---

## Command palette — `Ctrl+K`

Press `Ctrl+K` (or `⌘K`) anywhere to open the command palette. It opens centered,
focused, and ready for input.

- **Type to filter.** Results are grouped into sections — **Actions** (New Item,
  New Project) and **Go to** (Board, List, Table, Calendar, Timeline, Sprint,
  Overview, Project Settings) and **Workspace** (All Projects, Templates, Global
  Settings). The available commands depend on context — item/view commands appear
  only while you're inside a project.
- **Navigate** with `↑` / `↓`, **run** the highlighted command with `↵`, and
  **dismiss** with `esc`.
- The palette is also reachable from the **Search…** button in the sidebar and the
  `⌃K` button in the top bar.

## Search — `Ctrl+/`

Press `Ctrl+/` (or `⌘/`), or click the **Search items…** field in the top bar, to
search items by text.

- Searches run as you type (debounced) against the project you're in, or across the
  whole workspace when you're not scoped to a project.
- Each result shows the item's **type** badge, **priority**, and current **status**.
- `↑` / `↓` to move through results, `↵` to **open** the highlighted item — this
  deep-links straight to its detail drawer over the project board. `esc` closes the
  results.

> Search matches item titles (and other indexed fields) via the backend's
> full-text search index, so it scales to large projects.
