# Design in Practice Skill Design

Date: 2026-07-09

Status: Approved for implementation planning

## Objective

Create a reusable `design-in-practice` Agent Skill that facilitates Rich
Hickey's process from *Design in Practice*. The skill should increase and
externalize understanding before production implementation, while preserving
the process's non-linear character and its emphasis on precise language,
questions, multiple alternatives, experiments, decision matrices, and useful
artifacts.

The skill is explicitly invoked. It is not an automatic ceremony for every
design question.

## Portability

The same skill package must work in both Codex and Claude Code.

- Keep the complete workflow in the standard `SKILL.md` and directly referenced
  resources.
- Use only standard `name` and `description` frontmatter in `SKILL.md`.
- Do not require platform-specific sub-skills, slash commands, planning modes,
  question tools, subagents, or proprietary tool names.
- Describe capabilities semantically: inspect files, edit Markdown, run a shell
  command, or open a local review session when those capabilities exist.
- Use plain conversational questions so either agent can facilitate the
  process.
- Treat `agents/openai.yaml` as optional Codex UI metadata. Claude Code must not
  need it.
- Detect optional executables using the active shell's native lookup mechanism
  rather than assuming a particular operating system.
- Provide a fallback whenever an optional capability is absent.

The skill may be invoked as `$design-in-practice`,
`/design-in-practice`, or an explicit natural-language request to run the
Design in Practice workflow.

## Core Principles

- Measure design progress by increasing understanding, not accumulating
  implementation.
- Keep observations, diagnoses, problems, approaches, decisions, and
  implementation distinct.
- Use precise, consistent words and define domain terms when needed.
- Formulate explicit questions before seeking answers.
- Generate more than one hypothesis or approach when the answer is uncertain.
- Use logic and existing evidence before conducting experiments.
- Treat a decision matrix as a generative comparison surface, not a scoring
  algorithm or shopping table.
- Keep factual aspects in matrix prose and subjective judgment in color
  metadata.
- Backtrack explicitly whenever new evidence invalidates an earlier conclusion.
- Create only artifacts that improve reasoning, review, resumption, or durable
  understanding.
- Do not begin production implementation until the user confirms the design is
  settled.

## Facilitated Workflow

The skill preserves Hickey's phases:

```text
Orient
  |
Describe -> Diagnose -> Delimit -> Direction -> Design -> Development handoff
    ^                                                       |
    +---------------- explicit backtracking ----------------+

Decide may occur at any point: proceed, narrow scope, defer, or stop.
Experiments may occur wherever an unresolved question requires evidence.
```

At activation or resumption, perform reflective inquiry:

- What do we know?
- What do we need to know?
- Where are we?
- Where are we going?

Classify every phase as `active`, `already satisfied`, `relevant later`, or
`not applicable`. A phase may be skipped when existing evidence satisfies it or
when it is irrelevant. Skipping an activity does not permit skipping its
invariant.

### Describe

Record the situation, context, observations, symptoms, requests, and attributed
claims. Do not assert the problem or accept a requested feature as proof of the
problem.

### Diagnose

For defects, name multiple plausible causes when uncertainty exists, use logic
to rule them out, and identify evidence needed to distinguish them.

For feature requests, translate the proposed feature into the user's intention:
what they want to accomplish or make different, and what currently obstructs
them.

### Delimit

Write a succinct, solution-free statement of the unmet objective and its
cause. Define what the current effort will and will not solve. Refine the
problem statement as understanding changes.

### Direction

Describe user intentions without implementation mechanics. Generate multiple
high-level approaches. Use a strategy decision matrix only when materially
different approaches have real tradeoffs. Decide whether to proceed, change
scope, defer, stop, or select a direction.

### Design

Determine how users or consuming systems accomplish their intentions. Compare
implementation approaches when the choice is non-obvious. Create diagrams when
architecture, flow, state, relationships, or layout are clearer visually.
Resolve important unknowns through research or approved experiments.

### Development Handoff

Confirm that the why and how are understood, consequential questions are
resolved or explicitly accepted, and the user considers the design settled.
Hand the artifacts to a separate implementation workflow. The skill does not
perform production implementation.

## Interaction Rules

- Ask one question at a time.
- Inspect the repository for discoverable facts instead of asking the user.
- Ask the user about intent, priorities, constraints, scope, and decisions.
- Challenge ambiguous or conflicting terminology immediately.
- Use concrete scenarios to expose fuzzy concepts and boundaries.
- Do not recommend a direction before relevant alternatives and criteria are
  visible.
- Treat recommendations as hypotheses until supported by the design work.
- Record user decisions, rationale, consequences, and assumptions.
- State explicitly when moving backward to an earlier phase and why.
- Resume from existing artifacts rather than restarting the process.

## Conditional Techniques

| Technique | Apply when |
|---|---|
| Glossary | Terms are overloaded, ambiguous, conflicting, or domain-specific |
| Multiple hypotheses | A cause or underlying problem is uncertain |
| Scientific experiment | Logic and existing evidence cannot resolve a factual question |
| User-intention use cases | The design changes what a person or consuming system can accomplish |
| Decision matrix | Two or more materially different approaches have real tradeoffs |
| Diagram | Visual structure communicates architecture, flow, state, relationships, or layout more clearly |
| POC | Code is the cheapest credible way to resolve a named uncertainty |
| RFC | A proposal needs structured review by other people |
| ADR | An accepted decision is hard to reverse, surprising without context, and based on a real tradeoff |

Do not create an artifact merely because its technique exists.

## Canonical Artifacts

Follow an existing project documentation convention when one exists. Otherwise
use:

```text
docs/design/<topic>/
├── design.md
├── use-cases.md                 # optional
├── RFC.md                       # optional, only after confirmation
├── matrices/
│   ├── 01-strategy.md           # authoritative
│   └── 01-strategy.html         # generated review artifact
├── experiments/                 # only after confirmation
│   └── 01-<question>/
│       ├── experiment.md
│       └── poc/                 # optional disposable code
└── diagrams/                    # optional
    └── <name>.mmd
```

### Working Design

`design.md` is the top story and resumption point. It contains, when relevant:

- Title
- Description
- Problem statement
- Status: what is known and where the work is
- Agenda: what remains unknown and where the work is going
- Scope and exclusions
- Current direction or approach
- Phase ledger
- Working terminology
- Decisions and links to supporting artifacts

Unsettled terminology remains in the working design. After the user confirms a
term is durable domain language, offer to update an existing glossary or create
`GLOSSARY.md`.

### Decision Matrix

Keep the decision or problem visible. Put approaches in columns and criteria in
rows. Include the current approach when one exists, relevant precedents, and
initial ideas. Allow comparison to generate new or hybrid approaches.

Use only salient and relevant criteria. Each cell describes how one approach
handles one criterion. Avoid yes/no shorthand when approaches achieve the same
property differently.

Canonical Markdown syntax:

```markdown
| Criterion | Current approach | Event log |
|---|---|---|
| Recovery | {red} State is lost after process failure. | {green} Replays accepted events. |
| Team familiarity | Existing implementation is understood. | {yellow} Requires event-model training. |
| Peak throughput | ? Not measured. | ? Not measured. |
```

Verdicts:

- no tag: neutral
- `{green}`: particularly desirable
- `{yellow}`: a challenge or negative aspect
- `{red}`: blocking or failing to address the problem
- `?`: unknown and unjudged

Keep judgment in verdict metadata rather than cell prose. Neutral is the
default. Unknown cells must not have a color. Do not use numeric scores or
weights in version one.

Complete cells before recording the decision. Do not reverse-engineer the
matrix around a favored answer. Capture questions immediately and make every
important `?` part of the design agenda.

### Experiments and POCs

Before proposing an experiment, state:

- The question
- A supporting or refuting conjecture
- Why logic and existing evidence are insufficient
- The result structure that would answer the question
- Expected effort and files affected
- What will be disposable and what might be retained

Ask for explicit permission before creating or running a POC. A POC answers a
named question; it is not an early production implementation. Record its
method, observations, limitations, and conclusion, then update the relevant
design artifacts.

### RFCs and ADRs

An RFC is a review artifact. It may contain the problem, scope, alternatives,
matrices, diagrams, experiments, open questions, and proposed direction. RFC
states are `Draft`, `In Review`, `Accepted`, `Rejected`, `Withdrawn`, and
`Superseded`.

Explain why an RFC is warranted and ask for explicit permission before creating
one.

An accepted RFC remains the complete historical argument. Create an ADR only
when an individual accepted decision is independently durable: hard to reverse,
surprising without context, and a genuine tradeoff. The ADR references the RFC
instead of duplicating its analysis. A team's established RFC-only or ADR-only
convention takes precedence.

## HTML Review Artifacts

Markdown is authoritative. HTML is disposable and may always be regenerated.
HTML renders verdict metadata as unsaturated background colors, removes verdict
tags from visible prose, and includes accessibility metadata for each verdict.

Do not require Python, Node, or another runtime to generate HTML. The agent can
populate the bundled self-contained HTML template directly.

### Optional Lavish Integration

Detect the `lavish-axi` executable using the active shell's native executable
lookup.

When available:

1. Read each applicable Lavish playbook, including comparison, table, plan, or
   diagram guidance as appropriate.
2. Generate the HTML artifact using that guidance.
3. Ask before opening a Lavish browser review session.
4. If approved, open the artifact locally and process feedback.
5. Apply feedback to canonical Markdown first, then regenerate HTML.

When unavailable or failing, generate HTML from the bundled template. Lavish is
an enhancement, never a dependency or blocker.

Do not automatically install Lavish or invoke `npx`. Never use Lavish sharing
or another external publication mechanism without explicit permission.

## Skill Package

Use progressive disclosure:

```text
design-in-practice/
├── SKILL.md
├── agents/
│   └── openai.yaml
├── references/
│   ├── phases.md
│   ├── facilitation.md
│   ├── decision-matrices.md
│   ├── experiments.md
│   └── artifacts.md
└── assets/
    ├── design-template.md
    ├── use-cases-template.md
    ├── matrix-template.md
    ├── matrix-template.html
    ├── experiment-template.md
    └── rfc-template.md
```

`SKILL.md` is the concise phase router. Detailed techniques load only when
relevant. Templates are copied selectively. The skill has no scripts or
required external dependencies.

## Validation Strategy

Follow RED-GREEN-REFACTOR for process documentation.

Baseline-test agents without the skill on at least:

1. A feature request that disguises a solution as the problem.
2. An architecture decision where one option is already favored.
3. Unknown matrix cells that might require a POC.
4. A simple decision that should not trigger the full process.
5. A review-worthy proposal where the agent must ask before creating an RFC.
6. A session where Lavish is unavailable.
7. A resume scenario using existing design artifacts.

Then run the same scenarios with the skill. Verify:

- The agent distinguishes observation, diagnosis, problem, and solution.
- The agent generates multiple alternatives when appropriate.
- The agent uses factual matrix prose and color metadata correctly.
- The agent asks before POCs, RFCs, and Lavish browser sessions.
- The agent skips irrelevant techniques without skipping their invariants.
- The agent backtracks when evidence changes the design.
- Both Codex and Claude Code can follow the package without platform-specific
  dependencies.

Validate the final folder with the skill creator's validator. Inspect all
generated artifacts and run a portability review for platform-specific
assumptions before installation.

