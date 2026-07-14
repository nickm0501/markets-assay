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
  is **implemented** as of 2026-07-13 (commits `e4fe75d`..`ba60769`, Tasks 1-10).
  `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, and all 45
  tests (34 lib + 11 CLI) pass; `cargo run -- run --config configs/stage0_fixture.json`
  produces 6 per-configuration verdicts (5 `continue`, 1 `revise`) with the full
  `runs/<run_id>/` artifact set. One implementation deviation from the plan's
  illustrative code was required: the `serde_arrow` 0.14.2 API (schema tracing
  via `from_samples`, `FieldRef` from `arrow::datatypes`, string-encoded unit
  enums) differs from the plan's snippets — the plan's Implementation Notes
  anticipated this adapter. Captured in commit `a29881e`.
- `run_id` is defined in the spec as the identifier for **one** analysis or
  backtest configuration and result. The Stage 0 plan's Architecture section
  states later stages reuse the analysis/backtest/report loop unchanged and
  only swap the data source — so whatever grain Stage 0 bakes into that loop
  becomes the grain for every later stage.
- **Stage 1's real-data findings (2026-07-14)** — the answers only the payloads
  could give, now recorded in `fixtures/saved_sample/README.md`:
  - **S1-A: Alpaca hourly bars are clock-aligned (`:00`) and include pre/post
    market.** The feared mass-drop from grid misalignment does *not* happen —
    `signal_time` is derived from the bar, so the signal grid *is* the bar grid.
    But two real bugs did: non-regular bars could open trades (including a
    `13:00Z` bar whose `open` is a 09:00 ET pre-market price that never traded
    in-session), and — worse — with after-hours bars now available, the
    contiguity check would have **silently bridged the session close**,
    reversing Decision 3. Both fixed; entry *and* measurement bars must now be
    regular-session.
  - **S1-B: no syndication.** 0 of 132 distinct titles appear under more than one
    URL. The planned near-duplicate work is **not needed** and was not built.
    (Dedupe still does real work: 85 exact duplicates, a fetch artifact of
    querying per-ticker.)
  - **S1-C: the sentiment lexicon reads 1 headline in 5** (`lexicon_hit_rate =
    0.2021`). This is the Stage 2 blocker. See Next.
  - **79% of articles (301/381) are published outside the regular session**,
    median deferral ~14 hours. Financial news is written overnight: the
    after-hours deferral rule is the *common path*, governing the majority of
    every signal, not an edge case. The fixture had 1 deferred article of 5 and
    badly under-represented it.
  - **Zero quarantined articles.** Every real timestamp parsed. The quarantine
    path is correct but was not exercised by this sample.
  - **Result: all 6 configurations returned `revise`.** No signal — as expected
    from 5 days and a scorer that cannot read most of the news. **These numbers
    are mechanically valid and statistically meaningless.**
- Stage 1 Tasks 1-4 and 7-9 are **implemented** as of 2026-07-14. `cargo fmt`,
  `cargo clippy --all-targets -- -D warnings`, and all 93 tests (78 lib + 15
  CLI) pass; the fixture path still produces 6 per-configuration verdicts
  (5 `continue`, 1 `revise`), unchanged from Stage 0 — the plumbing did not
  move the science. Two things were found *during* implementation that the
  plan had not anticipated, both now fixed:
  (a) **dedupe silently discarded the losing duplicate** — it kept whichever
  copy a `BTreeMap` happened to hold first, and the other simply evaporated
  uncounted. It now keeps the **earliest** `published_at` (the only copy that
  could actually have been traded; a later republication would backdate the
  signal) and records the loser as an excluded row with a `duplicate_of`
  pointer.
  (b) **`run_all` printed `decisions_revise = total - continue`**, which folded
  `stop`/`expand data`/`expand sources` into `revise` the moment Decision 14's
  full vocabulary existed — three distinct diagnoses collapsing into one wrong
  word. It now prints one line per verdict value that actually occurred.
  Tasks 5, 6, and 10 are blocked on the real payloads (`scripts/fetch_sample.sh`).
- Stage 1 is planned as of 2026-07-14
  (`docs/superpowers/plans/2026-07-14-stage-1-real-sample.md`), driven by a
  code audit of what Stage 0 actually left behind. Three structural gaps
  between the implemented Stage 0 code and the spec were found, and Stage 1
  exists to close them: (a) the spec's news/price **source adapters do not
  exist** — `pipeline.rs` calls `generate_fixture()` directly, so the data
  source is hardwired and there is no seam for the saved-file or live-API
  sources; (b) `RawArticle.published_at` is a
  non-optional `DateTime<Utc>`, so the spec's required *"quarantine articles
  with absent or ambiguous publication times"* is **unrepresentable in the
  type**; (c) the dataset manifest's `date_start`/`date_end` are **hardcoded
  fixture literals** (`pipeline.rs:126-127`) and would be silently wrong on
  real data. See Decisions 12-17.
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
- Three Stage 1 questions that **only the real payloads can answer**, not
  documentation or reasoning (see the Stage 1 plan's Open Questions): whether
  real Alpaca hourly bars are session-aligned (09:30) or clock-aligned (09:00);
  whether real syndication appears at all in a 5-day 4-ETF sample; and whether
  the 14-word Stage 0 sentiment lexicon is salvageable on real headlines.
  Stage 1's plan treats the first two as explicit *investigations* whose
  outcome may be "no change needed."

### Current Phase

- done: **Stage 1 is complete** (2026-07-14). All 10 tasks. The real sample is
  fetched, committed, and running; 95 tests green; the Decision Demo and the
  spec's hand-trace Required Test both pass on real data. Findings in
  `fixtures/saved_sample/README.md`.
- done: **Stage 2 Design** — the scorer bake-off ran (Decisions 20-21), and
  Stage 2 is planned:
  `docs/superpowers/plans/2026-07-14-stage-2-real-experiment.md`.
- active: **Development Handoff (Stage 2)** — awaiting confirmation the plan is
  settled enough to implement.
- done: Development Handoff (Stage 1) — plan reviewed via Lavish, four
  annotations applied (one of which, the quarantine/exclusion split, found a
  real defect in the design before any code was written — see Decision 15).
- done: Development Handoff (Stage 0) — Stage 0 plan handed off and
  implemented (see Known).
- already satisfied: Describe, Diagnose, Delimit, Direction (staged delivery
  0-4 chosen and recorded in the spec; Stage 1's own intake direction chosen
  in Decision 12), Design (Stage 0 and Stage 1 plans audited).
- relevant later: Direction/Design for Stages 2-4 (not yet planned in detail);
  the 1-year holdout/decision-gate-strength question before Stage 3; the
  sentiment-scorer question before Stage 2 (Stage 1 measures it, does not fix
  it).
- not applicable: none identified yet.

### Next

**Stage 2 is planned** (`docs/superpowers/plans/2026-07-14-stage-2-real-experiment.md`).
It closes the three things standing between us and a real two-year test:

1. **We cannot read the news.** Fixed by Decision 20 (LM+VADER), now to be
   shipped. Everything downstream is meaningless until this lands.
2. **We cannot tell signal from noise.** Only 1 of the spec's 4 baselines
   exists. Without the rest, `continue` means nothing — we cannot distinguish an
   edge from a coin that landed well. This is Decision 6's still-open half.
3. **We cannot fetch two years.** Ingestion is neither idempotent nor resumable,
   and Massive's free tier is ~5 req/min: a two-year fetch is a multi-hour job
   that *will* be interrupted. The spec has required this since day one and it
   has never been built.

**Two things must NOT be forgotten before Stage 3 runs** (both now tracked as
open questions):

- **S2-B: the First Experiment Matrix is a quarter-built.** 1 of 4 measurement
  horizons, 1 of 2 threshold pairs, 1 of 4 targets, no 15-minute bars, no
  doubled-cost stress case. **Running the two-year experiment on a quarter of
  the intended matrix would answer a question nobody asked.**
- **The 1-year-holdout sufficiency question**, open since 2026-07-08. Stage 2's
  baselines make the gates materially stronger; revisit once they exist.

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

- Fixture-first, then saved-sample, then narrow free-source probe, then a
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
Stage 1 (hand-picked real sample) → Stage 2 (narrow Massive/GDELT/Alpaca
probe) → Stage 3 (free two-year hypothesis test) → Stage 4 (paid
history/modeling only if Stage 3 justifies it). Stage 0 is planned and
implemented; Stage 1 is planned in full (2026-07-14); Stages 2-4 are named but
not yet planned in detail.

Stage 1's own direction (Decision 12) is the spec's **saved-file mode**: a
human hand-saves real vendor payloads, they are checked into the repo, and the
code reads them offline behind a source-adapter trait. Stage 2 later adds the
**API mode** behind the same trait without touching the research loop. This is
the seam the spec always required and Stage 0 never built.

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
| 2026-07-14 | Stage 1 gets its real data from a **saved-file reader** that loads real vendor payloads committed to `fixtures/saved_sample/`. Stage 1 contains **no network code** — no HTTP client, no API keys, no rate-limit handling. Stage 1 also makes the data source **swappable** for the first time, introducing the `NewsSource`/`PriceSource` trait seam the spec has always required, and moving the existing fixture generator behind it unchanged. (The spec calls this the "curated-file mode"; "curated" only means a human hand-picked the sample. Our code and prose say **saved-file** — clearer, and it survived a review round where "curated-file adapter" did not.) | Accepted | The spec names three distinct modes — *"Fixture, curated-file, and API modes must produce the same normalized schemas"* — and puts the API source probe at Stage 2, so Stage 1 is the saved-file mode by the spec's own structure. Alternatives considered: (a) building the live API adapters now, rejected because it collapses Stage 2 into Stage 1 and forces API keys, rate limits, and retries *before* a single real timestamp has been inspected by hand — inspection is Stage 1's entire purpose; (b) hand-transcribing articles instead of saving real payloads, rejected because it sacrifices exactly the real-world quirks (encodings, missing fields, syndication) Stage 1 exists to find. Keeping Stage 1 offline also makes it permanently reproducible — the sample cannot rot when a vendor changes an endpoint. **How the payloads are actually fetched (raised in review 2026-07-14):** by a throwaway `curl` script, `scripts/fetch_sample.sh`, run once outside the binary. The script is not application code — **no HTTP dependency enters `Cargo.toml` and the pipeline still reads only from disk** — but it makes the sample reproducible and refreshable rather than a pile of mystery files. Fetching becomes a real, tested, rate-limit-aware part of the application in Stage 2. | Creates `src/source/{mod,fixture,saved_files}.rs`, `src/source/vendor/{massive,gdelt,alpaca}.rs`, and `scripts/fetch_sample.sh`; deletes `src/fixture.rs`; adds the spec's missing `ingest` CLI subcommand; renames `Stage0Config` → `PipelineConfig`. `pipeline.rs::create_dataset_snapshot` stops calling `generate_fixture()` directly. API keys come from the environment and are never committed. | Stage 1 plan `docs/superpowers/plans/2026-07-14-stage-1-real-sample.md` Tasks 1, 3 |
| 2026-07-14 | The Stage 1 saved sample is **`SPY`/`QQQ`/`DIA`/`IWM` over one real trading week** that contains a weekend boundary and at least one real holiday or early close. ~140 hourly bars, ~50-150 real articles. | Accepted | Small enough that a human can eyeball every article's `published_at` — which is what "hand-picked ... for timestamp and leakage inspection" *means* — but wide enough that observations, analyze, and backtest all produce non-empty output and `calendar.rs` (Decision 2's DST/early-close code) meets real dates for the first time. Alternatives: 1-2 symbols × 2 days was rejected because at 1h bars it yields ~14 bars/symbol, so most measurement horizons get dropped for lack of coverage (Decision 3's drop rule) and the backtest may produce zero trades — it would prove the adapter and nothing else. The spec's full ~25-equity universe over a month was rejected because it is not hand-curatable: you cannot eyeball thousands of articles, so the fetch must be scripted, which *is* Stage 2. | **Stage 1's backtest numbers are mechanically valid and statistically meaningless, and must never be quoted as signal evidence.** Their only job is to prove real data flows through the loop. Recorded as an Implementation Note in the Stage 1 plan. | Stage 1 plan Scope; `configs/stage1_saved_sample.json` |
| 2026-07-14 | Stage 1 implements the spec's **full 5-value decision vocabulary** (`stop`/`revise`/`expand data`/`expand sources`/`continue`), driven by real data-quality evidence: quarantine rate, articles-per-signal, source-set coverage, lexicon hit rate, dropped-horizon rate. Data-quality gates are evaluated **before** signal gates. The **4 baselines** (always-flat, random, prior-return momentum) stay deferred and now gate Stage 2/3 alone. | Accepted | This closes half of Decision 6's gate, and does so on Decision 6's own logic. Decision 6 deferred these three values because "the fixture has no real data-quality signal to react to" — Stage 1 is the first stage that has one, and `expand data`/`expand sources` are *unverifiable* without it. Building them here means Stage 3's go/no-go gate arrives already exercised against real data rather than written blind against the two-year dataset, where a mis-specified gate is expensive to discover and there is no small sample left to debug it on. The baselines stay deferred by the same logic inverted: baselines measure *signal quality*, and a 4-symbol/5-day sample has no signal to measure — verifying them here would be unverifiable ceremony. Ordering data-quality gates first is load-bearing: a signal result computed on unusable data is not a signal result, and without the ordering Stage 3 could report a false positive. | Stage 1 plan Task 8. All verdict thresholds live in `PipelineConfig.verdict_thresholds` — no magic numbers in the verdict function, because Stage 3 will need to tune and audit them. `write_summary` must report *which* metric tripped a non-`continue` verdict; an unauditable verdict is ceremony. Decision 6's baseline half remains open. | Stage 1 plan `docs/superpowers/plans/2026-07-14-stage-1-real-sample.md` Task 8; supersedes half of Decision 6 |
| 2026-07-14 | `RawArticle.published_at` becomes `Option<DateTime<Utc>>` and `normalize_articles` returns `(Vec<NormalizedArticle>, Vec<SetAsideArticle>)`. `NormalizedArticle.published_at` stays **non-optional** — surviving quarantine is what makes an article normalized. Set-aside rows retain the vendor's **original unparsed timestamp string**. Critically, a set-aside article carries one of two *distinct* dispositions: **quarantined** (a data-quality failure: missing/unparseable/ambiguous timestamp, missing title+summary) or **excluded** (a scope fact: outside the dataset window, no relevant symbol, duplicate). `quarantine_rate` counts **only** the former. | Accepted | Two findings, one type change. (a) The spec requires "Quarantine articles with absent or ambiguous publication times," but the current type (`domain/article.rs:32`) is a non-optional `DateTime<Utc>` — **a schema that cannot represent a missing timestamp cannot quarantine one.** The fixture never surfaced this because it always generates a clean timestamp. (b) **Raised in review 2026-07-14:** an earlier draft of this decision lumped out-of-window articles under `QuarantineReason`. That is wrong, and dangerously so — quarantine means *"this row cannot be trusted"*, while an out-of-window article is perfectly trustworthy and merely out of sample. Because `quarantine_rate` drives the `stop` verdict ("timestamps unreliable", Decision 14), conflating them would let a **sample boundary** masquerade as a **data-quality failure** and halt Stage 3 for no reason. Keeping `NormalizedArticle.published_at` non-optional puts the quarantine invariant in the type system rather than in a convention. | Stage 1 plan Task 2. `article_id` currently hashes `(source, url, published_at)` (`normalize.rs:64-71`) and needs a `(source, url)` variant for rows with no timestamp. New `set_aside_articles.parquet` in the dataset snapshot, with quarantined and excluded counts recorded **separately** in the manifest and reported separately in `summary.md`. Two invariants asserted by test: every raw article lands in exactly one of normalized/quarantined/excluded (nothing silently dropped), and a high *exclusion* rate must **not** yield `stop`. | Stage 1 plan `docs/superpowers/plans/2026-07-14-stage-1-real-sample.md` Tasks 2, 4, 8, 9; spec "Data Quality And Error Handling" |
| 2026-07-14 | The dataset manifest's `date_start`/`date_end` must be **derived from the data** (min/max UTC date across price bars and normalized articles), not hardcoded. Articles falling outside the config's expected window are quarantined (`PublishedAtOutsideDatasetWindow`) rather than silently widening the manifest. | Accepted | `pipeline.rs:126-127` writes `date_start: 2026-06-29, date_end: 2026-07-07` as **literals**. On real data that manifest would be silently wrong — and the manifest is the lineage artifact every later stage, and every reproducibility claim, depends on. The spec requires the manifest record "exact date boundaries." Quarantining out-of-window articles rather than widening the range gives that requirement teeth: the configured window is an assertion, not a suggestion. | Stage 1 plan Task 4. Adds `dataset_date_start`/`dataset_date_end` to `PipelineConfig`. A pinning test requires the derived range to reproduce Stage 0's old hardcoded literals for the fixture config, so the fix is provably a no-op on the fixture path. | Stage 1 plan `docs/superpowers/plans/2026-07-14-stage-1-real-sample.md` Task 4 |
| 2026-07-14 | A **degenerate** sentiment distribution (collapsed/inverted quantile thresholds, or a modal-value share above `max_modal_share`, default 0.8) produces **zero trades** and an `expand sources` verdict — never an all-long book. | Accepted | **This is a live defect in the Stage 0 code, found by auditing it against real-data expectations.** `sentiment.rs:12-29` is a 14-word lexicon built for fixture headlines; real headlines will miss it, so most articles score exactly `0.0`. `backtest.rs:59-66` derives both thresholds from that distribution, so `long_threshold == short_threshold == 0.0`; and because `backtest.rs:78` tests `mean_sentiment >= long_threshold`, **every neutral observation is classified `long`, the short branch never fires, and the run emits an all-long book with plausible-looking metrics.** It fails silently, not loudly. The same tie-collapse makes `analysis.rs::top_minus_bottom` bucket on ties, so `recommendation` flips on noise. A pipeline that cannot distinguish "no signal" from "no signal *resolution*" will mislead Stage 3. | Stage 1 plan Task 7. `BacktestMetrics` carries an explicit `degenerate: true` flag rather than a silent zero. Required test constructs the exact degenerate case (10 observations, 9 at sentiment 0.0) and **must be shown to fail against the current `backtest.rs` before the fix** — that is the proof the bug was real. A counter-test guards the check from being over-eager and suppressing a healthy Stage 3 distribution. Stage 1 *measures* the lexicon hit rate; it does **not** fix the scorer — that is Stage 2's first design question (Open Question S1-C). | Stage 1 plan `docs/superpowers/plans/2026-07-14-stage-1-real-sample.md` Task 7; `src/backtest.rs:59-84`; `src/sentiment.rs:12-29` |

| 2026-07-14 | **The sentiment scorer's failure is four distinct defects, not "the lexicon is too small" — and only two of them are the scorer's fault.** Measured on the Stage 1 real sample: (1) **directional bias**, 101 positive vs 7 negative on Massive (14:1) — the lexicon's positive words (`strong`, `growth`, `positive`) are ordinary financial prose while its negative words (`hawkish`, `weighs`, `shock`) are rare, so the long book fills on *vocabulary*, not news; (2) **no resolution** — 7 discrete values 0.25 apart, and the strategy is *quantile-based*, so a scorer with 7 buckets and 50-94% ties **cannot rank**, which is what `is_degenerate` exists to catch; (3) **blindness** — 49% of Massive articles score exactly 0.0; (4) **GDELT is 94% blind, and this is a TEXT problem, not a scorer problem** — GDELT gives title only, averaging 11 words, across 65% of the corpus (249/381). **No scorer fixes (4).** | Accepted | Naming the failure as "small lexicon" would have led straight to "adopt a bigger lexicon", which fixes (1)-(3) and leaves (4) — the largest slice of the corpus — untouched and unnoticed. The diagnosis matters more than the remedy: the strategy never uses the *absolute* score, only the top/bottom quantiles of the distribution, so **ordering power is the only property that counts**. A scorer can be "accurate" and still useless. | (4) is split out as its own open question (GDELT text starvation) and must not be treated as answered by any scorer work. The bake-off experiment (`2026-07-14-sentiment-scorer-bakeoff.md`) evaluates candidates on *ranking power first*, not accuracy. | `fixtures/saved_sample/README.md` (S1-C); `docs/superpowers/design/correlation-first-pipeline/2026-07-14-sentiment-scorer-bakeoff.md` |
| 2026-07-14 | **The local scorer stays primary; Massive's vendor sentiment is stored as a benchmark and never traded on.** Holds the spec's line ("a deterministic, locally computed, versioned scoring method"; "vendor sentiment may be stored as a benchmark but is not ground truth") — now that we know the vendor actually ships **per-ticker sentiment with reasoning at 100% coverage** of its articles (410 labels across 132 articles). | Accepted | Trading on vendor sentiment would change the research question from *"does news sentiment predict returns"* to *"does Massive's black box predict returns"* — a question we could not reproduce, version, explain, or debug, and whose answer the vendor could silently invalidate by changing their model mid-experiment. It also covers **zero** GDELT articles (65% of the corpus). Conversely, keeping it as a benchmark is pure upside: it hands us a free yardstick for a question we currently **cannot answer at all** — "is our local scorer any good?" The spec anticipated precisely this and we should take the gift on the spec's terms. | Vendor sentiment is parsed from Massive's `insights[]` (per-ticker, not per-article) and stored alongside the local score. It is the evaluation target for the bake-off. It must never reach `backtest.rs`. Note it is evidently LLM-generated (the `sentiment_reasoning` prose gives it away), which is exactly why it is a yardstick and not a signal. | Spec "Sentiment And Relevance"; `2026-07-14-sentiment-scorer-bakeoff.md` |

| 2026-07-14 | **Adopt Loughran-McDonald + VADER, continuous and equally weighted, as the sentiment scorer** (bake-off candidate D). Both lexicons are checked into `data/lexicons/`, so the scorer stays deterministic, local, versioned, and offline-reproducible per the spec. `SENTIMENT_VERSION` bumps when it ships. | Accepted | Measured on all 381 real Stage 1 articles against 410 vendor labels — chosen on evidence, not reputation. D beat the 14-word control on every criterion set *before* the data was seen: agreement with the vendor benchmark ρ 0.339 → **0.519**; blindness 54% → **2%** (Massive) and 94% → **27%** (GDELT); resolution 7 → **128** distinct values; directional bias 14:1 → **4:1**, which is *less* skewed than the vendor benchmark's own 6.5:1. LM and VADER fail in *different* places — LM (built from 10-K filings) reads Massive's fuller descriptions well but struggles on 11-word titles; VADER, built for short punchy text, is far better on headlines — which is exactly why combining them beats either alone. | Beats the vendor-hybrid (candidate E) on grounds unrelated to score: E covers only **35%** of the corpus (no GDELT at all), depends on Massive's **time-varying LLM** across the dev/holdout boundary, and **cannot be evaluated without circularity** (E contains the vendor label; measuring its agreement with that label is self-referential — its ρ of 0.974 was discarded as meaningless). **Methodological line recorded once so it is never crossed: the scorer must NEVER be selected by which one predicts returns best.** That fits the scorer to the outcome and is tuning-on-the-holdout wearing a different hat. Every criterion used is deliberately independent of returns. | `2026-07-14-sentiment-scorer-bakeoff.md` |
| 2026-07-14 | **Vendor sentiment cannot be the primary signal — for a mechanical reason, not a dogmatic one.** Massive's sentiment is **free** (100% coverage of its articles on the free tier) and semantically good, but it is **categorical**: `positive`/`neutral`/`negative`. Three values cannot be quantiled. Feeding it the spec's rule yields `short_threshold = 0`, `long_threshold = +1`, and therefore **46% long, 54% short, 0% flat — every neutral article gets shorted.** | Accepted | The spec's rule is "long the upper quantile, short the lower quantile, **remain flat in the middle**". Vendor sentiment **has no middle**. This is the same degeneracy failure as the 14-word lexicon (Decision 17), reached from the opposite direction: not too few *words*, but too few *values*. It also has zero GDELT coverage (65% of the corpus) and is LLM-generated, so a two-year snapshot may contain articles scored by *different models* over time — a scorer that varies with time manufactures spurious signal across exactly the dev/holdout boundary the experiment rests on. | Vendor sentiment is still parsed and stored as a **benchmark** (previous decision), and it earned its keep immediately: it was the yardstick that let the bake-off rank candidates at all. It must never reach `backtest.rs`. | `2026-07-14-sentiment-scorer-bakeoff.md`; spec "Sentiment And Relevance", "Backtest Rules" |

## Open Questions

| Question | Blocks decision? | How to resolve | Owner |
|---|---|---|---|
| Is a 1-year development / 1-year holdout split sufficient for an initial go/no-go, and are the stop/revise/continue gates strong enough to act on? | Blocks confidence in Stage 3's result, not Stage 0 or Stage 1 implementation. | Revisit once Stage 0/2 mechanics are proven; may not need resolution until Stage 3 is planned. | Open |
| S1-A: Are real Alpaca hourly bars session-aligned (09:30 ET) or clock-aligned (09:00 ET)? | Blocks Stage 1 Task 5. | **Look at the payload.** Cannot be answered from documentation or reasoning. If bars are clock-aligned, Decision 3's "drop any horizon a session close or gap prevents from being fully covered" rule could silently discard most of the sample. | Open |
| S1-B: Does real syndication (same story, different URLs) actually appear in a 5-day / 4-ETF sample? | Blocks the *scope* of Stage 1 Task 6, not Stage 1 itself. | Count it in the saved sample. `normalize.rs:96-102` keys dedupe on `url + title`, which cannot catch a story republished under a different URL — but if the sample is too small to contain any, "no" is a valid answer that defers the work to Stage 2. | Open |
| ~~S1-C: Is the 14-word lexicon salvageable, or does the scorer need replacing?~~ | — | **Closed 2026-07-14 — ANSWERED: it is not salvageable.** Measured `lexicon_hit_rate = 0.2021`. The bake-off replaced it with Loughran-McDonald + VADER (Decision 20): blindness 49%→2% on Massive, 94%→27% on GDELT; resolution 7→128 values; bias 14:1→4:1. | Closed |
| ~~S1-D: Do vendor terms permit committing real payloads to the repo?~~ | — | **Closed 2026-07-14** at the user's direction: not a blocker for this project. The payloads get committed. | Closed |
| **S2-A: GDELT text starvation.** GDELT supplies **title only, averaging 11 words**, for **65% of the corpus** (249/381 articles). ~~No scorer replacement fixes this.~~ **Correction (2026-07-14): that claim was wrong.** The bake-off measured it: the new scorer (LM+VADER) takes GDELT from **94% blind → 27% blind**, and from 3 distinct values → 101. VADER is built for short punchy text, which is what a headline is. **Mitigated, not solved** — 27% blind on 11-word titles is still thin. | No longer blocks the `broad_news` / `finance_plus_broad` source sets, which was the fear. Now a quality concern rather than a structural one. | Revisit *after* the new scorer ships and we can see real `broad_news` results. Remaining options if it still hurts: find a GDELT mode returning more text or its own GKG tone score; or accept title-only macro scoring and record the weakness honestly. **Lesson: this question was nearly closed on a plausible conjecture that measurement refuted.** | Open (downgraded) |

| **S2-B: The First Experiment Matrix is far from complete.** We have 2 of ~3 news windows, **1 of 4** measurement horizons (missing next-4h, same-day close, next regular session), **1 of 2** threshold pairs (missing 10%), **1 of 4** targets (missing direction, volatility expansion, tail events), no 15-minute bars, and no doubled-cost stress case. | **Blocks Stage 3**, not Stage 2. | Needs its own plan. `session-to-now` and `same-day close` require session-relative logic that does not exist. **Running the two-year experiment on a quarter of the intended matrix would answer a question nobody asked** — do not let Stage 3 start without this. | Open |
## Supporting Artifacts

- Spec (frozen, approved): `docs/superpowers/specs/2026-07-08-correlation-first-pipeline-design.md`
- Stage 0 plan (implemented 2026-07-13): `docs/superpowers/plans/2026-07-09-stage-0-fixture-pipeline.md`
- Stage 1 plan (awaiting handoff confirmation): `docs/superpowers/plans/2026-07-14-stage-1-real-sample.md`
- Prior design review artifacts (disposable HTML): `.lavish/correlation-first-pipeline-review.html`, `.lavish/correlation-first-pipeline-design-review.html`
