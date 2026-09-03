# How to write a decision (ADRs, and anything a human must approve)

An ADR is not a handoff. A handoff explains what an agent did, to another agent or to a
reviewer who already knows the card. An ADR asks a person to say yes or no to something —
often someone who has not been following every card, wave or file this cycle touched.
Optimizing an ADR's prose for an implementing agent's context budget is a mistake: the
audience for "do you accept this" is a human, once, and they should not have to read 300
lines of cited evidence to find out what they're being asked.

## Rule 1 — the first screen is the whole ask

A reader who reads only the top of the file — no scrolling — must come away knowing three
things: what is being decided, why it needs deciding now, and what happens if they do
nothing. If they have to read past that to understand the ask itself (not the reasoning
behind it), the document has failed its one job.

## Rule 2 — plain language first, evidence second, never interleaved

Every decision gets stated once, in one or two sentences a programmer with zero context on
this cycle can follow — no card ids, no wave numbers, no file:line citations, no
"per §X.Y". Put ALL of that — the rejected alternatives, the line numbers, the cross-ADR
citations, the cost analysis — in a clearly separate section below, e.g. "Full reasoning."
A reader who trusts the summary never opens that section. One who wants to check it can.
Never make the citation part of the sentence that states the decision.

## Rule 3 — a table beats six paragraphs

When there is more than one decision in a document, list them in a short table before any
prose: decision · the answer · one-sentence why. That table alone should let most readers
decide. The paragraph-per-decision detail exists for the person who wants to argue with
one specific line, not for the person doing the first read.

## Rule 4 — name the blocker in one sentence, at the top

If something cannot proceed until this is accepted, say exactly what, in the same breath as
the ask — "nothing below runs until you accept this" — not buried in a Status line's
parenthetical or left to the last paragraph.

## Rule 5 — the shape

```
# ADR <n>: <plain-language title, not a jargon phrase>

**Decide:** <one sentence — what you're being asked to approve>
**Why now:** <one or two sentences — the concrete problem this fixes, in plain terms>
**If you do nothing:** <one sentence — what stays blocked>

## The decisions, in short

| # | Decision | Why |
|---|---|---|
| 1 | <the answer, as a plain sentence> | <one clause> |
| 2 | … | … |

## Full reasoning

*(Everything below this line is detail for whoever implements this or wants to check a
specific call. Nothing above it depends on anything below it.)*

### 1. <decision title>
<the existing depth: rejected alternatives, citations, line numbers, cost.>
```

## Rule 6 — write it for someone who was not in the room

Never assume the reader remembers which card raised this, what a prior ADR said verbatim,
or which acronym means what. If a term needs a citation to make sense, define it in one
clause inline before using it, then cite it in the reasoning section for the skeptical
reader.

## The test

Hand a programmer the first screen only. If they can say, out loud, what they're
approving and why in one sentence, the document passed. If they ask "wait, what exactly
am I saying yes to?", it failed — restructure it, don't just trim words.
