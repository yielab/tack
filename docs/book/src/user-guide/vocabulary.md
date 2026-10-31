# Vocabulary

Each project has its own **vocabulary**: 16 terms that rename things throughout
the UI and the command line. Two projects can use completely different language while running on the same underlying
system.

---

## The 16 terms

| Term | Default | Construction example | Homework example |
|---|---|---|---|
| `epic` | Epic | Building | Course |
| `feature` | Feature | Section | Module |
| `task` | Task | Work Order | Assignment |
| `subtask` | Subtask | Activity | Question |
| `bug` | Bug | Defect | Correction |
| `requirement` | Requirement | Specification | Rubric Item |
| `sprint` | Sprint | Phase | Week |
| `backlog` | Backlog | Pending Work | Upcoming |
| `board` | Board | Project Board | Planner |
| `blocker` | Blocker | Hold | Dependency |
| `story_points` | Story Points | Effort Hours | Effort |
| `assignee` | Assignee | Responsible | Student |
| `deliverable` | Deliverable | Deliverable | Submission |
| `phase` | Phase | Phase | Term |
| `milestone` | Milestone | Inspection Point | Exam |
| `release` | Release | Handover | Graduation |

Every term is optional. A term you leave blank uses its default label. A new project starts with the words of its project type.

---

## Changing Vocabulary

**In the UI:** **Settings → Vocabulary**, edit the fields, then **Save vocabulary**. The new words appear in every view of that project.

**Via API:**

```sh
curl -X PATCH http://localhost:3210/api/projects/{id} \
  -H "Content-Type: application/json" \
  -d '{"vocabulary":{"task":"Work Order","sprint":"Phase","epic":"Building"}}'
```

The request replaces the whole vocabulary: a term you leave out goes back to its default label, so send every term you want to keep.

**In a project file.** The `project.vocabulary` key takes any subset of the 16 terms, and the rest keep the project type's words. A file applies it only when it creates a project. A word that is not one of the 16 makes the file be refused. See [Import, Export & Project Files](import-export.md#the-project-file).

---

## Vocabulary is Per-Project

A "Sprint" in a software project and a "Phase" in a construction project are the same thing — only the label differs. Vocabulary belongs to one project and does not affect others.

---

## CLI Behavior

The CLI shows your project's words. Use `--json` to get the raw names instead:

```sh
tack list --project <id>          # shows "Work Order" instead of "Task"
tack list --project <id> --json   # returns {"item_type":"task", ...}
```

---

## Practical Tip

Set the vocabulary early: the words appear in the item forms, the view names and the command line. Changing it later is safe, because it only changes labels, but people used to the old words may be confused.

Starter vocabulary for a construction project:

```json
{
  "task":      "Work Order",
  "epic":      "Building",
  "sprint":    "Phase",
  "requirement": "Specification",
  "assignee":  "Responsible",
  "blocker":   "Hold",
  "release":   "Handover",
  "milestone": "Inspection Point"
}
```
