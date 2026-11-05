# Types and words

A project names its work in two ways. Its **types** are the kinds of item it has (an Epic, a Bug, a Work Order), each with a name, a colour and a level. Its **vocabulary** renames ten more words that the screens and the command line use (a Sprint can be a Phase, a Backlog can be Pending Work). Two projects can use completely different language while running on the same underlying system.

---

## Types

A new project starts with the types of its project type. Every type has:

| Part | What it is |
|---|---|
| Key | The short name the files and the command line use: lowercase letters, digits, `_` and `-`. It never changes. |
| Name | What the project calls it. Free text, and changeable at any time. |
| Colour | One of the app's colours: accent, accent 2, danger, warning, success or neutral. |
| Level | **Holds work** (like an epic: it has items under it) or **Work** (like a task: something a person or an agent does). A level changes only while no item uses the type. The level decides where an item can go: a type that holds work never goes under work (see [Moving an item under another](items.md#moving-an-item-under-another)). |
| Default | Exactly one type is the default work type: what an item becomes when it is made without a type. It is always a work type. |

| Project type | Types that hold work | Types that are work (the default is first) |
|---|---|---|
| Software, Web, Mobile, Maintenance, Personal, Custom | Epic | Task, Feature, Bug, Requirement |
| Construction | Building, Section | Work Order, Activity, Defect, Specification |
| Homework | Course, Module | Assignment, Question, Correction, Rubric Item |
| Legal | Matter | Filing, Case, Issue, Statute |
| Research | Study | Protocol, Experiment, Anomaly, Hypothesis |
| Event | Event | Task, Track, Issue, Requirement |

There is no "subtask" type: a sub-item is work under another item, whatever its type.

**List them:**

```sh
tack types <project-id>
```

The table shows each type's key, name, level, colour, how many items use it, and which one is the default.

**Change them via the API.** `GET /api/projects/{id}/types` lists the types with how many items use each; `POST` adds one, `PATCH /api/projects/{id}/types/{key}` changes its name, colour, position, level or default, and `DELETE /api/projects/{id}/types/{key}?merge_into=<key>` removes one. A type that items use cannot be deleted until you say where its items go with `merge_into`: they move in the same step. The type you merge into must be at the same level.

**In a project file.** The `types` section lists them. A file that creates a project gives it exactly those types; a file that adds to a project adds the types it lacks and takes the names and colours of the ones it has. An item whose `type` is a word the project does not have gets a new work type of that name, and the preview lists it. See [Import, Export & Project Files](import-export.md#the-project-file).

---

## The ten words

| Term | Default | Construction example | Homework example |
|---|---|---|---|
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

Every word is optional. A word you leave blank uses its default label. A new project starts with the words of its project type.

---

## Changing the words

**In the UI:** **Settings → Vocabulary**, edit the fields, then **Save vocabulary**. The new words appear in every view of that project.

**Via API:**

```sh
curl -X PATCH http://localhost:3210/api/projects/{id} \
  -H "Content-Type: application/json" \
  -d '{"vocabulary":{"sprint":"Phase","backlog":"Pending Work"}}'
```

The request replaces the whole vocabulary: a word you leave out goes back to its default label, so send every word you want to keep. A key that is not one of the ten is refused; item types are not words, they are the project's types above.

**In a project file.** The `project.vocabulary` key takes any subset of the ten words, and the rest keep the project type's words. A file applies it only when it creates a project. A word that is not one of the ten makes the file be refused. See [Import, Export & Project Files](import-export.md#the-project-file).

---

## Per project

A "Sprint" in a software project and a "Phase" in a construction project are the same thing — only the label differs. Types and words belong to one project and do not affect others.

---

## CLI behavior

The CLI shows your project's type names. Use `--json` to get the keys instead:

```sh
tack list --project <id>          # shows "Work Order" instead of "task"
tack list --project <id> --json   # returns {"item_type":"task", ...}
```

`tack add -t <key>` takes any key of the project's types; without `-t` the item gets the default work type.

---

## Practical tip

Set the types and the words early: they appear in the item forms, the view names and the command line. Changing a name later is safe, because it only changes labels, but people used to the old words may be confused.

Starter words for a construction project:

```json
{
  "sprint":    "Phase",
  "assignee":  "Responsible",
  "blocker":   "Hold",
  "release":   "Handover",
  "milestone": "Inspection Point"
}
```
