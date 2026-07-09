# Experiments and POCs

Use logic, existing evidence, and repository inspection before proposing an experiment. Experiment only when a named uncertainty matters and cannot be resolved credibly by reasoning or existing facts.

## Permission Gate

Ask for explicit user permission before creating files, running code, benchmarking, changing configuration, or building a POC.

Before asking, state:

- question;
- conjecture that would be supported or refuted;
- why logic and existing evidence are insufficient;
- result structure that would answer the question;
- expected effort;
- files or systems affected;
- what is disposable and what might be retained.

If permission is not granted, continue the design with the uncertainty recorded.

## Experiment Record

Create an experiment record only after permission. Use `assets/experiment-template.md`.

Required sections:

- question;
- conjecture;
- method;
- scope;
- affected files;
- observations;
- limitations;
- conclusion;
- artifact updates.

## POC Rules

A POC answers a named question. It is not early production implementation.

Do:

- keep it small and disposable;
- isolate it under the experiment directory when possible;
- record setup and observations;
- explicitly state what the POC does not prove;
- update matrices, problem statements, or design notes after the result.

Do not:

- fold POC code into production without a separate implementation workflow;
- expand the POC into extra features;
- treat subjective preference as experimental evidence;
- run benchmarks without defining what result would matter.

## Subjective Architecture Comparisons

When no clear benchmark exists, use evidence appropriate to design:

- concrete scenarios;
- failure-mode walkthroughs;
- operational stories;
- migration steps;
- review by affected maintainers;
- decision matrices with color verdicts;
- small POCs only for factual uncertainties.

Do not invent a benchmark just to make a subjective design look quantitative.
