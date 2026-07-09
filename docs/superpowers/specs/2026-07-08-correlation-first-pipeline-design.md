# Correlation-First Market Sentiment Pipeline Design

Date: 2026-07-08

Status: Approved for implementation planning

## Objective

Build a local, rerunnable research pipeline that determines whether historical
news sentiment contains useful out-of-sample information about subsequent U.S.
equity and ETF returns.

The first version optimizes for speed of proving or falsifying signal. It is not
a live-trading platform. A useful result must go beyond correlation and show
that a moderately realistic long/short/flat rule remains interesting after
costs, timing constraints, baselines, and an untouched holdout period.

## Fixed V1 Decisions

- Vendor budget: zero dollars.
- Research history: the latest complete two-year interval shared by all
  selected sources at first ingestion.
- Development split: the first chronological year.
- Holdout split: the second chronological year, left untouched until the
  primary experiment configuration is frozen.
- Universe: `SPY`, `QQQ`, `DIA`, `IWM`, then roughly 25 liquid U.S. large-cap
  equities after the source probe.
- Price interval: one-hour bars primary, 15-minute bars secondary when reliable,
  and daily bars as a sanity-check baseline.
- Text input: article headline plus summary or description.
- Sentiment: a deterministic, locally computed, versioned scoring method.
- Strategy: symmetric long/short/flat rules.
- Storage: immutable JSONL raw payloads, Parquet research datasets, and
  CSV/Markdown/static-chart reports.

The two-year result is an initial signal-validation experiment. It cannot
establish robustness across every market regime.

## Free Data Sources

### Finance And Ticker News

Use the free Massive Stocks Basic news endpoint:

- endpoint: `GET /v2/reference/news`
- two years of history
- title, description, publisher, URL, ticker tags, and `published_utc`
- hourly updates

Documentation: https://massive.com/docs/rest/stocks/news

### Broad News

Use GDELT for macro, policy, rates, housing, geopolitical, sector, and theme
coverage.

- GDELT data is free and open.
- Prefer direct downloads for v1.
- Do not require BigQuery, which may introduce separate cloud costs.

Documentation: https://www.gdeltproject.org/data.html

### Market Prices

Use the free Alpaca Basic historical market-data API.

- U.S. stock and ETF history back to 2016
- 200 historical requests per minute
- enough history and throughput for this offline experiment

Documentation:
https://docs.alpaca.markets/us/v1.1/docs/about-market-data-api

### Excluded Sources

X and other paid social feeds are excluded from v1. Their access, historical
depth, licensing, and pricing have not been verified for this experiment.

## Core Terminology

- `news_window`: historical articles included in a signal, such as trailing one
  hour, trailing four hours, or session-to-now.
- `signal_time`: time at which an aggregated signal is available for analysis or
  trading.
- `measurement_horizon`: future market period measured after `signal_time`.
- `price_interval`: OHLCV bar duration.
- `source_set`: configured source mixture used to create an observation.
- `dataset_id`: immutable identifier for a versioned input snapshot.
- `observation_set_id`: immutable identifier for observations derived from one
  dataset and one aggregation configuration.
- `run_id`: identifier for one analysis or backtest configuration and result.

## Architecture

The CLI is divided into independently testable stages:

1. `fixture` creates deterministic synthetic inputs.
2. `ingest` fetches or loads news and prices through source adapters.
3. `normalize` converts vendor payloads to canonical records.
4. `score` computes sentiment and market-relevance labels.
5. `build-observations` aligns news signals with market context and future
   measurements.
6. `analyze` runs non-trading correlation, bucket, lag, source, and tail tests.
7. `backtest` applies long/short/flat rules with execution costs.
8. `report` generates decision-oriented tables, charts, and a summary.
9. `run` may orchestrate required stages while reusing existing snapshots.

Vendor-specific code stays behind news and price source adapters. Fixture,
curated-file, and API modes must produce the same normalized schemas.

## Data Flow

1. Ingest source payloads once and retain them unchanged.
2. Normalize timestamps, publishers, symbols, themes, and price fields.
3. Deduplicate syndicated and republished articles.
4. Score headline and summary sentiment locally.
5. Classify each article as ticker-specific, sector/theme, or macro-market.
6. Map direct and indirect relevance to tradable symbols using versioned rules.
7. Aggregate eligible articles for each configured `news_window` at each
   `signal_time`.
8. Join only price information available at `signal_time`.
9. Measure returns, volatility, drawdown, runup, and tail events strictly after
   `signal_time`.
10. Store the generated observations as a reusable observation set.
11. Analyze or backtest that observation set without refetching source data.

Changing thresholds or costs reruns only the backtest. Changing windows,
relevance rules, source sets, or sentiment scoring regenerates observations
from the stored snapshot. Adding history creates a new immutable dataset
snapshot.

## Stored Data

Each source snapshot lives under `data/datasets/<dataset_id>/`:

- `raw/`: immutable vendor JSONL payloads
- `normalized_articles.parquet`
- `price_bars.parquet`
- `source_catalog.parquet`
- `manifest.json`

The manifest records sources, exact date boundaries, schema versions, row
counts, checksums, coverage, and creation time.

Each derived observation set lives under
`data/observation_sets/<observation_set_id>/`:

- `news_signal_observations.parquet`
- `manifest.json`

The observation-set manifest records its source `dataset_id`, aggregation
configuration, sentiment version, relevance-rule version, schema, row count,
checksums, and creation time.

Each run lives under `runs/<run_id>/`:

- `config.json`
- `dataset_manifest.json`
- `observation_set_manifest.json`
- `reports/summary.md`
- `reports/coverage.csv`
- `reports/bucket_returns.csv`
- `reports/backtest_metrics.csv`
- `reports/trade_log.csv`
- `charts/`

## Main Research Dataset

`news_signal_observations` contains one row per:

`symbol x signal_time x news_window x measurement_horizon x price_interval x source_set`

Each row contains:

- article counts by ticker, sector/theme, and macro scope
- source and publisher counts
- mean, weighted, extreme, and dispersion sentiment features
- prior return, prior volatility, session, and after-hours context
- future return, volatility, tail event, maximum drawdown, and maximum runup
- contributing article and price-bar identifiers
- dataset and run lineage

## Sentiment And Relevance

The first scorer is deterministic and versioned. It operates on headline plus
summary, emits a bounded score, and records enough intermediate values to
explain a result. Vendor sentiment may be stored as a benchmark but is not
ground truth.

Ticker tags are one relevance input, not the only one. Configured mappings
propagate:

- direct company news to linked equities
- sector and theme news to relevant equities and ETFs
- macro news to broad-market or economically exposed symbols

Source sets are evaluated separately:

- `finance_only`
- `broad_news`
- `finance_plus_broad`

## First Experiment Matrix

- Price interval: one hour primary; 15 minutes secondary if the source probe
  confirms reliable coverage.
- News windows: trailing one hour, trailing four hours, and session-to-now.
- Measurement horizons: next hour, next four hours, same-day close, and next
  regular session.
- Strategy thresholds: top and bottom 10 percent and 20 percent of development
  sentiment distributions.
- Trading costs: 0, 5, and 10 basis points, plus a doubled-cost stress case for
  the selected configuration.
- Baselines: always-flat, random signal, shuffled sentiment, and prior-return
  momentum.
- Targets: future return, direction, volatility expansion, and tail-event
  occurrence.

The primary matrix is frozen before holdout evaluation. Added configurations
are labeled exploratory and cannot count as proof unless tested on new
untouched data.

## Validation

The latest common two-year source interval is frozen at first ingestion. The
first year is used for exploration and chronological walk-forward validation.
The second year is the final untouched holdout.

Parameter selection uses only the development year. Quantile thresholds are
learned from development data and then frozen. The holdout is evaluated once
for the primary result; repeated holdout-driven tuning is prohibited.

Source-set comparisons use identical symbols, dates, windows, and market data.
Uncertainty estimates use time-aware resampling or aggregation so correlated
intraday observations are not treated as fully independent.

## Backtest Rules

- Enter at the next configured tradable bar after `signal_time`.
- Never trade on an article before its publication timestamp.
- Defer after-hours signals to the next regular-session tradable bar.
- Long only the selected upper sentiment quantile.
- Short only the selected lower sentiment quantile.
- Remain flat in the middle.
- Report gross and net returns.
- Apply spread/slippage through the configured basis-point cost.
- Report long and short sides separately as well as combined.

## Data Quality And Error Handling

- Normalize all timestamps to UTC while retaining original timestamps.
- Quarantine articles with absent or ambiguous publication times.
- Deduplicate syndicated and republished articles before aggregation.
- Respect weekends, holidays, early closes, after-hours sessions, and daylight
  saving transitions.
- Do not silently forward-fill missing news or price bars.
- Make ingestion idempotent and resumable.
- Record rate-limit responses, retries, source outages, dropped rows, and
  coverage gaps.
- Mark a run invalid when material coverage gaps violate its configured minimum.
- Keep failed experiments and their diagnostics as first-class run artifacts.

## Required Tests

- News-window boundary tests.
- Publication-time and no-lookahead tests.
- Next-bar and after-hours execution tests.
- Weekend, holiday, early-close, and daylight-saving tests.
- Article deduplication tests.
- Price return and backtest accounting tests, including short positions.
- Determinism tests for identical snapshot, configuration, and seed.
- A manually traceable fixture observation from raw article through trade.
- Synthetic positive, negative, neutral, and shuffled-signal controls.
- Integration tests covering fixture through final report generation.

## Decision Demo

The v1 demonstration must:

1. Ingest or generate a dataset snapshot once.
2. Run a baseline analysis and backtest.
3. Show the summary, coverage, bucket returns, equity curve, drawdown, metrics,
   and trade log.
4. Change one window, threshold, or cost assumption.
5. Rerun without downloading source data.
6. Compare both runs.
7. End with an explicit `stop`, `revise`, `expand data`, `expand sources`, or
   `continue` recommendation.

## Success And Failure Gates

V1 is promising only when:

- the held-out top-minus-bottom sentiment return spread is positive
- the held-out long/short strategy is profitable after moderate costs
- the selected result remains profitable under doubled costs
- development walk-forward periods show consistent direction
- results are not dominated by one symbol, source, week, or event
- tail-event prediction improves over non-sentiment baselines
- source coverage and timestamp validity pass configured checks

V1 should stop or revise when:

- coverage is too sparse or publication times are unreliable
- sentiment performs no better than shuffled or non-sentiment baselines
- the apparent edge disappears under next-bar execution or simple costs
- results depend on one fragile configuration or isolated event

## Delivery Stages

1. Stage 0: deterministic fixture pipeline and synthetic signal demo.
2. Stage 1: hand-curated real sample for timestamp and leakage inspection.
3. Stage 2: narrow Massive, GDELT, and Alpaca source probe.
4. Stage 3: free two-year hypothesis test.
5. Stage 4: expanded paid history or modeling only if Stage 3 justifies it.

Every stage produces the same dataset lineage, reports, controls, and explicit
next decision.

## V1 Non-Goals

- live trading or brokerage execution
- interactive dashboards
- tick-level market data
- futures, options, or exact benchmark-index feeds
- portfolio optimization
- supervised or deep-learning models
- mandatory social-media ingestion
- production-grade proof across many market regimes
