# Design in Practice Skill Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build a portable `design-in-practice` skill package that facilitates Rich Hickey's Design in Practice workflow for Codex and Claude Code.

**Architecture:** Create a standard Agent Skill at `skills/design-in-practice/` with a concise `SKILL.md`, directly linked reference files, reusable Markdown/HTML templates, and optional Codex UI metadata. Markdown remains authoritative; HTML is a disposable review artifact. No runtime scripts or required external dependencies are included.

**Tech Stack:** Markdown, standard Agent Skills layout, optional `agents/openai.yaml`, skill-creator validation scripts.

---

## File Structure

- Create `skills/design-in-practice/SKILL.md`: concise trigger, routing, invariants, reference map, and hard confirmation rules.
- Create `skills/design-in-practice/agents/openai.yaml`: optional Codex UI metadata; Claude Code must not depend on it.
- Create `skills/design-in-practice/references/phases.md`: Hickey phase guide, skip/backtrack rules, development handoff.
- Create `skills/design-in-practice/references/facilitation.md`: one-question-at-a-time facilitation, terminology, reflective inquiry, Socratic checks.
- Create `skills/design-in-practice/references/decision-matrices.md`: matrix structure, verdict metadata, color semantics, unknown handling, matrix failure modes.
- Create `skills/design-in-practice/references/experiments.md`: research/experiment/POC gating and recording.
- Create `skills/design-in-practice/references/artifacts.md`: canonical artifact layout, RFC/ADR rules, diagrams, HTML/Lavish behavior.
- Create `skills/design-in-practice/assets/design-template.md`: working design/resumption template.
- Create `skills/design-in-practice/assets/use-cases-template.md`: user-intention use case template.
- Create `skills/design-in-practice/assets/matrix-template.md`: authoritative Markdown decision matrix template.
- Create `skills/design-in-practice/assets/matrix-template.html`: self-contained HTML review template for matrix rendering.
- Create `skills/design-in-practice/assets/experiment-template.md`: approved experiment/POC record template.
- Create `skills/design-in-practice/assets/rfc-template.md`: optional RFC template.
- Create `docs/superpowers/skill-tests/2026-07-09-design-in-practice-pressure-scenarios.md`: local pressure-scenario validation record, used because subagents are not authorized in this session.

## Task 1: Record local RED pressure scenarios

**Files:**
- Create: `docs/superpowers/skill-tests/2026-07-09-design-in-practice-pressure-scenarios.md`

- [ ] **Step 1: Write pressure-scenario validation record**

Create a validation document with seven scenarios from the approved spec:

```markdown
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
```

- [ ] **Step 2: Verify the RED record exists before skill implementation**

Run:

```bash
test -s docs/superpowers/skill-tests/2026-07-09-design-in-practice-pressure-scenarios.md
```

Expected: exit code `0`.

## Task 2: Initialize skill scaffold

**Files:**
- Create: `skills/design-in-practice/SKILL.md`
- Create: `skills/design-in-practice/agents/openai.yaml`
- Create directories: `skills/design-in-practice/references/`, `skills/design-in-practice/assets/`

- [ ] **Step 1: Run skill-creator scaffold**

Run:

```bash
/Users/nickmaietta/.codex/skills/.system/skill-creator/scripts/init_skill.py design-in-practice --path skills --resources references,assets --interface display_name="Design in Practice" --interface short_description="Facilitate serious technical design decisions" --interface default_prompt="Use $design-in-practice to facilitate a technical design decision."
```

Expected: `skills/design-in-practice/` exists with `SKILL.md`, `agents/openai.yaml`, `references/`, and `assets/`.

- [ ] **Step 2: Verify scaffold**

Run:

```bash
test -f skills/design-in-practice/SKILL.md
test -f skills/design-in-practice/agents/openai.yaml
test -d skills/design-in-practice/references
test -d skills/design-in-practice/assets
```

Expected: every command exits `0`.

## Task 3: Write the skill package

**Files:**
- Modify: `skills/design-in-practice/SKILL.md`
- Modify: `skills/design-in-practice/agents/openai.yaml`
- Create: all reference and asset files listed in File Structure

- [ ] **Step 1: Replace generated placeholder content**

Replace generated placeholders with:

- `SKILL.md` containing only standard `name` and `description` frontmatter, explicit-invocation trigger, concise overview, phase router, reference map, hard confirmation gates, and common mistakes.
- Reference files containing the detailed Hickey-inspired workflow.
- Asset templates containing repo-native Markdown and self-contained HTML.
- `agents/openai.yaml` with `policy.allow_implicit_invocation: false`.

- [ ] **Step 2: Verify no placeholder text remains**

Run:

```bash
rg -n "[T]ODO|[T]BD|[P]LACEHOLDER|fill[ ]in|implement[ ]later" skills/design-in-practice docs/superpowers/skill-tests/2026-07-09-design-in-practice-pressure-scenarios.md
```

Expected: no matches.

## Task 4: Validate skill shape and portability

**Files:**
- Validate: `skills/design-in-practice/`

- [ ] **Step 1: Run skill validator**

Run:

```bash
/Users/nickmaietta/.codex/skills/.system/skill-creator/scripts/quick_validate.py skills/design-in-practice
```

Expected: validation passes.

- [ ] **Step 2: Run portability checks**

Run:

```bash
rg -n "spawn_agent|request_user_input|TodoWrite|Claude-only|Codex-only|npx -y|pip install|npm install|python script|node script|share " skills/design-in-practice
```

Expected: no platform-specific required dependency. Matches are acceptable only when the surrounding text explicitly forbids or makes the behavior optional.

- [ ] **Step 3: Verify no runtime scripts exist**

Run:

```bash
test ! -d skills/design-in-practice/scripts
```

Expected: exit code `0`.

## Task 5: Run GREEN scenario inspection

**Files:**
- Inspect: `skills/design-in-practice/SKILL.md`
- Inspect: `skills/design-in-practice/references/*.md`
- Inspect: `skills/design-in-practice/assets/*.md`
- Inspect: `skills/design-in-practice/assets/matrix-template.html`

- [ ] **Step 1: Check each pressure scenario against skill instructions**

Run:

```bash
rg -n "solution as the problem|alternatives|numeric|POC|RFC|Lavish|resume|backtrack|unknown|permission|ask before" skills/design-in-practice
```

Expected: each pressure scenario has explicit handling in the skill package.

- [ ] **Step 2: Check matrix semantics**

Run:

```bash
rg -n "\\{green\\}|\\{yellow\\}|\\{red\\}|numeric|weights|false precision|unknown" skills/design-in-practice/references/decision-matrices.md skills/design-in-practice/assets/matrix-template.md skills/design-in-practice/assets/matrix-template.html
```

Expected: color verdicts and unknown handling are documented; numeric scores/weights are prohibited in version one.

## Task 6: Review and commit

**Files:**
- Review all files created by this plan.

- [ ] **Step 1: Run whitespace check**

Run:

```bash
git diff --check
```

Expected: no whitespace errors.

- [ ] **Step 2: Review diff**

Run:

```bash
git diff -- docs/superpowers/plans/2026-07-09-design-in-practice-skill.md docs/superpowers/skill-tests/2026-07-09-design-in-practice-pressure-scenarios.md skills/design-in-practice
```

Expected: diff includes only the implementation plan, local validation record, and new skill package.

- [ ] **Step 3: Commit only this work**

Run:

```bash
git add docs/superpowers/plans/2026-07-09-design-in-practice-skill.md docs/superpowers/skill-tests/2026-07-09-design-in-practice-pressure-scenarios.md skills/design-in-practice
git commit -m "feat: add design-in-practice skill"
```

Expected: commit succeeds without staging unrelated untracked files.
