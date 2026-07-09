# Decision Matrices

Use a decision matrix when two or more materially different approaches have real tradeoffs. Do not create one for a trivial or already-settled choice.

## Shape

Use this structure:

- title names the decision or problem;
- columns are approaches;
- rows are criteria;
- cells are factual aspects;
- verdict metadata carries judgment.

Include the current approach when one exists, relevant precedents, and initial ideas. Let the matrix generate better or hybrid approaches.

## Markdown Syntax

```markdown
| Criterion | Current approach | Event log |
|---|---|---|
| Recovery | {red} State is lost after process failure. | {green} Replays accepted events. |
| Team familiarity | Existing implementation is understood. | {yellow} Requires event-model training. |
| Peak throughput | ? Not measured. | ? Not measured. |
```

Verdicts:

| Marker | Meaning |
|---|---|
| no marker | neutral |
| `{green}` | particularly desirable |
| `{yellow}` | challenge, weakness, or negative aspect |
| `{red}` | blocking or fails to address the problem |
| `?` | unknown and unjudged |

Use unsaturated colors when rendering HTML. Do not use color alone; include accessible labels or `data-verdict` metadata.

## Criteria Selection

Use criteria that distinguish approaches and matter to the problem. Examples are domain-dependent:

- recovery semantics;
- delivery guarantees;
- latency;
- throughput;
- consistency model;
- operational complexity;
- debuggability;
- migration cost;
- team familiarity;
- failure isolation;
- reversibility;
- user experience;
- security or compliance constraints.

Do not reuse criteria mechanically across domains. Keep only criteria that affect the decision.

## Cell Writing Rules

- Write how the approach handles the criterion.
- Prefer factual prose over adjectives.
- Avoid yes/no shorthand when approaches satisfy the same property differently.
- Put subjective judgment in the marker, not the sentence.
- Use `?` when the answer is unknown.
- Do not color unknowns.
- Do not complete a decision while important `?` cells remain unresolved or explicitly unaccepted.

## Numeric Scores

Do not use numeric scores, weights, weighted totals, or ranked totals in version one. They create false precision for subjective architecture decisions and hide disagreement behind arithmetic.

If the user asks for prioritization, use:

- color verdicts;
- short rationale;
- explicit decision notes;
- open questions;
- stated risk acceptance.

## Completion

Before deciding:

1. Check that every material approach and criterion is represented.
2. Check that cells describe facts rather than selling an answer.
3. Capture questions created by `?` cells.
4. Look for a new or hybrid approach suggested by the matrix.
5. State whether unresolved unknowns block the decision or are accepted risks.

## Failure Modes

| Failure | Correction |
|---|---|
| Matrix built around a favored answer | Add credible alternatives and criteria before recommending. |
| Rows are generic and not domain-specific | Remove criteria that do not change the decision. |
| Cells contain "good" or "bad" without facts | Rewrite the cell as observable behavior, cost, or consequence. |
| Unknowns are colored | Remove the color and add the question to the agenda. |
| Totals choose the winner | Delete totals and make the decision explicitly with rationale. |
