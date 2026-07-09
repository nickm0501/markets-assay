# Artifacts

Artifacts exist to improve reasoning, review, resumption, or durable understanding. Do not create them as ceremony.

## Default Layout

Follow existing project conventions first. If none exist:

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

Use only the files that the problem needs.

## Working Design

`design.md` is the top story and resumption point. Use `assets/design-template.md`.

Keep it current with:

- problem statement;
- status;
- agenda;
- scope and exclusions;
- phase ledger;
- working terminology;
- current direction;
- decisions;
- links to matrices, diagrams, experiments, RFCs, and ADRs.

## Use Cases

Create `use-cases.md` when the design changes what a person or consuming system can accomplish. Use `assets/use-cases-template.md`.

Write use cases in intention terms, not implementation mechanics.

## Diagrams

Create diagrams when visual structure is clearer than prose:

- architecture;
- flow;
- state;
- relationships;
- sequence;
- layout.

Prefer repo-native diagram formats already used by the project. Mermaid is a good fallback because it is text-editable.

Diagram problems as well as solutions when that improves understanding.

## RFCs

An RFC is a review artifact. Use one only when a proposal needs structured review by other people.

Before creating an RFC:

1. Explain why review is warranted.
2. Ask for explicit permission.
3. If approved, use `assets/rfc-template.md`.

RFC states:

- `Draft`
- `In Review`
- `Accepted`
- `Rejected`
- `Withdrawn`
- `Superseded`

An accepted RFC remains the full historical argument.

## ADRs

Create an ADR only when an accepted decision is independently durable:

- hard to reverse;
- surprising without context;
- based on a real tradeoff.

The ADR should reference the RFC or design artifacts instead of duplicating analysis. If a team uses an RFC-only or ADR-only convention, follow that convention.

## HTML Review Artifacts

Markdown is authoritative. HTML is generated and disposable.

For matrix HTML:

- render `{green}`, `{yellow}`, `{red}` as unsaturated backgrounds;
- remove verdict markers from visible prose;
- represent unknowns without color;
- include accessible verdict labels;
- keep the file self-contained when possible.

Use `assets/matrix-template.html` as a starting point.

## Optional Lavish Use

Detect `lavish-axi` with the active shell's normal executable lookup:

- POSIX shells: `command -v lavish-axi`
- PowerShell: `Get-Command lavish-axi`
- Windows command shell: `where lavish-axi`

If `lavish-axi` is available:

1. Read applicable Lavish guidance or playbooks if accessible locally.
2. Generate the HTML artifact using that guidance.
3. Ask before opening a local browser review session.
4. Apply feedback to canonical Markdown first.
5. Regenerate HTML after Markdown changes.

If unavailable or failing, fall back to agent-generated HTML from the bundled template.

Do not install Lavish, fetch a package-manager launcher, publish, share, or upload artifacts without explicit permission.
