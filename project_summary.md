# Project Summary

Date: 2026-07-08

## Main Goal

Build a program that measures whether news sentiment is correlated with market performance and whether that signal can be used for prediction.

The practical objective is:

- collect historical news data
- collect matching historical market data
- compute sentiment features from news
- align news events to market time series
- test correlation and predictive power

The initial research horizon is the latest complete two-year interval available
across all selected sources at first ingestion. The exact start and end dates
must be frozen in the dataset manifest. Version 1 must use free data sources.

## Core Problem

We need two input systems:

1. `Sentiment analysis`
2. `Market data ingestion`

Then we need a training/research pipeline that joins both datasets without leaking future information.

## Recommended Research Direction

Start with a narrower, more controllable first version:

- U.S. equities and ETF proxies instead of all global markets
- intraday OHLCV bars when available, with daily bars as a sanity-check fallback
- headline + summary sentiment instead of full article-body NLP

This reduces licensing cost, ingestion complexity, and modeling noise.

## Candidate Approaches

### 1. Correlation-first pipeline

Build a research pipeline that:

- ingests historical market-relevant news, including ticker-specific, sector/theme, and macro-market news
- scores each article for sentiment
- aggregates sentiment by symbol, news window, and market relevance scope
- joins sentiment windows to market returns and volatility
- measures correlation, lag, and stability

This is the best first step because it answers the most important question: is there any usable signal at all?

#### Recommended v1 shape for option 1

The recommended first implementation of the correlation-first pipeline is a `hybrid correlation-first pipeline` optimized for `speed of proving signal`.

Primary question:

- using free historical news and market data, does simple sentiment from
  `headline + summary` contain enough out-of-sample directional information to
  support a moderately realistic short-horizon trading rule?

Recommended scope:

- Universe: `SPY`, `QQQ`, `DIA`, `IWM` plus `20-50` liquid U.S. large-cap equities
- Time range: the latest common two-year interval available at first ingestion
- News input: historical market-relevant news with `title/headline`, `summary/description`, `published timestamp`, `source`, optional ticker tags, and optional theme/macro labels
- Sentiment input: custom in-house scoring on `headline + summary`
- Market input: hourly OHLCV bars preferred; shorter intervals such as `15-minute`, `5-minute`, or `1-minute` are useful if available cheaply and reliably; daily bars remain a fallback and sanity check
- Objective: falsify or support the hypothesis quickly, not build a durable platform first

Why this shape is preferred:

- ETFs provide broad market sanity checks
- large-cap equities provide a better test of ticker-level signal
- intraday bars better capture news reaction timing than daily bars
- daily bars are still useful as a low-noise baseline and data-availability fallback
- tick-level data is intentionally out of scope for v1 because it adds cost and execution complexity before the signal is proven

Supported news scopes:

- `ticker-specific`: articles directly about a company, ETF, or tradable symbol
- `sector/theme`: articles about industries or themes such as semiconductors, banks, AI, energy, or housing
- `macro-market`: articles about rates, inflation, jobs, Fed policy, oil, housing, geopolitics, and other broad market drivers

Source-diversity goal:

- source quality will heavily influence the result, so v1 should track source mix explicitly instead of treating all articles as equivalent
- start with a scoped but varied source set: one finance/ticker-oriented feed, one broad news source for macro/theme coverage, and optional social data if access is cheap enough
- store source metadata such as publisher, source type, source group, URL, and ingest batch
- support experiments that compare `finance_only`, `broad_news`, `finance_plus_broad`, and optionally `social_augmented` source sets
- do not make X.com or other social feeds mandatory for v1 because API access and historical coverage may create cost and licensing friction
- if a source requires paid access, exclude it from v1 and record it only as a
  future option

Core research table terminology:

- `news_window`: the historical article window used to compute sentiment, such as trailing 2 hours, morning session, full session, or after-hours
- `signal_time`: the time when the sentiment signal becomes available for measurement or trading
- `measurement_horizon`: the future market period measured after the signal, such as next hour, same-day close, next session, or next 3 sessions
- `price_interval`: the OHLCV bar size used for market measurement, such as daily, hourly, 15-minute, 5-minute, or 1-minute
- `source_set`: the configured source mix used to generate a research row, such as `finance_only` or `finance_plus_broad`

Main research dataset:

- use `news_signal_observations`
- one row should represent `symbol x signal_time x news_window x measurement_horizon x price_interval x source_set`
- each row should contain the aggregated news signal known at `signal_time` and the future market movement measured over `measurement_horizon`
- this table should be regenerated from stored raw and normalized data whenever windows, source sets, or sentiment rules change

Recommended stored datasets:

- `raw_articles`: original vendor payloads, source, URL, publish timestamp, title, summary, and raw metadata
- `normalized_articles`: cleaned article records with sentiment, dedupe status, linked symbols, theme tags, news scope, and source metadata
- `source_catalog`: publishers and feeds with source type, source group, access notes, and whether the feed is free, paid, or unknown
- `price_bars`: OHLCV bars by symbol and interval
- `news_signal_observations`: generated research rows that pair news-derived signals with future market observations
- `analysis_runs`: non-trading research runs such as bucket tests, correlation tests, lag tests, source-set comparisons, and tail-event tests
- `backtest_runs`: trading-rule runs with assumptions, metrics, and trade logs

Artifact contract:

- immutable vendor payloads should be stored as JSONL under
  `data/datasets/<dataset_id>/raw/`
- normalized articles and price bars should be stored as Parquet under
  `data/datasets/<dataset_id>/`
- every dataset snapshot should include a manifest with its `dataset_id`,
  sources, schemas, date coverage, row counts, and checksums
- generated observations should be stored under
  `data/observation_sets/<observation_set_id>/`
- every observation-set manifest should record its source `dataset_id`,
  windows, horizons, source sets, sentiment version, relevance rules, schemas,
  row counts, and checksums
- each run should reference a dataset snapshot instead of refetching or
  duplicating the complete input dataset, and analysis or backtest runs should
  reference a reusable observation set
- each run should write human-facing outputs into a unique directory such as
  `runs/<run_id>/`
- `runs/<run_id>/config.json`: exact run configuration used
- `runs/<run_id>/dataset_manifest.json`: exact dataset snapshot and checksums
  used by the run
- `runs/<run_id>/observation_set_manifest.json`: exact generated observation
  set used by the run
- `runs/<run_id>/reports/summary.md`: human-readable result summary and next decision
- `runs/<run_id>/reports/coverage.csv`: counts by symbol, source set, news scope, and time window
- `runs/<run_id>/reports/bucket_returns.csv`: returns grouped by sentiment bucket
- `runs/<run_id>/reports/backtest_metrics.csv`: run-level trading metrics
- `runs/<run_id>/reports/trade_log.csv`: simulated trades for the run
- `runs/<run_id>/charts/`: static charts such as sentiment buckets, source-set comparison, equity curve, drawdown, and threshold sweep

Run configuration contract:

- `run_id`: stable identifier for the experiment
- `stage`: `stage_0_fixture`, `stage_1_curated_sample`, `stage_2_source_probe`, `stage_3_hypothesis_test`, or `stage_4_expanded_validation`
- `data_mode`: `synthetic`, `curated_file`, or `api`
- `symbols`: symbols included in the run
- `source_sets`: source mixes to compare, such as `finance_only`, `broad_news`, and `finance_plus_broad`
- `news_windows`: sentiment aggregation windows, such as `trailing_1h`, `trailing_4h`, and `session_to_now`
- `measurement_horizons`: future market periods to measure, such as `next_1h`, `next_4h`, `same_day_close`, and `next_session`
- `price_intervals`: bar intervals used, such as `1h`, `15m`, `5m`, `1m`, or `1d`
- `sentiment_model`: the scoring method and version
- `long_thresholds` and `short_thresholds`: rule thresholds used for long, short, and flat decisions
- `cost_assumptions_bps`: trading cost assumptions in basis points
- `baselines`: non-sentiment controls such as `always_flat`, `random_signal`, `prior_return_momentum`, or `shuffled_sentiment`
- `split`: chronological development, validation, walk-forward, and untouched
  holdout period definitions when real data is used

Minimum observation fields:

- identity fields: `observation_id`, `symbol`, `signal_time`, `news_window`, `measurement_horizon`, `price_interval`, `source_set`
- source fields: `ticker_article_count`, `sector_theme_article_count`, `macro_article_count`, `source_count`, `publisher_count`
- sentiment fields: `mean_sentiment`, `weighted_sentiment`, `positive_article_count`, `negative_article_count`, `extreme_positive_count`, `extreme_negative_count`, `sentiment_dispersion`
- market context fields: `prior_return`, `prior_volatility`, `market_session`, `is_after_hours_signal`
- future measurement fields: `future_return`, `future_volatility`, `future_tail_event`, `future_max_drawdown`, `future_max_runup`
- audit fields: `article_ids`, `price_bar_ids`, `created_by_run_id`

Rerunnable experiment workflow:

- ingest article and price data once, preserving raw inputs
- normalize and score articles into reusable intermediate datasets
- generate `news_signal_observations` from configurable windows, source sets, symbols, and price intervals
- run analysis and backtests from versioned run configurations
- allow backtests to be rerun with different thresholds, costs, source sets, news windows, and measurement horizons without re-fetching raw data
- allow more historical news or additional sources to be ingested later and then replay the same analysis configurations

Incremental de-risking path:

- `Stage 0: dummy data`: generate a tiny synthetic dataset with a few symbols, articles, price bars, sentiment scores, and obvious expected relationships; produce sample reports and charts before using any external APIs
- `Stage 1: hand-curated real sample`: ingest or manually load a few real articles and a few real price bars for one or two symbols; verify timestamp alignment, sentiment scoring, `news_signal_observations`, and run-output generation
- `Stage 2: narrow API slice`: fetch a small real historical slice for a few symbols and a short date range; confirm rate limits, schemas, source metadata, and repeatable runs
- `Stage 3: first real experiment`: expand to the initial hybrid universe and a larger historical range only after the data model, report outputs, and rerun workflow work on the smaller slices
- each stage should produce the same artifact types, so early dummy runs exercise the same pipeline shape as later real experiments

Incremental delivery principle:

- every milestone should run end-to-end, even when the data is fake or tiny
- each milestone should produce inspectable data files, a run record, and at least one report or chart
- the pipeline shape should stay stable as data grows: ingest, normalize, score, generate `news_signal_observations`, analyze, backtest, report
- early stages should use fixture or CSV-style inputs so the system can be tested before API credentials, rate limits, and paid access become blockers
- API-specific code should sit behind source adapters so dummy data, hand-curated samples, and real vendor feeds all produce the same normalized schemas

First implementation slices:

- `Slice 1: fixture writer`
- Goal: generate raw article JSONL, `price_bars.parquet`, and
  `source_catalog.parquet` from synthetic in-memory or fixture data
- Success: files exist, schemas are stable, and one or two rows can be inspected by hand

- `Slice 2: normalization and sentiment`
- Goal: produce `normalized_articles.parquet` with deterministic simple
  sentiment scores and market relevance labels
- Success: positive, negative, neutral, ticker-specific, sector/theme, and macro examples are all represented

- `Slice 3: observation builder`
- Goal: produce `news_signal_observations.parquet` for a tiny set of
  `news_window`, `signal_time`, `measurement_horizon`, and `price_interval`
  combinations
- Success: at least one row can be traced from raw article IDs to future price bars with no lookahead

- `Slice 4: analysis report`
- Goal: produce bucket, correlation, coverage, and baseline comparison reports
- Success: the synthetic relationship is recovered, and shuffled or neutral sentiment performs worse

- `Slice 5: rule backtest`
- Goal: produce `backtest_metrics.csv`, `trade_log.csv`, and an equity-curve chart from threshold-based long/short/flat rules
- Success: changing thresholds or cost assumptions changes the run output without regenerating raw data

- `Slice 6: curated real sample`
- Goal: replace synthetic inputs with a tiny hand-curated real sample while keeping the same output contract
- Success: timestamp alignment and market-hours behavior can be inspected from raw article to trade decision

Future command shape:

- `cargo run -- fixture --config configs/stage0_fixture.json`
- `cargo run -- ingest --config <config>`
- `cargo run -- score --config <config>`
- `cargo run -- build-observations --config <config>`
- `cargo run -- analyze --config configs/stage0_fixture.json`
- `cargo run -- backtest --config configs/stage0_fixture.json`
- `cargo run -- report --run-id <run_id>`
- a convenience `run` command may execute the required stages while reusing
  existing dataset snapshots
- these commands are illustrative until implementation starts, but the plan
  should preserve separation between acquisition, transformation, analysis,
  backtesting, and reporting

Working stage gates:

- `Stage 0A: schema and fixture demo`
- Input: hardcoded or fixture-based synthetic articles and price bars
- Output: sample `raw_articles`, `normalized_articles`, `price_bars`, and `news_signal_observations`
- Gate: the program can generate the core datasets without external services
- Decision enabled: validate that the data model is understandable before touching real data

- `Stage 0B: synthetic signal demo`
- Input: synthetic sentiment deliberately constructed to predict synthetic future returns
- Output: bucket report, simple chart, and one `backtest_run`
- Gate: the analysis recovers the known fake relationship and changes when thresholds or costs change
- Decision enabled: validate that analysis and reporting plumbing works

- `Stage 1: hand-curated real sample`
- Input: a few real articles and price bars for one or two symbols
- Output: the same artifacts as Stage 0, plus a timestamp-alignment report
- Gate: market-hours, after-hours, and measurement-horizon handling are visible and correct by inspection
- Decision enabled: validate that the leakage controls make sense on real timestamps

- `Stage 2: narrow source probe`
- Input: a tiny API-backed slice for a few symbols, a short date range, and one or two source sets
- Output: source coverage report, normalized data sample, analysis report, and backtest report
- Gate: source fields, limits, timestamp quality, licensing notes, and missing data behavior are known
- Decision enabled: decide whether the chosen free sources are good enough to
  expand

- `Stage 3: first real hypothesis test`
- Input: the initial universe, selected source sets, and the free two-year
  history
- Output: correlation, lag, bucket, tail-event, and backtest reports
- Gate: the configuration is selected using the first year and evaluated once
  against the untouched second year
- Decision enabled: decide whether sentiment has enough signal to justify more data and better modeling

- `Stage 4: expanded validation`
- Input: broader history, broader universe, and any added sources that passed earlier probes
- Output: reproducible run comparisons across source sets, windows, costs, and market regimes
- Gate: the signal remains visible after costs, timestamp rules, and out-of-sample checks
- Decision enabled: decide whether to move from correlation-first research to supervised prediction or strategy refinement

De-risking rules:

- do not expand data volume until the previous stage produces usable reports
- do not optimize strategy rules until the source coverage and timestamp alignment are trusted
- do not add machine learning until rule-based bucket, tail, and long/short tests show some edge
- do not treat one strong backtest as enough; require reruns across windows, thresholds, costs, and holdout periods
- document failed experiments as first-class run outputs so the project learns from dead ends instead of repeating them

Stage completion evidence:

- each stage should be reproducible from checked-in configuration or documented run parameters
- each stage should produce machine-generated artifacts, not manually assembled screenshots or one-off spreadsheets
- each stage should leave behind enough data to inspect a single observation from raw article to sentiment score to market measurement to trade decision
- each stage should include at least one negative or control case, such as shuffled sentiment, neutral sentiment, zero-cost vs nonzero-cost runs, or a source-set comparison
- each stage should make the next decision explicit: stop, revise assumptions, add data, add sources, test more windows, or proceed to the next stage

Stage 0 POC acceptance checklist:

- fixture generation creates a dataset snapshot plus a complete
  `runs/<run_id>/` folder with its config, manifest, reports, and charts
- the same config produces identical deterministic outputs when rerun
- changing only thresholds changes `backtest_metrics.csv` and `trade_log.csv` without changing raw or normalized data
- changing only cost assumptions changes net returns without changing gross returns
- changing only `news_windows` regenerates
  `news_signal_observations.parquet` without changing raw articles or price bars
- a known synthetic positive signal produces better bucket returns than neutral or shuffled sentiment
- a known synthetic negative signal can trigger short trades and produce measurable short-side results
- at least one after-hours article is pushed to the next tradable `signal_time`
- at least one observation can be manually traced through `article_ids` and `price_bar_ids`
- reports include the next recommended decision: continue, revise, or stop

Explicit v1 exclusions:

- live trading or brokerage execution
- an interactive dashboard
- tick-level market data
- portfolio optimization
- supervised or deep-learning models
- mandatory social-media ingestion

Core data invariants:

- no observation may use an article with `published_at` later than its `signal_time`
- no observation may use a price bar ending after `signal_time` for prior-return or prior-volatility features
- every `future_return` must be measured strictly after `signal_time`
- every trade entry must occur at or after `signal_time`
- after-hours signals must enter no earlier than the next configured tradable bar
- each output row should include `created_by_run_id` or equivalent run lineage
- every backtest must report both gross and net results
- every real-data run must report missing article counts, missing price bars, and dropped observations
- every source-set comparison must use the same symbols, dates, windows, and market data unless the config explicitly says otherwise

Data quality and reproducibility safeguards:

- preserve immutable raw responses and record both publication and ingestion
  timestamps in UTC
- quarantine articles with missing or ambiguous publication times
- deduplicate syndicated and republished articles before aggregation
- use an exchange calendar that handles weekends, holidays, early closes,
  after-hours news, and daylight-saving transitions
- never silently forward-fill missing news or price bars
- make ingestion idempotent so interrupted jobs can resume safely
- store the run configuration, dataset manifest, code version, and random seed
  with every result
- mark runs with material coverage gaps invalid instead of reporting them as
  ordinary negative results

POC exit criteria:

- move from Stage 0 to Stage 1 only when fixture outputs are deterministic, traceable, and recover the known synthetic signal
- move from Stage 1 to Stage 2 only when real timestamp alignment can be inspected and no leakage is visible in the hand-curated sample
- move from Stage 2 to Stage 3 only when at least one news source and one price source produce enough usable coverage for a meaningful small experiment
- do not move to Stage 4 until Stage 3 has at least one out-of-sample result that remains interesting after costs and controls

Hypothesis validation requirements:

- compare sentiment-based runs against baseline rules that do not use sentiment
- use the first chronological year of the frozen dataset as development data
  and the second year as the untouched holdout
- use chronological walk-forward checks inside the development year for
  selecting windows and thresholds
- do not inspect or tune against the holdout year until the primary experiment
  configuration is frozen
- test whether the result survives reasonable trading costs and timestamp rules
- test whether the result is driven by broad average drift, extreme sentiment buckets, tail events, volatility expansion, or a small number of outliers
- test whether source diversity helps by comparing `finance_only`, `broad_news`, and `finance_plus_broad` runs
- prefer a modest, repeatable signal across several related configurations over one spectacular but fragile result
- treat two years as an initial signal-validation experiment rather than final
  production-grade proof across many market regimes

Go/no-go framework:

- `Stop`: data coverage is too sparse, timestamps are unreliable, results disappear under simple costs, or baselines perform as well as sentiment
- `Revise`: source coverage is usable but labels, windows, entity resolution, or sentiment scoring are visibly weak
- `Expand data`: small-sample results are directionally promising and survive controls, but confidence is limited by history length or universe size
- `Expand sources`: finance-only results are weak but macro/theme coverage improves ETF or broad-market predictions, or source-set comparisons show clear differences
- `Improve modeling`: rule-based results show repeatable edge across related windows, but simple thresholds leave obvious signal unused
- `Move to supervised prediction`: Stage 3 or Stage 4 shows out-of-sample signal after costs, with enough observations to train and validate a simple model

Bounded first experiment matrix:

- source sets: `finance_only`, `broad_news`, `finance_plus_broad`
- price intervals: `1h` first, plus `15m` only if data is available without extra friction
- news windows: `trailing_1h`, `trailing_4h`, `session_to_now`
- measurement horizons: `next_1h`, `next_4h`, `same_day_close`, `next_session`
- signal thresholds: long the top `10%` or `20%`, short the bottom `10%` or
  `20%`, and remain flat otherwise
- costs: `0 bps`, `5 bps`, `10 bps`
- baselines: `always_flat`, `random_signal`, `shuffled_sentiment`, `prior_return_momentum`
- initial symbols: `SPY`, `QQQ`, `DIA`, `IWM`, plus `5-10` liquid
  large-cap equities before expanding to roughly `25`
- initial real-data probe: a few weeks or one month before attempting the full
  two-year experiment

Experiment matrix guardrail:

- do not add more windows, symbols, sources, or models until the bounded matrix produces understandable reports
- prefer fewer configurations with clear interpretation over many configurations that invite accidental cherry-picking
- when a configuration is added, record why it was added and which previous result motivated it

Decision-demo output:

- each run should produce a short report that explains what changed, what assumptions were used, and whether the signal improved or degraded
- useful charts include sentiment bucket returns, future return by sentiment decile, source-set comparison, horizon comparison, equity curve, drawdown, and threshold-sweep heatmaps
- useful tables include top/bottom symbols by signal quality, trade logs, performance by source set, performance by news window, and performance after costs
- v1 can start with Markdown, CSV, and static plot files; an interactive dashboard is useful later but not required for proving signal

Success bar for v1:

- the held-out top-minus-bottom sentiment return spread is positive
- the long/short strategy remains profitable after moderate costs and under a
  doubled-cost stress test
- results are positive across multiple development walk-forward periods
- performance is not dominated by one symbol, source, week, or isolated event
- tail-event prediction shows measurable lift over non-sentiment baselines
- coverage and timestamp quality pass the run's validity checks

Recommended execution realism for v1:

- use `moderate realism`
- news published after market close cannot be traded until the next regular session
- intraday news can only affect trades after publication time
- apply a simple slippage/spread proxy or flat cost assumption by asset and horizon

Three practical variants were considered:

1. `ETF-first sanity check`
2. `Hybrid correlation-first pipeline`
3. `Event-study-first`

The recommended choice is `Hybrid correlation-first pipeline` because it is the best balance of acquisition proof, signal-testing depth, and implementation speed.

### 2. Supervised prediction pipeline

Build a feature table for each `ticker x time bucket` and train a model to predict:

- next-session direction
- next-session return
- next-session volatility

Candidate features:

- average sentiment
- weighted sentiment by source
- article count
- sentiment dispersion
- recency decay
- prior returns
- volume
- market regime features

Good initial models:

- logistic regression
- linear models
- gradient-boosted trees

This should come after the correlation-first pipeline establishes that signal exists.

### 3. Event-study pipeline

Treat each article as a market event and measure post-publication behavior:

- 30-minute move
- same-day move
- next-day move
- multi-day drift
- volatility response

This is useful for understanding whether specific article types, publishers, or sentiment buckets consistently move markets.

## Recommended Build Order

1. Build the Stage 0 fixture pipeline with synthetic articles, price bars, observations, analysis output, backtest output, and charts
2. Add hand-curated real samples to validate timestamp alignment and leakage controls before using external APIs
3. Build narrow source adapters for one finance/ticker feed and one broad macro/theme feed
4. Run a tiny API-backed source probe and produce coverage, quality, and timing reports
5. Expand to the first real hypothesis test only after the fixture and source-probe reports work
6. Compare source sets, news windows, measurement horizons, thresholds, and cost assumptions with rerunnable configs
7. Train predictive models only after the correlation-first and rule-based tests show a real signal worth modeling

## Data Source Findings

## News / Sentiment Data

The main finding is that affordable self-serve APIs usually do **not** provide full historical article body text. They usually provide:

- headline/title
- summary or description
- publication timestamp
- source/publisher metadata
- ticker/entity tags
- URL

That is enough for a first sentiment system.

### Best finance/ticker option

`Massive / Polygon News`

Why it stands out:

- the free Stocks Basic plan includes two years of news history
- the paid Stocks Starter plan includes all available news history, but is not
  required for v1
- ticker-tagged news
- `published_utc`
- publisher metadata
- title and description fields
- hourly updates on all individual plans

Use it as the primary `finance/ticker` news input for the first version, not as the only news input.

Official docs:

- https://massive.com/docs/rest/stocks/news

### Broad macro/theme news source

`GDELT`

Why it is worth testing:

- broad global news coverage
- useful for macro, policy, geopolitical, and theme-level signals
- free/open access model
- can help test whether non-ticker news improves ETF and broad-market predictions

Docs:

- https://www.gdeltproject.org/data.html

### Other viable news sources

`Finnhub Company News`

- ticker-linked finance news
- includes timestamp, source, summary, and related companies/tickers
- good fallback or secondary source

Docs:

- https://finnhub.io/docs/api/company-news
- https://finnhub.io/pricing

`Alpha Vantage NEWS_SENTIMENT`

- historical market news and sentiment endpoint
- useful for prototyping
- lower cost entry point
- less attractive as the sole long-term source

Docs:

- https://www.alphavantage.co/documentation/#news-sentiment
- https://www.alphavantage.co/premium/

`NewsAPI`

- broad general news coverage
- not ideal as the sole financial signal source because it lacks native ticker tagging
- potentially useful for source-diversity experiments if historical access and licensing fit the v1 budget

Docs:

- https://newsapi.org/docs/endpoints/everything
- https://newsapi.org/pricing

`X.com / social feeds`

- potentially useful for fast-moving sentiment and rumor diffusion
- excluded from v1 because API access, historical depth, pricing, and licensing
  have not been verified for this experiment
- if included, keep it as a separate `social_augmented` source set so its impact can be measured independently

### News-side recommendation

Start with:

- `Massive / Polygon` as the finance/ticker news feed
- one broad source such as `GDELT` for macro/theme coverage
- `headline + description/summary` as the text input
- custom sentiment scoring built in-house
- source-set comparisons so the experiment can measure whether broader source diversity improves or hurts signal quality

Treat any vendor sentiment field as a benchmark feature, not as ground truth.

## Market Data

The main finding is that raw market data is easiest to acquire for:

- stocks
- ETFs
- standard OHLCV bars

It becomes much more expensive and operationally heavy for:

- official benchmark index data
- futures depth
- options quote/trade history

### Best starter market source

`Alpaca Historical Stock Data`

Why it works well:

- the Basic plan is free
- historical U.S. stock and ETF data is available back to 2016
- historical access is limited to 200 API requests per minute, which is
  sufficient for an offline backtest
- strong fit for U.S. equity and ETF-based research

Docs:

- https://docs.alpaca.markets/us/docs/historical-stock-data-1
- https://docs.alpaca.markets/us/docs/about-market-data-api

### Indexes

If exact benchmark index levels are required, use a vendor with index API coverage such as `Massive Indices`.

Docs:

- https://massive.com/docs/rest/indices/aggregates/daily-ticker-summary

But for the first version, ETF proxies are the better choice because they are easier to access and model:

- `SPY` for S&P 500 exposure
- `QQQ` for Nasdaq-100 exposure
- `DIA` for Dow exposure
- `IWM` for Russell 2000 exposure

This proxy choice is an implementation recommendation based on licensing and simplicity.

### Commodities, Futures, Options

If later work requires raw exchange-grade history:

- `CME DataMine` for futures and commodity futures
- `Cboe DataShop` for options data

Docs:

- https://www.cmegroup.com/datamine.html
- https://datashop.cboe.com/

These should not be part of the first implementation unless the research specifically depends on them.

## Initial Dataset Recommendation

For version 1, use:

- News: free `Massive / Polygon Stocks Basic` for two years of finance/ticker
  news, plus free `GDELT` for macro/theme coverage
- Prices: free `Alpaca Basic`
- Universe: `SPY`, `QQQ`, `DIA`, `IWM`, plus a small set of liquid U.S. equities
- Time range: the latest common two-year interval available at first ingestion,
  frozen in the dataset manifest
- Granularity: hourly primary, `15-minute` secondary when reliable, and daily
  as a sanity-check baseline
- Sentiment: deterministic local scoring with no model API cost
- Expected vendor cost: `$0`

This is the best balance of:

- cost
- implementation speed
- data quality
- backtest practicality

## Important Caveats

- Full article-body history is the hardest part to obtain legally and cheaply.
- GDELT's source data is free, but optional BigQuery use may create Google
  Cloud charges; v1 should prefer direct downloads.
- Vendor retention and redistribution terms must be checked before sharing or
  publishing downloaded datasets.
- Two years can establish whether a signal deserves more investment, but it
  cannot prove robustness across many market regimes.
- Historical constituent backtests are harder than index-level backtests.
- Market/news timestamp alignment matters; pre-market, intraday, and post-close news should be handled differently.
- Prediction should only be attempted after validating that the signal survives out-of-sample testing.

## Near-Term Next Steps

1. Define the Stage 0 fixture schema for synthetic articles, synthetic price bars, and expected synthetic signal behavior
2. Define the output contract for `news_signal_observations`, `analysis_runs`, `backtest_runs`, reports, and charts
3. Build the first runnable fixture demo before accessing external APIs
4. Define the initial source catalog and source-set experiments for the Stage 2 source probe
5. Implement adapters for the free Massive, GDELT, and Alpaca access paths
6. Run the first narrow API-backed source probe
7. Expand to the free two-year hypothesis test only after the source probe
   produces usable coverage and timing reports
