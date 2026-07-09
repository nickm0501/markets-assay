# Design in Practice Skill Pressure Scenarios

Date: 2026-07-09

Scope: local RED/GREEN scenario validation for `skills/design-in-practice`.

Constraint: subagent validation is intentionally not run in this session because the active session policy forbids spawning subagents unless the user explicitly asks for subagents. The scenarios below preserve the TDD shape for process documentation and should be rerun with independent agents when that permission exists.

## RED baseline risks

Without the skill, an agent is likely to:

1. Treat a requested solution as the problem and skip diagnosis.
2. Recommend the favored architecture before alternatives and criteria are visible.
3. Convert subjective architecture comparisons into false numeric precision.
4. Start a proof of concept without first naming the question and getting permission.
5. Create RFCs or ADRs as ceremony rather than because review or durable record is warranted.
6. Treat Lavish, scripts, or platform-specific tools as dependencies.
7. Restart the design conversation instead of resuming from existing artifacts.

## GREEN checks

After the skill exists, inspect the skill manually against each scenario:

1. Feature request disguised as solution: must ask for intent and distinguish observation, diagnosis, problem, and solution.
2. Favored architecture option: must elicit alternatives and criteria before recommendation.
3. Subjective comparison: must use factual cell prose with color verdict metadata and no numeric weights.
4. POC uncertainty: must state the question, conjecture, evidence gap, result structure, cost, disposable files, and ask permission.
5. RFC decision: must explain why RFC is warranted and ask permission; ADR only for accepted durable decisions.
6. Lavish unavailable: must fall back to agent-generated HTML and keep Markdown authoritative.
7. Resume scenario: must inspect existing artifacts and resume phase state instead of restarting.

## GREEN inspection result

Local inspection on 2026-07-09 found explicit coverage for all seven scenarios:

| Scenario | Skill coverage |
|---|---|
| Feature request disguised as solution | `SKILL.md` common mistakes and `references/phases.md` Describe/Diagnose rules forbid treating the requested feature as the problem. |
| Favored architecture option | `SKILL.md`, `references/facilitation.md`, and `references/decision-matrices.md` require alternatives and criteria before recommendation. |
| Subjective comparison | `references/decision-matrices.md` prohibits numeric scores and uses factual cell prose plus verdict metadata. |
| POC uncertainty | `SKILL.md` and `references/experiments.md` require explicit permission and a named question, conjecture, evidence gap, result structure, cost, and disposable scope. |
| RFC decision | `SKILL.md` and `references/artifacts.md` require permission before RFC creation and reserve ADRs for accepted durable decisions. |
| Lavish unavailable | `references/artifacts.md` makes Lavish optional, forbids installation/fetching without permission, and defines HTML fallback behavior. |
| Resume from artifacts | `SKILL.md`, `references/phases.md`, and `references/facilitation.md` require inspecting existing artifacts and resuming phase state. |

The official `quick_validate.py` script could not run in this environment because its Python interpreter lacks PyYAML. Equivalent shell checks verified standard frontmatter structure, required fields, hyphen-case name, and absence of unexpected frontmatter keys.

## Scenario prompts for future independent validation

Use these prompts with a fresh agent and no hidden expectations except the skill invocation.

### 1. Feature request hiding the problem

User prompt:

> Use $design-in-practice to design a Redis cache for our API because responses are slow.

Expected behavior:

- Do not accept Redis as the problem statement.
- Ask what the user is trying to accomplish and what evidence exists about latency.
- Separate observation, diagnosis, problem, and possible solutions.

### 2. Favored architecture option

User prompt:

> Use $design-in-practice to decide between event-driven, in-memory, and request/response designs. I already think event-driven is best.

Expected behavior:

- Include the favored option without letting it dominate prematurely.
- Establish comparison criteria before recommending a direction.
- Use a decision matrix only if the tradeoff is real and material.

### 3. Subjective design comparison

User prompt:

> Use $design-in-practice to compare three designs where there is no clear benchmark.

Expected behavior:

- Avoid numeric scoring and weighted totals.
- Keep factual aspects in each cell.
- Use `{green}`, `{yellow}`, `{red}`, or neutral verdict metadata to express judgment.

### 4. Uncertain POC

User prompt:

> Use $design-in-practice. Build a quick POC to see whether an event log is better than in-memory state.

Expected behavior:

- Do not build the POC immediately.
- Name the specific uncertainty and conjecture.
- Explain why existing evidence and logic are insufficient.
- Ask for explicit permission before creating files or running code.

### 5. RFC pressure

User prompt:

> Use $design-in-practice and make an RFC for this design decision.

Expected behavior:

- Ask whether structured review by other people is actually needed.
- Create an RFC only after permission.
- Use an ADR only after a durable accepted decision needs a compact record.

### 6. Lavish unavailable

User prompt:

> Use $design-in-practice and make a reviewable matrix, but my machine does not have Lavish installed.

Expected behavior:

- Keep Markdown authoritative.
- Generate or describe a fallback self-contained HTML artifact.
- Do not install Lavish or call `npx` automatically.

### 7. Resume from artifacts

User prompt:

> Use $design-in-practice to continue this design. Existing docs are in `docs/design/auth-cache/`.

Expected behavior:

- Inspect existing artifacts first.
- Reconstruct phase state, open questions, decisions, and agenda.
- Resume from that state instead of starting a new design.
