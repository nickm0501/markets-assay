# Facilitation

Use facilitation to expose understanding, not to perform ceremony.

## One Question at a Time

Ask one concrete question, then wait for the answer before stacking more. Prefer questions the repository cannot answer.

Good question types:

- Intent: "What should the user be able to accomplish that they cannot accomplish now?"
- Priority: "Which failure mode matters most: lost data, latency, recovery, or operational simplicity?"
- Boundary: "What is out of scope for this design?"
- Evidence: "What observation makes us believe the current approach is failing?"
- Decision: "Do you want to proceed with this direction, defer it, or keep comparing?"

Avoid asking the user for facts available in code, docs, tickets, logs, or configuration. Inspect first when safe and in scope.

## Terminology

Challenge ambiguous words immediately. Common risk words:

- event
- message
- command
- state
- cache
- workflow
- session
- realtime
- durable
- source of truth
- consistency
- user

When terms are overloaded, create a working terminology section in `design.md`. Only promote terms into a durable glossary after the user confirms the terms are domain language worth preserving.

## Socratic Checks

Use concrete scenarios to test fuzzy designs:

- "A request succeeds, then the process crashes before response delivery. What should be true after restart?"
- "Two writers update the same entity concurrently. Which state wins and why?"
- "A downstream consumer is unavailable for ten minutes. What backs up, drops, retries, or degrades?"
- "A user changes their mind midway through the flow. What state exists and who owns it?"
- "The system is redeployed while work is in progress. What must survive?"

If answers diverge, update the problem statement, criteria, or use cases before comparing solutions.

## Reflective Inquiry

Periodically restate:

```markdown
Known:
- ...

Unknown:
- ...

Current phase:
- ...

Next:
- ...
```

Use this at activation, after major new evidence, before decisions, and when resuming an interrupted design.

## Recommendation Discipline

Do not recommend a direction before:

- the problem is delimited;
- relevant alternatives are visible;
- salient criteria are visible;
- unknowns that could change the decision are either resolved or explicitly accepted.

When recommending, say what would change the recommendation.

## Phase Backtracking

Backtrack when:

- a term changes meaning;
- evidence contradicts the problem statement;
- a matrix exposes a missing criterion;
- an experiment invalidates an assumption;
- the user changes scope or priority.

State the backtrack plainly:

```text
This evidence changes the diagnosis, so I am moving back from Design to Diagnose before updating the matrix.
```
