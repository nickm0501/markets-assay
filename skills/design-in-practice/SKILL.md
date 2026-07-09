---
name: design-in-practice
description: Use when the user explicitly invokes design-in-practice, $design-in-practice, /design-in-practice, or asks to run Rich Hickey's Design in Practice workflow for a technical design, architecture decision, RFC, ADR, decision matrix, POC, or subjective design comparison.
---

# Design in Practice

Facilitate technical design as the deliberate growth of understanding before production implementation. Keep observations, diagnoses, problem statements, alternatives, decisions, experiments, and implementation handoff distinct.

## Activation

Use this skill only for explicit invocation. Do not turn ordinary design questions into this workflow unless the user asks to run `design-in-practice`, `$design-in-practice`, `/design-in-practice`, or Rich Hickey's Design in Practice process.

On activation or resumption:

1. Inspect existing project artifacts before asking for facts that can be found.
2. Ask reflective inquiry: what do we know, what do we need to know, where are we, where are we going?
3. Classify each phase as `active`, `already satisfied`, `relevant later`, or `not applicable`.
4. Ask one question at a time.
5. Record decisions, rationale, assumptions, consequences, and open questions.

## Phase Router

Read [references/phases.md](references/phases.md) before facilitating the workflow.

| Phase | Purpose |
|---|---|
| Describe | Record the situation, observations, symptoms, requests, and attributed claims without asserting the problem. |
| Diagnose | Explain likely causes or translate a feature request into user intention and obstruction. |
| Delimit | State the solution-free unmet objective, cause, scope, and exclusions. |
| Direction | Compare high-level directions and decide whether to proceed, narrow, defer, stop, or select a direction. |
| Design | Work out how users or systems accomplish intentions, compare implementation approaches, diagram, and resolve important unknowns. |
| Development handoff | Confirm the design is settled before separate production implementation begins. |

Backtrack whenever new evidence changes earlier understanding. State the phase you are moving back to and why.

## Load References As Needed

- Use [references/facilitation.md](references/facilitation.md) for terminology, Socratic questioning, concrete scenarios, and one-question-at-a-time interview discipline.
- Use [references/decision-matrices.md](references/decision-matrices.md) when two or more materially different approaches have real tradeoffs.
- Use [references/experiments.md](references/experiments.md) before research spikes, scientific experiments, or POCs.
- Use [references/artifacts.md](references/artifacts.md) when creating design docs, use cases, diagrams, RFCs, ADRs, or HTML review artifacts.

## Hard Gates

Ask for explicit user confirmation before:

- creating or running a POC, experiment, benchmark, or disposable implementation;
- creating an RFC;
- opening a local browser review session with Lavish or another review tool;
- publishing, sharing, uploading, or externally exposing any artifact;
- starting production implementation.

If permission is not granted, continue with reasoning, diagrams, matrices, or artifact drafts that do not require the gated action.

## Canonical Artifacts

Follow the project's existing documentation convention when one exists. Otherwise use:

```text
docs/design/<topic>/
├── design.md
├── use-cases.md
├── RFC.md
├── matrices/
│   ├── 01-strategy.md
│   └── 01-strategy.html
├── experiments/
│   └── 01-<question>/
│       ├── experiment.md
│       └── poc/
└── diagrams/
    └── <name>.mmd
```

Only create artifacts that improve reasoning, review, resumption, or durable understanding.

Copy templates from `assets/` selectively:

- `design-template.md`
- `use-cases-template.md`
- `matrix-template.md`
- `matrix-template.html`
- `experiment-template.md`
- `rfc-template.md`

Markdown is authoritative. HTML is disposable and may be regenerated.

## Common Mistakes

| Mistake | Correction |
|---|---|
| Treating the requested feature as the problem | Diagnose the intention and obstruction first. |
| Recommending before alternatives and criteria are visible | Generate credible alternatives and compare salient criteria first. |
| Using weighted numeric scores for subjective design | Use factual matrix prose plus color verdict metadata. |
| Running a POC to explore generally | Name the question, conjecture, evidence gap, result structure, cost, and ask permission. |
| Creating RFCs or ADRs as ceremony | Use RFCs for needed review; use ADRs for accepted durable decisions. |
| Depending on platform-specific tools | Describe capabilities semantically and provide a fallback. |
| Restarting an existing design | Inspect current artifacts and resume phase state. |
