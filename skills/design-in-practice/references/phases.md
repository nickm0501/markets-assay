# Design in Practice Phases

Use these phases non-linearly. The invariant is increasing understanding; the sequence is a guide, not a conveyor belt.

## Orient or Resume

Begin by reconstructing state:

- What do we know?
- What do we need to know?
- Where are we?
- Where are we going?
- What artifacts already exist?
- Which decisions are accepted, tentative, deferred, or invalidated?

Classify each phase:

| State | Meaning |
|---|---|
| `active` | Work here now. |
| `already satisfied` | Existing evidence covers the invariant. |
| `relevant later` | Do not do it yet, but keep it on the agenda. |
| `not applicable` | The problem does not need this activity. |

Skipping an activity does not skip its invariant. If a skipped phase later becomes relevant, backtrack explicitly.

## Describe

Goal: record the situation without prematurely naming the problem.

Capture:

- observed behavior, request, symptom, or opportunity;
- context and constraints;
- actors, systems, data, and timelines;
- attributed claims and their source;
- current implementation or precedent when relevant;
- missing facts.

Do not convert "build X" into "the problem is lack of X." Treat requests as evidence about intention, not proof of the problem.

## Diagnose

Goal: explain what is causing the gap or what intention is obstructed.

For defects:

- List multiple plausible causes when uncertainty exists.
- Use logic and existing evidence to eliminate causes before proposing experiments.
- State what evidence would distinguish remaining causes.

For feature requests:

- Translate the requested feature into what the user or consuming system wants to accomplish.
- Name what currently prevents that intention.
- Separate user intent from implementation mechanics.

## Delimit

Goal: make the problem small enough and clear enough to design against.

Write a solution-free problem statement:

```text
<Actor/system> cannot <intended outcome> because <cause/obstruction>, within <scope/context>.
```

Also record:

- scope;
- exclusions;
- non-goals;
- assumptions;
- accepted constraints;
- what would make the problem statement change.

## Direction

Goal: compare broad ways to address the delimited problem.

Use this phase when the team is choosing among materially different strategies, such as event-driven vs request/response, local vs distributed state, build vs buy, or in-memory vs persistent architecture.

Actions:

- Generate multiple high-level approaches.
- Include the current approach and relevant precedents when useful.
- Establish criteria before recommending.
- Use a strategy matrix only when tradeoffs are material.
- Decide whether to proceed, narrow scope, defer, stop, or select a direction.

## Design

Goal: work out how users or consuming systems accomplish their intentions and how the chosen direction can be realized.

Actions:

- Write user-intention use cases when behavior or workflow is central.
- Compare implementation approaches when the choice is non-obvious.
- Diagram architecture, flow, state, relationships, or layout when visual structure is clearer than prose.
- Resolve important unknowns through research or approved experiments.
- Preserve open questions instead of hiding uncertainty.

## Decide

Decisions can happen in any phase. Record:

- decision;
- alternatives considered;
- rationale;
- consequences;
- assumptions;
- follow-up questions;
- whether the decision is tentative or accepted.

Do not manufacture a final decision when important unknowns remain unresolved or explicitly unaccepted.

## Development Handoff

Before production implementation, confirm with the user:

- The problem statement is understood.
- The selected direction and design are understood.
- Consequential questions are answered or explicitly accepted as risks.
- POCs are recorded as evidence, not treated as production code.
- RFC or ADR needs are resolved.
- The user considers the design settled enough to hand off.

This skill stops at handoff. Use the project's normal implementation workflow for production changes.
