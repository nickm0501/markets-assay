# Design: Correlation-First Market Sentiment Pipeline

Status: Active (living)

This is the resumption point for the correlation-first pipeline. It supersedes
the frozen spec and dated plans as the source of truth for terminology and
cross-cutting decisions. Specs and plans stay dated and unedited as historical
record; corrections and durable decisions that affect more than one stage live
here.

## Reflective Inquiry

### Known

- The objective, free data sources, architecture, data flow, and V1 fixed
  decisions in `docs/superpowers/specs/2026-07-08-correlation-first-pipeline-design.md`
  are approved and internally coherent.
- The Stage 0 fixture plan (`docs/superpowers/plans/2026-07-09-stage-0-fixture-pipeline.md`)
  is a fully coded, task-by-task implementation plan, not yet executed (repo
  has only an untracked `cargo init` scaffold — `src/main.rs` is still
  `Hello, world!`).
- `run_id` is defined in the spec as the identifier for **one** analysis or
  backtest configuration and result. The Stage 0 plan's Architecture section
  states later stages reuse the analysis/backtest/report loop unchanged and
  only swap the data source — so whatever grain Stage 0 bakes into that loop
  becomes the grain for every later stage.
- An audit of the Stage 0 plan against the spec surfaced 6 gaps plus one minor
  clarity fix (`run_all` hardcoding `cost_bps`). All 6 gaps are resolved:
  Decisions 1-6 above.
- A second-round adversarial review (spec-compliance + logic-gaps) against the
  corrected plan surfaced 4 more durable spec-compliance gaps (`--run-id`
  overrides, `runs/<run_id>/` manifest files, per-side backtest metrics,
  `extreme_sentiment`; see the four 2026-07-10 decisions above) plus several
  plan-local defects fixed in place without a durable decision: a broken
  `news_window_uses_available_at_and_excludes_future_articles` test whose
  fixture data could never satisfy its own assertions, a config-validation
  gap that let `run_all` index empty configured-value vectors and panic,
  a hardcoded `source_catalog` that under-reported the fixture's actual
  sources, and prose-only helper-refactor guidance in Task 9 that risked a
  double-printed `dataset_id`/`observation_set_id`. A manually-traceable
  fixture-to-trade test (Required Tests) was also added to Task 8.

### Unknown

- Whether a 1-year development / 1-year holdout split is sufficient for an
  initial go/no-go decision, and whether the stop/revise/continue gates are
  strong enough to act on — both flagged by the Lavish design review
  (`.lavish/correlation-first-pipeline-review.html`) and never explicitly
  answered beyond the spec's acknowledgment that two years "cannot establish
  robustness across every market regime." Deliberately deferred until Stage
  0/2 mechanics are proven.

### Current Phase

- active: Development Handoff — all 6 audited gaps in the Stage 0 plan are
  resolved or explicitly accepted (see Decisions). Remaining step is explicit
  user confirmation the plan is settled enough to start implementation.
- already satisfied: Describe, Diagnose, Delimit, Direction (staged delivery
  0-4 chosen and recorded in the spec), Design (Stage 0 plan audited and
  corrected).
- relevant later: Direction/Design for Stages 1-4 (not yet planned in detail);
  the 1-year holdout/decision-gate-strength question before Stage 3.
- not applicable: none identified yet.

### Next

- Confirm with the user whether the corrected Stage 0 plan is settled enough
  to hand off to implementation (per this skill's Development Handoff gate).

## Description

Build a local, rerunnable research pipeline that determines whether historical
news sentiment contains useful out-of-sample information about subsequent U.S.
equity and ETF returns, without live trading, on a zero vendor budget, using a
frozen two-year free-data window (one development year, one untouched
holdout year).

## Problem Statement

A researcher cannot determine whether news sentiment has tradable predictive
value for U.S. equities/ETFs because no rerunnable, leakage-controlled,
cost-aware pipeline exists to test that cheaply, within a zero-dollar vendor
budget and a two-year free-data history ceiling.

## Scope

### In Scope

- Fixture-first, then curated-sample, then narrow free-source probe, then a
  full two-year hypothesis test (Stages 0-3 in the spec's Delivery Stages).
- Deterministic local sentiment scoring, symmetric long/short/flat strategy
  rules, immutable dataset/observation lineage, decision-oriented reports.

### Out of Scope (V1 Non-Goals, from the spec)

- Live trading or brokerage execution, interactive dashboards, tick-level
  data, futures/options/exact benchmark-index feeds, portfolio optimization,
  supervised/deep-learning models, mandatory social-media ingestion,
  production-grade proof across many market regimes.

## Working Terminology

Canonical from here forward. The spec's "Core Terminology" section is the
historical origin of these definitions; this table is the one to update.

| Term | Meaning | Notes |
|---|---|---|
| `news_window` | Historical articles included in a signal (e.g. trailing 1h, trailing 4h, session-to-now). | |
| `signal_time` | Time at which an aggregated signal is available for analysis or trading. | |
| `measurement_horizon` | Future market period measured after `signal_time`. | |
| `price_interval` | OHLCV bar duration. | |
| `source_set` | Configured source mixture used to create an observation (`finance_only`, `broad_news`, `finance_plus_broad`). | |
| `dataset_id` | Immutable identifier for a versioned input snapshot. | |
| `observation_set_id` | Immutable identifier for observations derived from one dataset and one aggregation configuration. | |
| `run_id` | Identifier for **one** analysis or backtest configuration and result. | See Decision 1 — this is singular by design, not a blended/pooled result. |
| `configuration` | One `(news_window, measurement_horizon, source_set)` triple (and, within backtest, one cost/threshold pair). | Introduced by Decision 1 to name the grain that must not be pooled. |
| `available_at` | Time Stage 0 allows a signal to use an article. Equals `published_at` for regular-hours articles; deferred to the next regular-session signal time for after-hours articles. | |
| `entry_time` | Equals `signal_time`, i.e. the open of the tradable bar whose open is at or after every eligible article's `available_at`. | See Decision 4 — this satisfies no-lookahead without shifting to a separate following bar. |

## Current Direction

Staged delivery, per the spec: Stage 0 (deterministic fixture pipeline) →
Stage 1 (hand-curated real sample) → Stage 2 (narrow Massive/GDELT/Alpaca
probe) → Stage 3 (free two-year hypothesis test) → Stage 4 (paid
history/modeling only if Stage 3 justifies it). Stage 0 is planned in full;
Stages 1-4 are named but not yet planned in detail.

## Decisions

| Date | Decision | Status | Rationale | Consequences | Links |
|---|---|---|---|---|---|
| 2026-07-09 | `analysis.rs`, `backtest.rs`, and `report.rs` must operate per `configuration` (`news_window` × `measurement_horizon` × `source_set`), never pooled across configurations. | Accepted | The spec defines `run_id` as one configuration's result, and the Stage 0 plan states later stages reuse this loop unchanged — pooling in Stage 0 would silently become the permanent behavior for the real two-year experiment, defeating the First Experiment Matrix's purpose of comparing configurations. | Requires reworking Stage 0 plan Tasks 7, 8, 9 before implementation starts: per-configuration quantile thresholds and bucket returns, per-configuration trade streams (no cross-configuration slot competition in `last_exit_by_symbol`), and a report that lists one recommendation per configuration instead of one blended verdict. | Stage 0 plan `docs/superpowers/plans/2026-07-09-stage-0-fixture-pipeline.md` Tasks 7-9 |
| 2026-07-09 | `calendar.rs` must compute NYSE open/close in America/New_York local time via `chrono-tz` and convert to UTC per date (not a fixed UTC offset), and `regular_close` must honor the `early_closes` config map. | Accepted | `normalize.rs` (which calls `is_regular_session`/`next_regular_signal_time`) is part of the shared research loop later stages reuse unchanged. Stage 2/3 ingest a full two-year real window that necessarily crosses the US DST boundary twice a year — a fixed-UTC-offset calendar would silently misclassify session/after-hours status for roughly half of any real dataset, not just a Stage 0 fixture edge case. | Reworked Stage 0 plan Task 2 (`calendar.rs`), plus call-site updates in `fixture.rs` and `normalize.rs` to thread `early_closes` through. Added DST-crossing and early-close tests alongside the existing weekend/holiday test. | Stage 0 plan `docs/superpowers/plans/2026-07-09-stage-0-fixture-pipeline.md` Task 2 |
| 2026-07-09 | `build_one` must sum contiguous price bars spanning `signal_time` through `exit_time` for measurement horizons wider than one `price_interval`, and must drop (not silently truncate) any horizon a session close or gap prevents from being fully covered. | Accepted | The spec's First Experiment Matrix requires horizons like "next four hours" over one-hour bars; the exact-single-bar-match design silently produced zero observations for any non-matching horizon, which is the same "do not silently drop rows" violation the spec warns against elsewhere. | Reworked Stage 0 plan Task 6 (`observations.rs`) `build_one` to aggregate open/high/low/close across contiguous future bars and validate full coverage before producing a row. Added a test proving a 240-minute horizon aggregates 4 bars mid-session and is dropped near session close. | Stage 0 plan `docs/superpowers/plans/2026-07-09-stage-0-fixture-pipeline.md` Task 6 |
| 2026-07-09 | `entry_time == signal_time` (trading the bar whose open is at or after every eligible article's `available_at`) is correct and intentional. The spec's phrase "enter at the next configured tradable bar after signal_time" is imprecise, not the implementation. | Accepted | `available_at <= signal_time == entry_time` for every eligible article already guarantees no lookahead — the entry never uses information published after the trade. "Next configured tradable bar" reads more literally as "the next bar, period," which would be an unnecessarily conservative and undocumented latency assumption nothing else in the spec asks for. | The spec file itself is left unedited (frozen/historical per this doc's header); this entry is the canonical wording going forward. Added a dedicated no-lookahead test (`no_observation_ever_uses_an_article_published_after_its_entry_time`) to Task 6 so this guarantee is verified directly rather than inferred from one example. | Stage 0 plan `docs/superpowers/plans/2026-07-09-stage-0-fixture-pipeline.md` Task 6; spec `docs/superpowers/specs/2026-07-08-correlation-first-pipeline-design.md` "Backtest Rules" (wording superseded here, file unedited) |
| 2026-07-09 | `is_after_hours_signal` and `market_session` must be computed per observation, not hardcoded. `is_after_hours_signal` = true when any eligible article's `available_at != published_at` (i.e. it was deferred); `market_session` = whether `signal_time` itself falls in the regular session. | Accepted | The fixture deliberately includes an after-hours article to exercise this path; hardcoding the fields to `false`/`"regular"` meant that scenario was silently discarded rather than verified. | Reworked Stage 0 plan Task 6 `build_one` to compute both fields (imports `calendar::is_regular_session`). Extended the existing after-hours test to assert `is_after_hours_signal` is true for the observation containing the deferred article and false for one that doesn't. Note: Stage 0's fixture bars are always regular-session bars, so `market_session` will read `"regular"` for every Stage 0 row even though it's now a real computation — non-`"regular"` values only appear once a later stage ingests pre-market/after-hours price bars. | Stage 0 plan `docs/superpowers/plans/2026-07-09-stage-0-fixture-pipeline.md` Task 6 |
| 2026-07-09 | Stage 0 intentionally implements only 2 of the spec's 5 decision values (`continue`/`revise`, not `stop`/`expand data`/`expand sources`) and 1 of its 4 baselines (shuffled sentiment, not flat/random/momentum). The full sets are a gate before Stage 2/3 reports are used for a real go/no-go call, not a Stage 0 deliverable. | Accepted | Stage 0's own scope list never asked for a real verdict, only proof that the mechanics work end-to-end on synthetic data. The fixture has no real data-quality signal to react to (nothing to justify `expand data`/`expand sources`) and no meaningful random-seed policy to validate — building the full set now would be unverifiable ceremony. | Rewrote the Stage 0 plan's Scope section to state this explicitly as a gate, replacing a stale line that implied Stage 0 already produced the full 5-value vocabulary. | Stage 0 plan `docs/superpowers/plans/2026-07-09-stage-0-fixture-pipeline.md` Scope section |
| 2026-07-09 | (No action needed — resolved as a byproduct.) `run_all` no longer hardcodes `cost_bps = 5.0`; it uses `config.costs_bps.first()`. Sweeping multiple costs within one `run` invocation was not implemented, matching the spec's own Decision Demo flow ("change one... cost assumption, rerun without downloading source data" — a separate `backtest --cost-bps` call, which `backtest_reruns_with_changed_cost_without_rebuilding_dataset` already covers). | Accepted | Decision 1's rework of `run_all` (to sum backtests per configuration) happened to replace the hardcoded `5.0` with a config-driven value along the way. | None — flagged during audit, found already fixed. | Stage 0 plan `docs/superpowers/plans/2026-07-09-stage-0-fixture-pipeline.md` `run_all` |
| 2026-07-10 | `run`, `analyze`, and `backtest` accept an optional `--run-id` CLI override (default: `config.run_id`) that names the `runs/<run_id>/` directory for that single invocation. | Accepted | The spec defines `run_id` as the identifier for "one analysis or backtest configuration and result" and this doc's `configuration` term explicitly includes "one cost/threshold pair" within backtest — but `run_id` was a single static config value, so every `backtest --cost-bps <n>` rerun overwrote the previous cost's `runs/<run_id>/reports/*.csv`, silently breaking the spec's own Decision Demo step 6 ("compare both runs"). | Stage 0 plan Task 1 (`cli.rs`: `RunArgs`, `run_id` field on `StageArgsWithObservationSet`), Task 7 (`run_analyze`), Task 8 (`run_backtest_command`, plus a new `distinct_run_ids_keep_separate_backtest_reports_for_comparison` CLI test), and Task 9 (`run_all`) all now take an explicit effective `run_id` instead of reading `config.run_id` directly. Default (no `--run-id`) behavior is unchanged, so no prior test needed to change its expected path. | Stage 0 plan `docs/superpowers/plans/2026-07-09-stage-0-fixture-pipeline.md` Tasks 1, 7, 8, 9 |
| 2026-07-10 | `runs/<run_id>/` must also contain `config.json`, `dataset_manifest.json`, and `observation_set_manifest.json`, copied in by a shared `write_run_manifests` helper every time `analyze`, `backtest`, or `run` writes into that directory. | Accepted | The spec's Stored Data section lists these three files under every run, but no Stage 0 task wrote them — an audit gap, not a deliberate scope cut. `config.json` records the *effective* `run_id` (post `--run-id` override), not the raw value loaded from the config file, so the file matches the directory it lives in. | Stage 0 plan Tasks 7-9 (`write_run_manifests`, called from `run_analyze`, `run_backtest_command`, `run_all`) and Task 10 Step 6's expected file listing. | Stage 0 plan `docs/superpowers/plans/2026-07-09-stage-0-fixture-pipeline.md` Tasks 7-10 |
| 2026-07-10 | `BacktestMetrics` reports long and short sides separately (`long_gross_return_sum`, `long_net_return_sum`, `long_win_rate`, `long_profit_factor`, and the `short_*` equivalents) in addition to the existing combined fields; `write_summary`'s Markdown table gains matching columns. | Accepted | Spec's Backtest Rules: "Report long and short sides separately as well as combined." Only combined sums plus bare `long_count`/`short_count` existed — an audit gap. | Stage 0 plan Task 2 (`domain/run.rs` `BacktestMetrics`), Task 8 (`backtest.rs` `side_summary` helper plus a new `per_side_metrics_sum_to_combined_metrics` test), Task 9 (`report.rs` `write_summary` columns and its test literals). | Stage 0 plan `docs/superpowers/plans/2026-07-09-stage-0-fixture-pipeline.md` Tasks 2, 8, 9 |
| 2026-07-10 | `NewsSignalObservation` gains an `extreme_sentiment: f64` field: the eligible article's sentiment score with the largest absolute value (sign preserved). | Accepted | Spec's Main Research Dataset section requires "mean, weighted, extreme, and dispersion sentiment features" per row; only mean/weighted/dispersion existed — an audit gap. | Stage 0 plan Task 2 (`domain/observation.rs`) and Task 6 (`observations.rs` `build_one` computes it from the eligible-article list already gathered for `mean_sentiment`/`sentiment_dispersion`). | Stage 0 plan `docs/superpowers/plans/2026-07-09-stage-0-fixture-pipeline.md` Tasks 2, 6 |

## Open Questions

| Question | Blocks decision? | How to resolve | Owner |
|---|---|---|---|
| Is a 1-year development / 1-year holdout split sufficient for an initial go/no-go, and are the stop/revise/continue gates strong enough to act on? | Blocks confidence in Stage 3's result, not Stage 0 implementation. | Revisit once Stage 0/2 mechanics are proven; may not need resolution until Stage 3 is planned. | Open |

## Supporting Artifacts

- Spec (frozen, approved): `docs/superpowers/specs/2026-07-08-correlation-first-pipeline-design.md`
- Stage 0 plan (in progress, being corrected): `docs/superpowers/plans/2026-07-09-stage-0-fixture-pipeline.md`
- Prior design review artifact (disposable HTML): `.lavish/correlation-first-pipeline-review.html`
