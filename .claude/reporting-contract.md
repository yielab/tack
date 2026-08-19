# How to report work

Write for a developer who does not have the whole project in their head. They should be
able to read the first three sentences and know what was built, whether it works, and
what is left — without recognizing a single file name.

## Rule 1 — talk about the capability, not the code

Say what a person can or cannot do. The file is evidence, not the subject.

- Write: **"The runner can now talk to the server, but it still cannot check out code,
  so it cannot actually run a task yet."**
- Not: "`UnavailableWorktreeProvisioner` is the only `WorktreeProvisioner` implementor."

The second sentence is true and belongs in the report — at the bottom, under technical
detail, where someone debugging will look for it.

## Rule 2 — do not report release status unless someone asked about releasing

Mid-development, "not releasable" is noise: of course it isn't, it is unfinished. The
useful question is **"is the feature we are building finished, and if not, what is
left?"** Report that instead. Save release/tag language for a card whose actual job is
shipping.

## Rule 3 — explain a blocker in three parts, in plain words

Every blocker gets one short paragraph that answers, in this order:

1. **What is missing** — in plain language.
2. **What it was supposed to do** — why that piece exists at all.
3. **What it blocks** — the concrete thing that cannot happen until it is built.

Example: *"Nothing in the runner can create a working copy of a repository. That step is
what gives each task its own isolated checkout to work in. Until it exists, a task can be
assigned to the runner but never actually starts, so none of the three coding tools can
be tested end to end."*

A reader who has never seen this codebase now understands the problem. No file was named.

## Rule 4 — one idea per paragraph, and at most one file path in it

If a paragraph names three files and two functions, split it or move it down. Density is
the main reason reports become unreadable and undebuggable.

**This applies to the technical section too.** Dense there is still unreadable — it just
has a better excuse. Give each topic its own labelled item: where the code lives, how one
mechanism works, what is technically blocking, the test numbers, what was not checked.
Someone debugging one of those should never have to read the other four.

## Rule 5 — the shape

```
## What this is about
One or two sentences. The feature in human terms. No file names.

## Where it stands
What works now. What does not work yet. Plain sentences.

## What is left
One short paragraph per item, written per Rule 3, ordered:
  - blocks the feature from working at all
  - blocks the next piece of work
  - can wait
Each says who should fix it.

## Technical detail
Broken into labelled items, one topic each — never one dense block. Use a bold label per
item so a reader can jump straight to the one they need:

  **Where the code lives** — the files added or changed, one line.
  **How <specific thing> works** — one topic, one short paragraph.
  **What is blocking, technically** — the exact type/file and why.
  **Test results** — the numbers.
  **Not checked** — what was skipped and why.

## Next step
One sentence. What to do, and the command if there is one.
```

Omit any section that is genuinely empty. Never pad.

## Rule 6 — be honest about what you did not check

"Verified" means you ran it and read the output — give the number. "Not checked" means
you did not, and it must never sit beside a green number where it reads as covered.
Never claim something is the *only* remaining problem unless you ran a search that proves
it; that claim has already shipped false here once.

## Rule 7 — finding something is good news

A card that discovers a problem in someone else's area and reports it did its job better,
not worse. Write it so it does not read as failure: name who owns the fix.
