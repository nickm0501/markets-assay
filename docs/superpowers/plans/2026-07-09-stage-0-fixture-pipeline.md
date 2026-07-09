# Stage 0 Fixture Pipeline Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build a deterministic synthetic end-to-end market-sentiment research pipeline that writes fixture snapshots, reusable observation sets, analysis reports, backtest reports, and charts without external data.

**Architecture:** Stage 0 uses a local fixture generator instead of vendor adapters, but it writes the same dataset, observation-set, and run artifacts planned for real data. The CLI separates snapshot creation, observation building, analysis, backtesting, reporting, and full orchestration so later stages can replace fixture inputs without redesigning the research loop.

**Tech Stack:** Rust 2024, `clap`, `serde`, `serde_json`, `csv`, `chrono`, `chrono-tz`, `arrow`, `parquet`, `serde_arrow`, `plotters`, `sha2`, `hex`, `assert_cmd`, `tempfile`.

---

## Scope

This plan implements only Stage 0 from `docs/superpowers/specs/2026-07-08-correlation-first-pipeline-design.md`: a deterministic fixture pipeline and synthetic signal demo.

Stage 0 must prove these properties before any API work:

- A dataset snapshot is generated once and reused.
- `observation_set_id` artifacts are derived from a dataset snapshot and aggregation config.
- Many `run_id` analysis/backtest outputs can be run against the same observation set.
- Positive and negative synthetic news can drive long and short trades.
- Changing costs or thresholds reruns analysis/backtests without regenerating source data.
- Reports show whether the next decision is `stop`, `revise`, `expand data`, `expand sources`, or `continue`.

Real Massive, GDELT, Alpaca, paid validation, and large historical ingestion are out of scope for this plan.

## File Structure

- Modify: `Cargo.toml` - add runtime and test dependencies.
- Modify: `.gitignore` - ignore generated artifacts.
- Create: `configs/stage0_fixture.json` - checked-in deterministic fixture configuration.
- Modify: `src/main.rs` - thin binary entrypoint.
- Create: `src/lib.rs` - library module exports used by CLI and tests.
- Create: `src/cli.rs` - `clap` commands and argument parsing.
- Create: `src/config.rs` - config loading, defaults, validation, and canonical hashing.
- Create: `src/domain/mod.rs` - domain module declarations.
- Create: `src/domain/article.rs` - raw and normalized article records.
- Create: `src/domain/market.rs` - price bars, sessions, and return helpers.
- Create: `src/domain/observation.rs` - `NewsSignalObservation` schema.
- Create: `src/domain/run.rs` - analysis, trade, and backtest result records.
- Create: `src/calendar.rs` - deterministic fixture trading-calendar helpers.
- Create: `src/ids.rs` - stable SHA-256 based identifiers and canonical JSON helpers.
- Create: `src/fixture.rs` - synthetic article and price-bar generator.
- Create: `src/sentiment.rs` - deterministic local lexicon sentiment scorer.
- Create: `src/normalize.rs` - deduplication, relevance mapping, and normalization.
- Create: `src/storage/mod.rs` - storage module declarations.
- Create: `src/storage/jsonl.rs` - JSONL read/write helpers.
- Create: `src/storage/parquet.rs` - Parquet read/write helpers.
- Create: `src/storage/manifest.rs` - dataset and observation manifest structs.
- Create: `src/observations.rs` - news-window aggregation and future measurement builder.
- Create: `src/analysis.rs` - coverage, bucket return, correlation, and baseline analysis.
- Create: `src/backtest.rs` - long/short/flat rule engine and metrics.
- Create: `src/report.rs` - Markdown, CSV, and SVG report generation.
- Create: `src/pipeline.rs` - orchestration for CLI subcommands.
- Create: `tests/stage0_cli.rs` - end-to-end CLI tests.

## Artifact Layout

The default config writes under `artifacts/`:

```text
artifacts/
  data/
    datasets/<dataset_id>/
      raw/raw_articles.jsonl
      normalized_articles.parquet
      price_bars.parquet
      source_catalog.parquet
      manifest.json
    observation_sets/<observation_set_id>/
      news_signal_observations.parquet
      manifest.json
  runs/<run_id>/
    config.json
    dataset_manifest.json
    observation_set_manifest.json
    reports/summary.md
    reports/coverage.csv
    reports/bucket_returns.csv
    reports/backtest_metrics.csv
    reports/trade_log.csv
    charts/bucket_returns.svg
    charts/equity_curve.svg
```

## Commands

Stage 0 exposes these commands:

```bash
cargo run -- run --config configs/stage0_fixture.json
cargo run -- fixture --config configs/stage0_fixture.json
cargo run -- build-observations --config configs/stage0_fixture.json --dataset-id <dataset_id>
cargo run -- analyze --config configs/stage0_fixture.json --observation-set-id <observation_set_id>
cargo run -- backtest --config configs/stage0_fixture.json --observation-set-id <observation_set_id> --cost-bps 5
```

`run` orchestrates all stages. The other commands make reruns explicit and are required so future stages can reuse source snapshots without redownloading data.

## Configuration Contract

`configs/stage0_fixture.json`:

```json
{
  "run_id": "stage0_fixture",
  "output_root": "artifacts",
  "generated_at": "2026-07-09T00:00:00Z",
  "symbols": ["SPY", "QQQ"],
  "source_sets": ["finance_only", "broad_news", "finance_plus_broad"],
  "news_windows_minutes": [60, 240],
  "measurement_horizons_minutes": [60],
  "price_interval_minutes": 60,
  "long_quantile": 0.8,
  "short_quantile": 0.2,
  "costs_bps": [0.0, 5.0, 10.0],
  "holidays": ["2026-07-03"],
  "early_closes": {},
  "theme_symbol_map": {
    "technology": ["QQQ"],
    "rates": ["SPY", "QQQ"],
    "housing": ["SPY"]
  },
  "macro_symbols": ["SPY", "QQQ"]
}
```

## Domain Contracts

`dataset_id` is generated from normalized source snapshot manifests and checksums. It changes only when input records, source metadata, or schema versions change.

`observation_set_id` is generated from `dataset_id`, aggregation config, sentiment version, relevance-rule version, and observation checksums. It changes when windows, horizons, source sets, relevance rules, or scoring versions change.

`run_id` identifies one analysis/backtest/report output. It may change costs or thresholds without changing `dataset_id` or `observation_set_id`.

`published_at` is the article timestamp from the source. `available_at` is when Stage 0 allows the signal to use that article. For regular-hours articles they match. For after-hours articles, `available_at` is deferred to the next regular-session signal time to enforce no-lookahead trading.

## Task 1: Dependencies, Config, And CLI Skeleton

**Files:**
- Modify: `Cargo.toml`
- Modify: `.gitignore`
- Create: `configs/stage0_fixture.json`
- Modify: `src/main.rs`
- Create: `src/lib.rs`
- Create: `src/cli.rs`
- Create: `src/config.rs`
- Test: `tests/stage0_cli.rs`

- [ ] **Step 1: Add the failing CLI/config tests**

Create `tests/stage0_cli.rs`:

```rust
use assert_cmd::Command;
use predicates::prelude::*;
use tempfile::TempDir;

#[test]
fn help_lists_stage0_commands() {
    let mut cmd = Command::cargo_bin("markets").unwrap();
    cmd.arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("run"))
        .stdout(predicate::str::contains("fixture"))
        .stdout(predicate::str::contains("build-observations"))
        .stdout(predicate::str::contains("analyze"))
        .stdout(predicate::str::contains("backtest"));
}

#[test]
fn run_rejects_missing_config_file() {
    let mut cmd = Command::cargo_bin("markets").unwrap();
    cmd.args(["run", "--config", "configs/not-real.json"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("failed to read config"));
}

#[test]
fn run_accepts_checked_in_stage0_config() {
    let temp = TempDir::new().unwrap();
    let mut cmd = Command::cargo_bin("markets").unwrap();
    cmd.args([
        "run",
        "--config",
        "configs/stage0_fixture.json",
        "--output-root",
        temp.path().to_str().unwrap(),
        "--dry-run",
    ])
    .assert()
    .success()
    .stdout(predicate::str::contains("stage0_fixture"))
    .stdout(predicate::str::contains("dry_run=true"));
}
```

- [ ] **Step 2: Run tests and verify they fail for missing CLI behavior**

Run:

```bash
cargo test --test stage0_cli
```

Expected: `help_lists_stage0_commands` fails because the binary still prints `Hello, world!`.

- [ ] **Step 3: Add dependencies**

Modify `Cargo.toml`:

```toml
[package]
name = "markets"
version = "0.1.0"
edition = "2024"

[dependencies]
anyhow = "1"
arrow = "59.0"
chrono = { version = "0.4", features = ["serde"] }
chrono-tz = "0.10"
clap = { version = "4.6", features = ["derive"] }
csv = "1"
hex = "0.4"
parquet = { version = "59.0", features = ["arrow"] }
plotters = { version = "0.3.7", default-features = false, features = ["line_series", "svg_backend"] }
serde = { version = "1", features = ["derive"] }
serde_arrow = { version = "0.14.2", features = ["arrow-59"] }
serde_json = "1"
sha2 = "0.10"

[dev-dependencies]
assert_cmd = "2"
predicates = "3"
tempfile = "3"
```

- [ ] **Step 4: Add generated-artifact ignores**

Modify `.gitignore`:

```gitignore
target/
artifacts/
```

- [ ] **Step 5: Add the Stage 0 config file**

Create `configs/stage0_fixture.json` with the JSON shown in the Configuration Contract section.

- [ ] **Step 6: Add the library module root**

Create `src/lib.rs`:

```rust
pub mod cli;
pub mod config;

pub use cli::run_cli;
```

- [ ] **Step 7: Implement config loading and validation**

Create `src/config.rs`:

```rust
use anyhow::{Context, Result, bail};
use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, fs, path::Path};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Stage0Config {
    pub run_id: String,
    pub output_root: String,
    pub generated_at: DateTime<Utc>,
    pub symbols: Vec<String>,
    pub source_sets: Vec<String>,
    pub news_windows_minutes: Vec<i64>,
    pub measurement_horizons_minutes: Vec<i64>,
    pub price_interval_minutes: i64,
    pub long_quantile: f64,
    pub short_quantile: f64,
    pub costs_bps: Vec<f64>,
    pub holidays: Vec<NaiveDate>,
    pub early_closes: BTreeMap<NaiveDate, String>,
    pub theme_symbol_map: BTreeMap<String, Vec<String>>,
    pub macro_symbols: Vec<String>,
}

impl Stage0Config {
    pub fn load(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        let text = fs::read_to_string(path)
            .with_context(|| format!("failed to read config {}", path.display()))?;
        let config: Self = serde_json::from_str(&text)
            .with_context(|| format!("failed to parse config {}", path.display()))?;
        config.validate()?;
        Ok(config)
    }

    pub fn validate(&self) -> Result<()> {
        if self.run_id.trim().is_empty() {
            bail!("run_id must not be empty");
        }
        if self.symbols.is_empty() {
            bail!("symbols must not be empty");
        }
        if self.news_windows_minutes.iter().any(|minutes| *minutes <= 0) {
            bail!("news_windows_minutes must be positive");
        }
        if self.measurement_horizons_minutes.iter().any(|minutes| *minutes <= 0) {
            bail!("measurement_horizons_minutes must be positive");
        }
        if !(0.0..1.0).contains(&self.short_quantile) {
            bail!("short_quantile must be between 0 and 1");
        }
        if !(0.0..1.0).contains(&self.long_quantile) {
            bail!("long_quantile must be between 0 and 1");
        }
        if self.short_quantile >= self.long_quantile {
            bail!("short_quantile must be below long_quantile");
        }
        Ok(())
    }
}
```

- [ ] **Step 8: Implement the CLI skeleton**

Create `src/cli.rs`:

```rust
use crate::config::Stage0Config;
use anyhow::Result;
use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Debug, Parser)]
#[command(name = "markets")]
#[command(about = "Correlation-first market sentiment research pipeline")]
pub struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Debug, Subcommand)]
enum Commands {
    Run(StageArgs),
    Fixture(StageArgs),
    BuildObservations(StageArgsWithDataset),
    Analyze(StageArgsWithObservationSet),
    Backtest(BacktestArgs),
}

#[derive(Debug, Parser)]
struct StageArgs {
    #[arg(long)]
    config: PathBuf,
    #[arg(long)]
    output_root: Option<PathBuf>,
    #[arg(long, default_value_t = false)]
    dry_run: bool,
}

#[derive(Debug, Parser)]
struct StageArgsWithDataset {
    #[command(flatten)]
    stage: StageArgs,
    #[arg(long)]
    dataset_id: String,
}

#[derive(Debug, Parser)]
struct StageArgsWithObservationSet {
    #[command(flatten)]
    stage: StageArgs,
    #[arg(long)]
    observation_set_id: String,
}

#[derive(Debug, Parser)]
struct BacktestArgs {
    #[command(flatten)]
    observation: StageArgsWithObservationSet,
    #[arg(long)]
    cost_bps: Option<f64>,
}

pub fn run_cli() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Commands::Run(args) => print_loaded_config(args, "run"),
        Commands::Fixture(args) => print_loaded_config(args, "fixture"),
        Commands::BuildObservations(args) => print_loaded_config(args.stage, "build-observations"),
        Commands::Analyze(args) => print_loaded_config(args.stage, "analyze"),
        Commands::Backtest(args) => print_loaded_config(args.observation.stage, "backtest"),
    }
}

fn print_loaded_config(args: StageArgs, command_name: &str) -> Result<()> {
    let mut config = Stage0Config::load(&args.config)?;
    if let Some(output_root) = args.output_root {
        config.output_root = output_root.display().to_string();
    }
    println!(
        "command={command_name} run_id={} output_root={} dry_run={}",
        config.run_id, config.output_root, args.dry_run
    );
    Ok(())
}
```

- [ ] **Step 9: Replace the binary entrypoint**

Modify `src/main.rs`:

```rust
use anyhow::Result;

fn main() -> Result<()> {
    markets::run_cli()
}
```

- [ ] **Step 10: Run tests and formatting**

Run:

```bash
cargo fmt
cargo test --test stage0_cli
```

Expected: all three tests pass.

- [ ] **Step 11: Commit**

Run:

```bash
git add Cargo.toml Cargo.lock .gitignore configs/stage0_fixture.json src/main.rs src/lib.rs src/cli.rs src/config.rs tests/stage0_cli.rs
git commit -m "feat: add stage0 cli skeleton"
```

## Task 2: Domain Models, Stable IDs, And Fixture Calendar

**Files:**
- Modify: `src/lib.rs`
- Create: `src/domain/mod.rs`
- Create: `src/domain/article.rs`
- Create: `src/domain/market.rs`
- Create: `src/domain/observation.rs`
- Create: `src/domain/run.rs`
- Create: `src/ids.rs`
- Create: `src/calendar.rs`

- [ ] **Step 1: Add tests for IDs, article availability, and next tradable bars**

Create module tests in the files named below. The first test goes in `src/ids.rs`, the second in `src/domain/article.rs`, and the third in `src/calendar.rs`:

```rust
#[test]
fn stable_id_uses_canonical_json() {
    #[derive(serde::Serialize)]
    struct Sample {
        symbol: &'static str,
        value: i32,
    }

    let first = stable_id("ds", &Sample { symbol: "SPY", value: 7 }).unwrap();
    let second = stable_id("ds", &Sample { symbol: "SPY", value: 7 }).unwrap();

    assert_eq!(first, second);
    assert!(first.starts_with("ds_"));
    assert_eq!(first.len(), 19);
}
```

```rust
#[test]
fn after_hours_article_can_be_deferred_to_next_signal_time() {
    let article = RawArticle {
        vendor_id: "a-after-close".into(),
        source: "fixture_macro".into(),
        source_kind: SourceKind::Broad,
        published_at: "2026-07-02T21:15:00Z".parse().unwrap(),
        title: "Fed shock weighs on markets".into(),
        summary: "Negative macro surprise after the close".into(),
        url: "fixture://after-close".into(),
        tickers: vec![],
        themes: vec!["rates".into()],
    };

    assert_eq!(article.published_at.to_rfc3339(), "2026-07-02T21:15:00+00:00");
}
```

```rust
#[test]
fn next_regular_signal_skips_weekend_and_fixture_holiday() {
    use chrono::{TimeZone, Utc};

    let holidays = vec!["2026-07-03".parse().unwrap()];
    let after_close = Utc.with_ymd_and_hms(2026, 7, 2, 21, 15, 0).unwrap();
    let next = next_regular_signal_time(after_close, 60, &holidays);

    assert_eq!(next.to_rfc3339(), "2026-07-06T13:30:00+00:00");
}
```

- [ ] **Step 2: Run tests and verify they fail because modules do not exist**

Run:

```bash
cargo test stable_id_uses_canonical_json after_hours_article_can_be_deferred_to_next_signal_time next_regular_signal_skips_weekend_and_fixture_holiday
```

Expected: compile failure for missing modules/types.

- [ ] **Step 3: Export new modules**

Modify `src/lib.rs`:

```rust
pub mod calendar;
pub mod cli;
pub mod config;
pub mod domain;
pub mod ids;

pub use cli::run_cli;
```

- [ ] **Step 4: Add stable IDs**

Create `src/ids.rs`:

```rust
use anyhow::Result;
use serde::Serialize;
use sha2::{Digest, Sha256};

pub fn stable_id(prefix: &str, value: &impl Serialize) -> Result<String> {
    let bytes = serde_json::to_vec(value)?;
    let digest = Sha256::digest(bytes);
    let hex = hex::encode(digest);
    Ok(format!("{prefix}_{}", &hex[..16]))
}

pub fn sha256_hex(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}
```

- [ ] **Step 5: Add domain module declarations**

Create `src/domain/mod.rs`:

```rust
pub mod article;
pub mod market;
pub mod observation;
pub mod run;
```

- [ ] **Step 6: Add article models**

Create `src/domain/article.rs`:

```rust
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceKind {
    Finance,
    Broad,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NewsScope {
    TickerSpecific,
    SectorTheme,
    MacroMarket,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SentimentLabel {
    Positive,
    Neutral,
    Negative,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RawArticle {
    pub vendor_id: String,
    pub source: String,
    pub source_kind: SourceKind,
    pub published_at: DateTime<Utc>,
    pub title: String,
    pub summary: String,
    pub url: String,
    pub tickers: Vec<String>,
    pub themes: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NormalizedArticle {
    pub article_id: String,
    pub vendor_id: String,
    pub source: String,
    pub source_kind: SourceKind,
    pub published_at: DateTime<Utc>,
    pub available_at: DateTime<Utc>,
    pub title: String,
    pub summary: String,
    pub url: String,
    pub tickers: Vec<String>,
    pub themes: Vec<String>,
    pub scope: NewsScope,
    pub relevant_symbols: Vec<String>,
    pub sentiment_score: f64,
    pub sentiment_label: SentimentLabel,
    pub dedupe_key: String,
}
```

- [ ] **Step 7: Add market models**

Create `src/domain/market.rs`:

```rust
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PriceBar {
    pub bar_id: String,
    pub symbol: String,
    pub start_time: DateTime<Utc>,
    pub end_time: DateTime<Utc>,
    pub open: f64,
    pub high: f64,
    pub low: f64,
    pub close: f64,
    pub volume: u64,
}

impl PriceBar {
    pub fn return_pct(&self) -> f64 {
        (self.close - self.open) / self.open
    }
}
```

- [ ] **Step 8: Add observation models**

Create `src/domain/observation.rs`:

```rust
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NewsSignalObservation {
    pub observation_id: String,
    pub dataset_id: String,
    pub symbol: String,
    pub signal_time: DateTime<Utc>,
    pub news_window_minutes: i64,
    pub measurement_horizon_minutes: i64,
    pub price_interval_minutes: i64,
    pub source_set: String,
    pub article_count: u32,
    pub ticker_article_count: u32,
    pub sector_theme_article_count: u32,
    pub macro_article_count: u32,
    pub source_count: u32,
    pub publisher_count: u32,
    pub mean_sentiment: f64,
    pub weighted_sentiment: f64,
    pub positive_article_count: u32,
    pub negative_article_count: u32,
    pub sentiment_dispersion: f64,
    pub prior_return: f64,
    pub prior_volatility: f64,
    pub market_session: String,
    pub is_after_hours_signal: bool,
    pub future_return: f64,
    pub future_volatility: f64,
    pub future_tail_event: bool,
    pub future_max_drawdown: f64,
    pub future_max_runup: f64,
    pub entry_time: DateTime<Utc>,
    pub exit_time: DateTime<Utc>,
    pub article_ids: Vec<String>,
    pub price_bar_ids: Vec<String>,
    pub created_by_run_id: String,
}
```

- [ ] **Step 9: Add run result models**

Create `src/domain/run.rs`:

```rust
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CoverageRow {
    pub source_set: String,
    pub symbol: String,
    pub observation_count: u32,
    pub article_count: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BucketReturnRow {
    pub source_set: String,
    pub bucket: String,
    pub observation_count: u32,
    pub mean_sentiment: f64,
    pub mean_future_return: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Trade {
    pub run_id: String,
    pub observation_id: String,
    pub symbol: String,
    pub side: String,
    pub signal_time: DateTime<Utc>,
    pub entry_time: DateTime<Utc>,
    pub exit_time: DateTime<Utc>,
    pub sentiment: f64,
    pub gross_return: f64,
    pub cost_bps: f64,
    pub net_return: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BacktestMetrics {
    pub run_id: String,
    pub cost_bps: f64,
    pub trade_count: u32,
    pub long_count: u32,
    pub short_count: u32,
    pub gross_return_sum: f64,
    pub net_return_sum: f64,
    pub average_net_return: f64,
    pub win_rate: f64,
    pub profit_factor: f64,
    pub max_drawdown: f64,
}
```

- [ ] **Step 10: Add fixture calendar helpers**

Create `src/calendar.rs`:

```rust
use chrono::{DateTime, Datelike, Duration, NaiveDate, NaiveTime, TimeZone, Utc, Weekday};

const OPEN_HOUR_UTC: u32 = 13;
const OPEN_MINUTE_UTC: u32 = 30;
const CLOSE_HOUR_UTC: u32 = 20;

pub fn is_trading_day(date: NaiveDate, holidays: &[NaiveDate]) -> bool {
    !matches!(date.weekday(), Weekday::Sat | Weekday::Sun) && !holidays.contains(&date)
}

pub fn regular_open(date: NaiveDate) -> DateTime<Utc> {
    Utc.from_utc_datetime(&date.and_time(NaiveTime::from_hms_opt(OPEN_HOUR_UTC, OPEN_MINUTE_UTC, 0).unwrap()))
}

pub fn regular_close(date: NaiveDate) -> DateTime<Utc> {
    Utc.from_utc_datetime(&date.and_time(NaiveTime::from_hms_opt(CLOSE_HOUR_UTC, 0, 0).unwrap()))
}

pub fn is_regular_session(time: DateTime<Utc>, holidays: &[NaiveDate]) -> bool {
    let date = time.date_naive();
    is_trading_day(date, holidays) && time >= regular_open(date) && time < regular_close(date)
}

pub fn next_regular_signal_time(time: DateTime<Utc>, interval_minutes: i64, holidays: &[NaiveDate]) -> DateTime<Utc> {
    let mut date = time.date_naive();
    loop {
        if is_trading_day(date, holidays) {
            let open = regular_open(date);
            let close = regular_close(date);
            if time <= open {
                return open;
            }
            if time < close {
                let elapsed = time - open;
                let intervals = (elapsed.num_minutes() + interval_minutes - 1) / interval_minutes;
                return open + Duration::minutes(intervals * interval_minutes);
            }
        }
        date = date.succ_opt().unwrap();
    }
}
```

- [ ] **Step 11: Run tests and commit**

Run:

```bash
cargo fmt
cargo test stable_id_uses_canonical_json after_hours_article_can_be_deferred_to_next_signal_time next_regular_signal_skips_weekend_and_fixture_holiday
```

Expected: all selected tests pass.

Commit:

```bash
git add src/lib.rs src/domain src/ids.rs src/calendar.rs
git commit -m "feat: add stage0 domain contracts"
```

## Task 3: Deterministic Fixture Generator

**Files:**
- Modify: `src/lib.rs`
- Create: `src/fixture.rs`
- Modify: `src/pipeline.rs`
- Modify: `src/cli.rs`
- Test: `src/fixture.rs`
- Test: `tests/stage0_cli.rs`

- [ ] **Step 1: Add fixture generator tests**

Create `src/fixture.rs` with tests first:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Stage0Config;

    fn config() -> Stage0Config {
        Stage0Config::load("configs/stage0_fixture.json").unwrap()
    }

    #[test]
    fn fixture_generation_is_deterministic() {
        let first = generate_fixture(&config()).unwrap();
        let second = generate_fixture(&config()).unwrap();

        assert_eq!(first.raw_articles, second.raw_articles);
        assert_eq!(first.price_bars, second.price_bars);
    }

    #[test]
    fn fixture_contains_positive_negative_macro_theme_duplicate_and_after_hours_cases() {
        let fixture = generate_fixture(&config()).unwrap();

        assert!(fixture.raw_articles.iter().any(|a| a.title.contains("breakout")));
        assert!(fixture.raw_articles.iter().any(|a| a.title.contains("downgrade")));
        assert!(fixture.raw_articles.iter().any(|a| a.themes.contains(&"rates".to_string())));
        assert!(fixture.raw_articles.iter().any(|a| a.themes.contains(&"technology".to_string())));
        assert!(fixture.raw_articles.iter().filter(|a| a.url == "fixture://spy-breakout").count() >= 2);
        assert!(fixture.raw_articles.iter().any(|a| a.published_at.to_rfc3339() == "2026-07-02T21:15:00+00:00"));
    }

    #[test]
    fn fixture_prices_include_known_positive_and_negative_future_moves() {
        let fixture = generate_fixture(&config()).unwrap();
        let spy_returns: Vec<f64> = fixture.price_bars.iter()
            .filter(|bar| bar.symbol == "SPY")
            .map(|bar| bar.return_pct())
            .collect();
        let qqq_returns: Vec<f64> = fixture.price_bars.iter()
            .filter(|bar| bar.symbol == "QQQ")
            .map(|bar| bar.return_pct())
            .collect();

        assert!(spy_returns.iter().any(|value| *value > 0.006));
        assert!(qqq_returns.iter().any(|value| *value < -0.006));
    }
}
```

- [ ] **Step 2: Run tests and verify they fail for missing generator**

Run:

```bash
cargo test fixture_generation_is_deterministic fixture_contains_positive_negative_macro_theme_duplicate_and_after_hours_cases fixture_prices_include_known_positive_and_negative_future_moves
```

Expected: compile failure for missing `generate_fixture`.

- [ ] **Step 3: Export fixture and pipeline modules**

Modify `src/lib.rs`:

```rust
pub mod calendar;
pub mod cli;
pub mod config;
pub mod domain;
pub mod fixture;
pub mod ids;
pub mod pipeline;

pub use cli::run_cli;
```

- [ ] **Step 4: Implement deterministic fixture records**

Create `src/fixture.rs` above the tests:

```rust
use crate::{
    calendar::{regular_open, is_trading_day},
    config::Stage0Config,
    domain::{article::{RawArticle, SourceKind}, market::PriceBar},
    ids::stable_id,
};
use anyhow::Result;
use chrono::{Duration, TimeZone, Utc};

#[derive(Debug, Clone, PartialEq)]
pub struct FixtureData {
    pub raw_articles: Vec<RawArticle>,
    pub price_bars: Vec<PriceBar>,
}

pub fn generate_fixture(config: &Stage0Config) -> Result<FixtureData> {
    let raw_articles = vec![
        article("massive-1", "fixture_finance", SourceKind::Finance, "2026-06-29T14:05:00Z", "SPY breakout on broad earnings strength", "Analysts call the move strong and constructive.", "fixture://spy-breakout", vec!["SPY"], vec![]),
        article("massive-1-dup", "fixture_wire", SourceKind::Finance, "2026-06-29T14:07:00Z", "SPY breakout on broad earnings strength", "Analysts call the move strong and constructive.", "fixture://spy-breakout", vec!["SPY"], vec![]),
        article("massive-2", "fixture_finance", SourceKind::Finance, "2026-06-30T15:05:00Z", "QQQ downgrade follows weak chip demand", "The note described weak orders and negative guidance.", "fixture://qqq-downgrade", vec!["QQQ"], vec!["technology"]),
        article("gdelt-1", "fixture_macro", SourceKind::Broad, "2026-07-01T13:40:00Z", "Rates relief boosts risk appetite", "Lower yields are positive for growth stocks and broad indexes.", "fixture://rates-relief", vec![], vec!["rates"]),
        article("gdelt-2", "fixture_macro", SourceKind::Broad, "2026-07-02T21:15:00Z", "Fed shock weighs on markets", "A surprise hawkish turn is negative for stocks after the close.", "fixture://after-close", vec![], vec!["rates"]),
        article("gdelt-3", "fixture_housing", SourceKind::Broad, "2026-07-06T14:10:00Z", "Housing data remains neutral", "Mixed data left investors with a neutral read.", "fixture://housing-neutral", vec![], vec!["housing"]),
    ];
    let price_bars = generate_price_bars(config)?;
    Ok(FixtureData { raw_articles, price_bars })
}

fn article(
    vendor_id: &str,
    source: &str,
    source_kind: SourceKind,
    published_at: &str,
    title: &str,
    summary: &str,
    url: &str,
    tickers: Vec<&str>,
    themes: Vec<&str>,
) -> RawArticle {
    RawArticle {
        vendor_id: vendor_id.into(),
        source: source.into(),
        source_kind,
        published_at: published_at.parse().unwrap(),
        title: title.into(),
        summary: summary.into(),
        url: url.into(),
        tickers: tickers.into_iter().map(String::from).collect(),
        themes: themes.into_iter().map(String::from).collect(),
    }
}

fn generate_price_bars(config: &Stage0Config) -> Result<Vec<PriceBar>> {
    let mut bars = Vec::new();
    let mut date = Utc.with_ymd_and_hms(2026, 6, 29, 0, 0, 0).unwrap().date_naive();
    while date <= Utc.with_ymd_and_hms(2026, 7, 7, 0, 0, 0).unwrap().date_naive() {
        if is_trading_day(date, &config.holidays) {
            for symbol in &config.symbols {
                let mut start = regular_open(date);
                let mut price = if symbol == "SPY" { 500.0 } else { 425.0 };
                while start < crate::calendar::regular_close(date) {
                    let ret = fixture_return(symbol, start);
                    let open = price;
                    let close = open * (1.0 + ret);
                    let high = open.max(close) * 1.001;
                    let low = open.min(close) * 0.999;
                    let end_time = start + Duration::minutes(config.price_interval_minutes);
                    let bar_id = stable_id("bar", &(symbol, start, end_time))?;
                    bars.push(PriceBar {
                        bar_id,
                        symbol: symbol.clone(),
                        start_time: start,
                        end_time,
                        open,
                        high,
                        low,
                        close,
                        volume: 1_000_000,
                    });
                    price = close;
                    start = end_time;
                }
            }
        }
        date = date.succ_opt().unwrap();
    }
    Ok(bars)
}

fn fixture_return(symbol: &str, start: chrono::DateTime<Utc>) -> f64 {
    match (symbol, start.to_rfc3339().as_str()) {
        ("SPY", "2026-06-29T15:30:00+00:00") => 0.008,
        ("QQQ", "2026-06-30T16:30:00+00:00") => -0.009,
        ("SPY", "2026-07-01T14:30:00+00:00") => 0.006,
        ("QQQ", "2026-07-01T14:30:00+00:00") => 0.007,
        ("SPY", "2026-07-06T13:30:00+00:00") => -0.007,
        ("QQQ", "2026-07-06T13:30:00+00:00") => -0.008,
        _ => 0.0005,
    }
}
```

- [ ] **Step 5: Add the initial pipeline functions used by CLI**

Create `src/pipeline.rs`:

```rust
use crate::{config::Stage0Config, fixture::generate_fixture};
use anyhow::Result;
use std::path::PathBuf;

pub fn run_fixture(config: &Stage0Config, output_root: Option<PathBuf>, dry_run: bool) -> Result<()> {
    let fixture = generate_fixture(config)?;
    let root = output_root.unwrap_or_else(|| PathBuf::from(&config.output_root));
    println!(
        "fixture run_id={} output_root={} dry_run={} articles={} price_bars={}",
        config.run_id,
        root.display(),
        dry_run,
        fixture.raw_articles.len(),
        fixture.price_bars.len()
    );
    Ok(())
}
```

- [ ] **Step 6: Route `fixture` and `run` through the generator**

Modify the matching arm in `src/cli.rs`:

```rust
use crate::{config::Stage0Config, pipeline};
```

Replace `Commands::Run` and `Commands::Fixture` arms:

```rust
Commands::Run(args) => run_loaded_config(args, pipeline::run_fixture),
Commands::Fixture(args) => run_loaded_config(args, pipeline::run_fixture),
```

Add helper:

```rust
fn run_loaded_config(
    args: StageArgs,
    runner: fn(&Stage0Config, Option<PathBuf>, bool) -> Result<()>,
) -> Result<()> {
    let config = Stage0Config::load(&args.config)?;
    runner(&config, args.output_root, args.dry_run)
}
```

Keep `print_loaded_config` for the non-implemented subcommands until later tasks replace it.

- [ ] **Step 7: Add an integration assertion for fixture output summary**

Append to `tests/stage0_cli.rs`:

```rust
#[test]
fn fixture_command_prints_generated_record_counts() {
    let temp = TempDir::new().unwrap();
    let mut cmd = Command::cargo_bin("markets").unwrap();
    cmd.args([
        "fixture",
        "--config",
        "configs/stage0_fixture.json",
        "--output-root",
        temp.path().to_str().unwrap(),
        "--dry-run",
    ])
    .assert()
    .success()
    .stdout(predicate::str::contains("articles=6"))
    .stdout(predicate::str::contains("price_bars="));
}
```

- [ ] **Step 8: Run tests and commit**

Run:

```bash
cargo fmt
cargo test fixture_generation_is_deterministic fixture_contains_positive_negative_macro_theme_duplicate_and_after_hours_cases fixture_prices_include_known_positive_and_negative_future_moves
cargo test --test stage0_cli fixture_command_prints_generated_record_counts
```

Expected: all selected tests pass.

Commit:

```bash
git add src/lib.rs src/fixture.rs src/pipeline.rs src/cli.rs tests/stage0_cli.rs
git commit -m "feat: generate deterministic stage0 fixtures"
```

## Task 4: Sentiment, Normalization, Relevance, And Deduplication

**Files:**
- Modify: `src/lib.rs`
- Create: `src/sentiment.rs`
- Create: `src/normalize.rs`
- Test: `src/sentiment.rs`
- Test: `src/normalize.rs`

- [ ] **Step 1: Add sentiment tests**

Create `src/sentiment.rs` with tests first:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lexicon_scores_positive_negative_and_neutral_text() {
        assert!(score_text("strong breakout boosts risk appetite").score > 0.0);
        assert!(score_text("weak downgrade negative shock").score < 0.0);
        assert_eq!(score_text("mixed data remains neutral").label, crate::domain::article::SentimentLabel::Neutral);
    }

    #[test]
    fn sentiment_is_bounded() {
        let result = score_text("strong strong strong breakout relief positive boosts");
        assert!(result.score <= 1.0);
        assert!(result.score >= -1.0);
    }
}
```

- [ ] **Step 2: Add normalization tests**

Create `src/normalize.rs` with tests first:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::{config::Stage0Config, fixture::generate_fixture};

    fn config() -> Stage0Config {
        Stage0Config::load("configs/stage0_fixture.json").unwrap()
    }

    #[test]
    fn normalization_deduplicates_by_canonical_url_and_title() {
        let config = config();
        let fixture = generate_fixture(&config).unwrap();
        let normalized = normalize_articles(&config, &fixture.raw_articles).unwrap();

        assert_eq!(fixture.raw_articles.len(), 6);
        assert_eq!(normalized.len(), 5);
        assert_eq!(normalized.iter().filter(|a| a.url == "fixture://spy-breakout").count(), 1);
    }

    #[test]
    fn normalization_maps_direct_theme_and_macro_relevance() {
        let config = config();
        let fixture = generate_fixture(&config).unwrap();
        let normalized = normalize_articles(&config, &fixture.raw_articles).unwrap();

        let spy = normalized.iter().find(|a| a.url == "fixture://spy-breakout").unwrap();
        let rates = normalized.iter().find(|a| a.url == "fixture://rates-relief").unwrap();
        let tech = normalized.iter().find(|a| a.url == "fixture://qqq-downgrade").unwrap();

        assert_eq!(spy.relevant_symbols, vec!["SPY"]);
        assert_eq!(rates.relevant_symbols, vec!["QQQ", "SPY"]);
        assert!(tech.relevant_symbols.contains(&"QQQ".to_string()));
    }

    #[test]
    fn after_hours_article_gets_deferred_available_at() {
        let config = config();
        let fixture = generate_fixture(&config).unwrap();
        let normalized = normalize_articles(&config, &fixture.raw_articles).unwrap();
        let after_close = normalized.iter().find(|a| a.url == "fixture://after-close").unwrap();

        assert_eq!(after_close.published_at.to_rfc3339(), "2026-07-02T21:15:00+00:00");
        assert_eq!(after_close.available_at.to_rfc3339(), "2026-07-06T13:30:00+00:00");
    }
}
```

- [ ] **Step 3: Run tests and verify failures for missing implementation**

Run:

```bash
cargo test lexicon_scores_positive_negative_and_neutral_text normalization_deduplicates_by_canonical_url_and_title normalization_maps_direct_theme_and_macro_relevance after_hours_article_gets_deferred_available_at
```

Expected: compile failure for missing `score_text` and `normalize_articles`.

- [ ] **Step 4: Export sentiment and normalization modules**

Modify `src/lib.rs`:

```rust
pub mod calendar;
pub mod cli;
pub mod config;
pub mod domain;
pub mod fixture;
pub mod ids;
pub mod normalize;
pub mod pipeline;
pub mod sentiment;

pub use cli::run_cli;
```

- [ ] **Step 5: Implement lexicon sentiment**

Create `src/sentiment.rs` above tests:

```rust
use crate::domain::article::SentimentLabel;

#[derive(Debug, Clone, PartialEq)]
pub struct SentimentResult {
    pub score: f64,
    pub label: SentimentLabel,
}

pub const SENTIMENT_VERSION: &str = "stage0_lexicon_v1";

pub fn score_text(text: &str) -> SentimentResult {
    let positive = ["strong", "breakout", "boosts", "relief", "positive", "constructive", "growth"];
    let negative = ["weak", "downgrade", "negative", "shock", "weighs", "surprise", "hawkish"];
    let lower = text.to_lowercase();
    let mut score = 0.0;
    for token in lower.split(|c: char| !c.is_ascii_alphabetic()) {
        if positive.contains(&token) {
            score += 1.0;
        }
        if negative.contains(&token) {
            score -= 1.0;
        }
    }
    let bounded = (score / 4.0_f64).clamp(-1.0, 1.0);
    let label = if bounded > 0.05 {
        SentimentLabel::Positive
    } else if bounded < -0.05 {
        SentimentLabel::Negative
    } else {
        SentimentLabel::Neutral
    };
    SentimentResult { score: bounded, label }
}
```

- [ ] **Step 6: Implement normalization, deduplication, and relevance mapping**

Create `src/normalize.rs` above tests:

```rust
use crate::{
    calendar::{is_regular_session, next_regular_signal_time},
    config::Stage0Config,
    domain::article::{NewsScope, NormalizedArticle, RawArticle},
    ids::stable_id,
    sentiment::score_text,
};
use anyhow::Result;
use std::collections::{BTreeMap, BTreeSet};

pub const RELEVANCE_RULE_VERSION: &str = "stage0_relevance_v1";

pub fn normalize_articles(config: &Stage0Config, raw_articles: &[RawArticle]) -> Result<Vec<NormalizedArticle>> {
    let mut by_dedupe_key: BTreeMap<String, &RawArticle> = BTreeMap::new();
    for article in raw_articles {
        by_dedupe_key.entry(dedupe_key(article)).or_insert(article);
    }

    let mut normalized = Vec::new();
    for article in by_dedupe_key.values() {
        let mut relevant_symbols = BTreeSet::new();
        for ticker in &article.tickers {
            if config.symbols.contains(ticker) {
                relevant_symbols.insert(ticker.clone());
            }
        }
        for theme in &article.themes {
            if let Some(symbols) = config.theme_symbol_map.get(theme) {
                for symbol in symbols {
                    relevant_symbols.insert(symbol.clone());
                }
            }
        }
        if relevant_symbols.is_empty() && article.tickers.is_empty() {
            for symbol in &config.macro_symbols {
                relevant_symbols.insert(symbol.clone());
            }
        }

        let scope = if !article.tickers.is_empty() {
            NewsScope::TickerSpecific
        } else if !article.themes.is_empty() {
            NewsScope::SectorTheme
        } else {
            NewsScope::MacroMarket
        };
        let combined_text = format!("{} {}", article.title, article.summary);
        let sentiment = score_text(&combined_text);
        let available_at = if is_regular_session(article.published_at, &config.holidays) {
            article.published_at
        } else {
            next_regular_signal_time(article.published_at, config.price_interval_minutes, &config.holidays)
        };
        let dedupe_key = dedupe_key(article);
        let article_id = stable_id("art", &(article.source.as_str(), article.url.as_str(), article.published_at))?;

        normalized.push(NormalizedArticle {
            article_id,
            vendor_id: article.vendor_id.clone(),
            source: article.source.clone(),
            source_kind: article.source_kind.clone(),
            published_at: article.published_at,
            available_at,
            title: article.title.clone(),
            summary: article.summary.clone(),
            url: article.url.clone(),
            tickers: article.tickers.clone(),
            themes: article.themes.clone(),
            scope,
            relevant_symbols: relevant_symbols.into_iter().collect(),
            sentiment_score: sentiment.score,
            sentiment_label: sentiment.label,
            dedupe_key,
        });
    }
    normalized.sort_by_key(|article| (article.available_at, article.article_id.clone()));
    Ok(normalized)
}

fn dedupe_key(article: &RawArticle) -> String {
    format!("{}::{}", article.url.trim().to_lowercase(), article.title.trim().to_lowercase())
}
```

- [ ] **Step 7: Run tests and commit**

Run:

```bash
cargo fmt
cargo test lexicon_scores_positive_negative_and_neutral_text sentiment_is_bounded
cargo test normalization_deduplicates_by_canonical_url_and_title normalization_maps_direct_theme_and_macro_relevance after_hours_article_gets_deferred_available_at
```

Expected: all selected tests pass.

Commit:

```bash
git add src/lib.rs src/sentiment.rs src/normalize.rs
git commit -m "feat: normalize fixture news sentiment"
```

## Task 5: JSONL, Parquet, And Manifests

**Files:**
- Modify: `src/lib.rs`
- Create: `src/storage/mod.rs`
- Create: `src/storage/jsonl.rs`
- Create: `src/storage/parquet.rs`
- Create: `src/storage/manifest.rs`
- Modify: `src/pipeline.rs`
- Test: `src/storage/jsonl.rs`
- Test: `src/storage/parquet.rs`
- Test: `src/storage/manifest.rs`
- Test: `tests/stage0_cli.rs`

- [ ] **Step 1: Add storage tests**

Create `src/storage/jsonl.rs` with tests first:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use serde::{Deserialize, Serialize};
    use tempfile::TempDir;

    #[derive(Debug, PartialEq, Serialize, Deserialize)]
    struct Row {
        id: String,
        value: i32,
    }

    #[test]
    fn jsonl_round_trips_rows() {
        let temp = TempDir::new().unwrap();
        let path = temp.path().join("rows.jsonl");
        let rows = vec![Row { id: "a".into(), value: 1 }, Row { id: "b".into(), value: 2 }];

        write_jsonl(&path, &rows).unwrap();
        let loaded: Vec<Row> = read_jsonl(&path).unwrap();

        assert_eq!(loaded, rows);
    }
}
```

Create `src/storage/parquet.rs` with tests first:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use serde::{Deserialize, Serialize};
    use tempfile::TempDir;

    #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
    struct Row {
        id: String,
        value: f64,
    }

    #[test]
    fn parquet_round_trips_rows() {
        let temp = TempDir::new().unwrap();
        let path = temp.path().join("rows.parquet");
        let rows = vec![Row { id: "a".into(), value: 1.25 }, Row { id: "b".into(), value: -0.75 }];

        write_parquet(&path, &rows).unwrap();
        let loaded: Vec<Row> = read_parquet(&path).unwrap();

        assert_eq!(loaded, rows);
    }
}
```

Create `src/storage/manifest.rs` with tests first:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    #[test]
    fn dataset_manifest_id_changes_when_file_checksum_changes() {
        let base = DatasetManifestInput {
            created_at: chrono::Utc.with_ymd_and_hms(2026, 7, 9, 0, 0, 0).unwrap(),
            schema_version: "stage0_dataset_v1".into(),
            sources: vec!["fixture".into()],
            symbols: vec!["SPY".into()],
            date_start: "2026-06-29".parse().unwrap(),
            date_end: "2026-07-07".parse().unwrap(),
            files: vec![FileManifest { relative_path: "a.parquet".into(), sha256: "abc".into(), rows: 1 }],
        };
        let changed = DatasetManifestInput {
            files: vec![FileManifest { relative_path: "a.parquet".into(), sha256: "def".into(), rows: 1 }],
            ..base.clone()
        };

        assert_ne!(dataset_id(&base).unwrap(), dataset_id(&changed).unwrap());
    }
}
```

- [ ] **Step 2: Run tests and verify storage functions are missing**

Run:

```bash
cargo test jsonl_round_trips_rows parquet_round_trips_rows dataset_manifest_id_changes_when_file_checksum_changes
```

Expected: compile failure for missing storage implementation.

- [ ] **Step 3: Export storage module**

Modify `src/lib.rs`:

```rust
pub mod calendar;
pub mod cli;
pub mod config;
pub mod domain;
pub mod fixture;
pub mod ids;
pub mod normalize;
pub mod pipeline;
pub mod sentiment;
pub mod storage;

pub use cli::run_cli;
```

Create `src/storage/mod.rs`:

```rust
pub mod jsonl;
pub mod manifest;
pub mod parquet;
```

- [ ] **Step 4: Implement JSONL helpers**

Create `src/storage/jsonl.rs` above tests:

```rust
use anyhow::{Context, Result};
use serde::{Serialize, de::DeserializeOwned};
use std::{fs::{self, File}, io::{BufRead, BufReader, Write}, path::Path};

pub fn write_jsonl<T: Serialize>(path: &Path, rows: &[T]) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut file = File::create(path).with_context(|| format!("failed to create {}", path.display()))?;
    for row in rows {
        serde_json::to_writer(&mut file, row)?;
        file.write_all(b"\n")?;
    }
    Ok(())
}

pub fn read_jsonl<T: DeserializeOwned>(path: &Path) -> Result<Vec<T>> {
    let file = File::open(path).with_context(|| format!("failed to open {}", path.display()))?;
    let mut rows = Vec::new();
    for line in BufReader::new(file).lines() {
        let line = line?;
        rows.push(serde_json::from_str(&line)?);
    }
    Ok(rows)
}
```

- [ ] **Step 5: Implement Parquet helpers**

Create `src/storage/parquet.rs` above tests:

```rust
use anyhow::{Context, Result};
use parquet::{arrow::{ArrowWriter, arrow_reader::ParquetRecordBatchReaderBuilder}, file::properties::WriterProperties};
use serde::{Serialize, de::DeserializeOwned};
use std::{fs::{self, File}, path::Path, sync::Arc};

pub fn write_parquet<T>(path: &Path, rows: &[T]) -> Result<()>
where
    T: Serialize,
{
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let fields = Vec::<serde_arrow::schema::FieldRef>::from_type::<T>(serde_arrow::schema::TracingOptions::default())?;
    let batch = serde_arrow::to_record_batch(&fields, rows)?;
    let file = File::create(path).with_context(|| format!("failed to create {}", path.display()))?;
    let props = WriterProperties::builder().build();
    let mut writer = ArrowWriter::try_new(file, batch.schema(), Some(props))?;
    writer.write(&batch)?;
    writer.close()?;
    Ok(())
}

pub fn read_parquet<T>(path: &Path) -> Result<Vec<T>>
where
    T: DeserializeOwned,
{
    let file = File::open(path).with_context(|| format!("failed to open {}", path.display()))?;
    let builder = ParquetRecordBatchReaderBuilder::try_new(file)?;
    let reader = builder.build()?;
    let mut rows = Vec::new();
    for batch in reader {
        let batch = batch?;
        let fields = Vec::<serde_arrow::schema::FieldRef>::from_arrow(&batch.schema().fields)?;
        let mut batch_rows: Vec<T> = serde_arrow::from_record_batch(&fields, &batch)?;
        rows.append(&mut batch_rows);
    }
    Ok(rows)
}
```

- [ ] **Step 6: Implement manifest models and checksum helpers**

Create `src/storage/manifest.rs` above tests:

```rust
use crate::ids::{sha256_hex, stable_id};
use anyhow::Result;
use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use std::{fs, path::Path};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileManifest {
    pub relative_path: String,
    pub sha256: String,
    pub rows: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DatasetManifestInput {
    pub created_at: DateTime<Utc>,
    pub schema_version: String,
    pub sources: Vec<String>,
    pub symbols: Vec<String>,
    pub date_start: NaiveDate,
    pub date_end: NaiveDate,
    pub files: Vec<FileManifest>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DatasetManifest {
    pub dataset_id: String,
    #[serde(flatten)]
    pub input: DatasetManifestInput,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ObservationSetManifestInput {
    pub dataset_id: String,
    pub created_at: DateTime<Utc>,
    pub schema_version: String,
    pub aggregation_config_hash: String,
    pub sentiment_version: String,
    pub relevance_rule_version: String,
    pub files: Vec<FileManifest>,
    pub news_windows_minutes: Vec<i64>,
    pub measurement_horizons_minutes: Vec<i64>,
    pub source_sets: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ObservationSetManifest {
    pub observation_set_id: String,
    #[serde(flatten)]
    pub input: ObservationSetManifestInput,
}

pub fn checksum_file(path: &Path) -> Result<String> {
    Ok(sha256_hex(&fs::read(path)?))
}

pub fn dataset_id(input: &DatasetManifestInput) -> Result<String> {
    stable_id("ds", input)
}

pub fn observation_set_id(input: &ObservationSetManifestInput) -> Result<String> {
    stable_id("obs", input)
}
```

- [ ] **Step 7: Wire fixture command to write dataset artifacts**

Modify `src/pipeline.rs` so `run_fixture` writes:

```rust
use crate::{
    config::Stage0Config,
    fixture::generate_fixture,
    normalize::normalize_articles,
    storage::{
        jsonl::write_jsonl,
        manifest::{checksum_file, dataset_id, DatasetManifest, DatasetManifestInput, FileManifest},
        parquet::write_parquet,
    },
};
use anyhow::Result;
use chrono::NaiveDate;
use serde::Serialize;
use std::{fs, path::PathBuf};

#[derive(Debug, Clone, Serialize)]
pub struct SourceCatalogRow {
    pub source: String,
    pub source_kind: String,
}

pub fn run_fixture(config: &Stage0Config, output_root: Option<PathBuf>, dry_run: bool) -> Result<()> {
    let fixture = generate_fixture(config)?;
    let normalized = normalize_articles(config, &fixture.raw_articles)?;
    let root = output_root.unwrap_or_else(|| PathBuf::from(&config.output_root));
    if dry_run {
        println!(
            "fixture run_id={} output_root={} dry_run={} articles={} normalized_articles={} price_bars={}",
            config.run_id,
            root.display(),
            dry_run,
            fixture.raw_articles.len(),
            normalized.len(),
            fixture.price_bars.len()
        );
        return Ok(());
    }

    let temp_dir = root.join("data").join("datasets").join("_building");
    let raw_path = temp_dir.join("raw").join("raw_articles.jsonl");
    let normalized_path = temp_dir.join("normalized_articles.parquet");
    let price_path = temp_dir.join("price_bars.parquet");
    let source_catalog_path = temp_dir.join("source_catalog.parquet");
    write_jsonl(&raw_path, &fixture.raw_articles)?;
    write_parquet(&normalized_path, &normalized)?;
    write_parquet(&price_path, &fixture.price_bars)?;
    let source_catalog = vec![
        SourceCatalogRow { source: "fixture_finance".into(), source_kind: "finance".into() },
        SourceCatalogRow { source: "fixture_macro".into(), source_kind: "broad".into() },
    ];
    write_parquet(&source_catalog_path, &source_catalog)?;

    let files = vec![
        FileManifest { relative_path: "raw/raw_articles.jsonl".into(), sha256: checksum_file(&raw_path)?, rows: fixture.raw_articles.len() as u64 },
        FileManifest { relative_path: "normalized_articles.parquet".into(), sha256: checksum_file(&normalized_path)?, rows: normalized.len() as u64 },
        FileManifest { relative_path: "price_bars.parquet".into(), sha256: checksum_file(&price_path)?, rows: fixture.price_bars.len() as u64 },
        FileManifest { relative_path: "source_catalog.parquet".into(), sha256: checksum_file(&source_catalog_path)?, rows: source_catalog.len() as u64 },
    ];
    let input = DatasetManifestInput {
        created_at: config.generated_at,
        schema_version: "stage0_dataset_v1".into(),
        sources: vec!["fixture".into()],
        symbols: config.symbols.clone(),
        date_start: NaiveDate::from_ymd_opt(2026, 6, 29).unwrap(),
        date_end: NaiveDate::from_ymd_opt(2026, 7, 7).unwrap(),
        files,
    };
    let id = dataset_id(&input)?;
    let final_dir = root.join("data").join("datasets").join(&id);
    if final_dir.exists() {
        fs::remove_dir_all(&temp_dir)?;
    } else {
        fs::rename(&temp_dir, &final_dir)?;
    }
    let manifest = DatasetManifest { dataset_id: id.clone(), input };
    fs::write(final_dir.join("manifest.json"), serde_json::to_string_pretty(&manifest)?)?;
    println!("dataset_id={id}");
    Ok(())
}
```

- [ ] **Step 8: Add CLI test for artifact creation**

Append to `tests/stage0_cli.rs`:

```rust
#[test]
fn fixture_command_writes_dataset_snapshot() {
    let temp = TempDir::new().unwrap();
    let mut cmd = Command::cargo_bin("markets").unwrap();
    let output = cmd.args([
        "fixture",
        "--config",
        "configs/stage0_fixture.json",
        "--output-root",
        temp.path().to_str().unwrap(),
    ])
    .assert()
    .success()
    .stdout(predicate::str::contains("dataset_id=ds_"))
    .get_output()
    .stdout
    .clone();
    let stdout = String::from_utf8(output).unwrap();
    let dataset_id = stdout.trim().strip_prefix("dataset_id=").unwrap();
    let dataset_dir = temp.path().join("data").join("datasets").join(dataset_id);

    assert!(dataset_dir.join("raw/raw_articles.jsonl").exists());
    assert!(dataset_dir.join("normalized_articles.parquet").exists());
    assert!(dataset_dir.join("price_bars.parquet").exists());
    assert!(dataset_dir.join("source_catalog.parquet").exists());
    assert!(dataset_dir.join("manifest.json").exists());
}
```

- [ ] **Step 9: Run tests and commit**

Run:

```bash
cargo fmt
cargo test jsonl_round_trips_rows parquet_round_trips_rows dataset_manifest_id_changes_when_file_checksum_changes
cargo test --test stage0_cli fixture_command_writes_dataset_snapshot
```

Expected: all selected tests pass.

Commit:

```bash
git add src/lib.rs src/storage src/pipeline.rs tests/stage0_cli.rs
git commit -m "feat: persist stage0 dataset snapshots"
```

## Task 6: Build Reusable News Signal Observations

**Files:**
- Modify: `src/lib.rs`
- Create: `src/observations.rs`
- Modify: `src/pipeline.rs`
- Modify: `src/cli.rs`
- Test: `src/observations.rs`
- Test: `tests/stage0_cli.rs`

- [ ] **Step 1: Add observation builder tests**

Create `src/observations.rs` with tests first:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::{config::Stage0Config, fixture::generate_fixture, normalize::normalize_articles};

    fn config() -> Stage0Config {
        Stage0Config::load("configs/stage0_fixture.json").unwrap()
    }

    #[test]
    fn observations_include_one_row_per_symbol_signal_window_horizon_source_set() {
        let config = config();
        let fixture = generate_fixture(&config).unwrap();
        let articles = normalize_articles(&config, &fixture.raw_articles).unwrap();
        let observations = build_observations(&config, "ds_test", &articles, &fixture.price_bars).unwrap();

        assert!(observations.iter().any(|row| row.symbol == "SPY" && row.news_window_minutes == 60 && row.source_set == "finance_only"));
        assert!(observations.iter().any(|row| row.symbol == "QQQ" && row.news_window_minutes == 240 && row.source_set == "finance_plus_broad"));
    }

    #[test]
    fn news_window_uses_available_at_and_excludes_future_articles() {
        let config = config();
        let fixture = generate_fixture(&config).unwrap();
        let articles = normalize_articles(&config, &fixture.raw_articles).unwrap();
        let observations = build_observations(&config, "ds_test", &articles, &fixture.price_bars).unwrap();
        let before_after_hours = observations.iter()
            .find(|row| row.signal_time.to_rfc3339() == "2026-07-02T19:30:00+00:00" && row.symbol == "SPY" && row.source_set == "broad_news")
            .unwrap();
        let after_holiday = observations.iter()
            .find(|row| row.signal_time.to_rfc3339() == "2026-07-06T13:30:00+00:00" && row.symbol == "SPY" && row.source_set == "broad_news")
            .unwrap();

        assert!(!before_after_hours.article_ids.iter().any(|id| after_holiday.article_ids.contains(id)));
        assert!(after_holiday.article_count > 0);
    }

    #[test]
    fn observations_measure_future_returns_after_signal_time() {
        let config = config();
        let fixture = generate_fixture(&config).unwrap();
        let articles = normalize_articles(&config, &fixture.raw_articles).unwrap();
        let observations = build_observations(&config, "ds_test", &articles, &fixture.price_bars).unwrap();
        let spy_positive = observations.iter()
            .find(|row| row.symbol == "SPY" && row.signal_time.to_rfc3339() == "2026-06-29T15:30:00+00:00" && row.source_set == "finance_only")
            .unwrap();

        assert!(spy_positive.mean_sentiment > 0.0);
        assert!(spy_positive.future_return > 0.0);
        assert_eq!(spy_positive.entry_time, spy_positive.signal_time);
        assert!(spy_positive.exit_time > spy_positive.entry_time);
    }
}
```

- [ ] **Step 2: Run tests and verify missing observation builder**

Run:

```bash
cargo test observations_include_one_row_per_symbol_signal_window_horizon_source_set news_window_uses_available_at_and_excludes_future_articles observations_measure_future_returns_after_signal_time
```

Expected: compile failure for missing `build_observations`.

- [ ] **Step 3: Export observations module**

Modify `src/lib.rs`:

```rust
pub mod calendar;
pub mod cli;
pub mod config;
pub mod domain;
pub mod fixture;
pub mod ids;
pub mod normalize;
pub mod observations;
pub mod pipeline;
pub mod sentiment;
pub mod storage;

pub use cli::run_cli;
```

- [ ] **Step 4: Implement observation building**

Create `src/observations.rs` above tests:

```rust
use crate::{
    config::Stage0Config,
    domain::{
        article::{NewsScope, NormalizedArticle, SentimentLabel, SourceKind},
        market::PriceBar,
        observation::NewsSignalObservation,
    },
    ids::stable_id,
};
use anyhow::{Result, bail};
use chrono::Duration;
use std::collections::BTreeSet;

pub fn build_observations(
    config: &Stage0Config,
    dataset_id: &str,
    articles: &[NormalizedArticle],
    price_bars: &[PriceBar],
) -> Result<Vec<NewsSignalObservation>> {
    let mut observations = Vec::new();
    for bar in price_bars {
        for news_window_minutes in &config.news_windows_minutes {
            for measurement_horizon_minutes in &config.measurement_horizons_minutes {
                for source_set in &config.source_sets {
                    if let Some(row) = build_one(
                        config,
                        dataset_id,
                        articles,
                        price_bars,
                        bar,
                        *news_window_minutes,
                        *measurement_horizon_minutes,
                        source_set,
                    )? {
                        observations.push(row);
                    }
                }
            }
        }
    }
    observations.sort_by_key(|row| (row.signal_time, row.symbol.clone(), row.news_window_minutes, row.source_set.clone()));
    Ok(observations)
}

fn build_one(
    config: &Stage0Config,
    dataset_id: &str,
    articles: &[NormalizedArticle],
    price_bars: &[PriceBar],
    signal_bar: &PriceBar,
    news_window_minutes: i64,
    measurement_horizon_minutes: i64,
    source_set: &str,
) -> Result<Option<NewsSignalObservation>> {
    let signal_time = signal_bar.start_time;
    let window_start = signal_time - Duration::minutes(news_window_minutes);
    let eligible: Vec<&NormalizedArticle> = articles.iter()
        .filter(|article| article.available_at > window_start && article.available_at <= signal_time)
        .filter(|article| article.relevant_symbols.contains(&signal_bar.symbol))
        .filter(|article| source_set_includes(source_set, article))
        .collect();
    if eligible.is_empty() {
        return Ok(None);
    }
    let exit_time = signal_time + Duration::minutes(measurement_horizon_minutes);
    let future_bar = price_bars.iter()
        .find(|bar| bar.symbol == signal_bar.symbol && bar.start_time == signal_time && bar.end_time == exit_time);
    let Some(future_bar) = future_bar else {
        return Ok(None);
    };
    let prior_bar = price_bars.iter()
        .filter(|bar| bar.symbol == signal_bar.symbol && bar.end_time <= signal_time)
        .max_by_key(|bar| bar.end_time);
    let article_count = eligible.len() as u32;
    let mean_sentiment = eligible.iter().map(|article| article.sentiment_score).sum::<f64>() / article_count as f64;
    let dispersion = eligible.iter().map(|article| (article.sentiment_score - mean_sentiment).abs()).sum::<f64>() / article_count as f64;
    let sources: BTreeSet<_> = eligible.iter().map(|article| article.source.clone()).collect();
    let article_ids: Vec<_> = eligible.iter().map(|article| article.article_id.clone()).collect();
    let price_bar_ids = vec![signal_bar.bar_id.clone(), future_bar.bar_id.clone()];
    let observation_id = stable_id("sig", &(dataset_id, signal_bar.symbol.as_str(), signal_time, news_window_minutes, measurement_horizon_minutes, source_set, &article_ids))?;
    let future_return = (future_bar.close - future_bar.open) / future_bar.open;

    Ok(Some(NewsSignalObservation {
        observation_id,
        dataset_id: dataset_id.into(),
        symbol: signal_bar.symbol.clone(),
        signal_time,
        news_window_minutes,
        measurement_horizon_minutes,
        price_interval_minutes: config.price_interval_minutes,
        source_set: source_set.into(),
        article_count,
        ticker_article_count: eligible.iter().filter(|article| article.scope == NewsScope::TickerSpecific).count() as u32,
        sector_theme_article_count: eligible.iter().filter(|article| article.scope == NewsScope::SectorTheme).count() as u32,
        macro_article_count: eligible.iter().filter(|article| article.scope == NewsScope::MacroMarket).count() as u32,
        source_count: sources.len() as u32,
        publisher_count: sources.len() as u32,
        mean_sentiment,
        weighted_sentiment: mean_sentiment,
        positive_article_count: eligible.iter().filter(|article| article.sentiment_label == SentimentLabel::Positive).count() as u32,
        negative_article_count: eligible.iter().filter(|article| article.sentiment_label == SentimentLabel::Negative).count() as u32,
        sentiment_dispersion: dispersion,
        prior_return: prior_bar.map(|bar| bar.return_pct()).unwrap_or(0.0),
        prior_volatility: prior_bar.map(|bar| bar.high / bar.low - 1.0).unwrap_or(0.0),
        market_session: "regular".into(),
        is_after_hours_signal: false,
        future_return,
        future_volatility: future_bar.high / future_bar.low - 1.0,
        future_tail_event: future_return.abs() >= 0.006,
        future_max_drawdown: (future_bar.low - future_bar.open) / future_bar.open,
        future_max_runup: (future_bar.high - future_bar.open) / future_bar.open,
        entry_time: signal_time,
        exit_time,
        article_ids,
        price_bar_ids,
        created_by_run_id: config.run_id.clone(),
    }))
}

fn source_set_includes(source_set: &str, article: &NormalizedArticle) -> bool {
    match source_set {
        "finance_only" => article.source_kind == SourceKind::Finance,
        "broad_news" => article.source_kind == SourceKind::Broad,
        "finance_plus_broad" => true,
        other => {
            eprintln!("unknown source_set={other}");
            false
        }
    }
}
```

- [ ] **Step 5: Add dataset loading and observation artifact writing**

Modify `src/pipeline.rs` with helpers:

```rust
use crate::{
    observations::build_observations,
    sentiment::SENTIMENT_VERSION,
    normalize::RELEVANCE_RULE_VERSION,
    storage::{
        manifest::{observation_set_id, ObservationSetManifest, ObservationSetManifestInput},
        parquet::read_parquet,
    },
    domain::{article::NormalizedArticle, market::PriceBar},
};

pub fn run_build_observations(
    config: &Stage0Config,
    output_root: Option<PathBuf>,
    dry_run: bool,
    dataset_id: &str,
) -> Result<()> {
    let root = output_root.unwrap_or_else(|| PathBuf::from(&config.output_root));
    let dataset_dir = root.join("data").join("datasets").join(dataset_id);
    let articles: Vec<NormalizedArticle> = read_parquet(&dataset_dir.join("normalized_articles.parquet"))?;
    let price_bars: Vec<PriceBar> = read_parquet(&dataset_dir.join("price_bars.parquet"))?;
    let observations = build_observations(config, dataset_id, &articles, &price_bars)?;
    let aggregation_config_hash = crate::ids::stable_id("agg", &(
        &config.news_windows_minutes,
        &config.measurement_horizons_minutes,
        &config.source_sets,
        config.price_interval_minutes,
    ))?;
    let temp_dir = root.join("data").join("observation_sets").join("_building");
    let observations_path = temp_dir.join("news_signal_observations.parquet");
    if dry_run {
        println!("observation_set dry_run=true rows={}", observations.len());
        return Ok(());
    }
    write_parquet(&observations_path, &observations)?;
    let file = FileManifest {
        relative_path: "news_signal_observations.parquet".into(),
        sha256: checksum_file(&observations_path)?,
        rows: observations.len() as u64,
    };
    let input = ObservationSetManifestInput {
        dataset_id: dataset_id.into(),
        created_at: config.generated_at,
        schema_version: "stage0_observations_v1".into(),
        aggregation_config_hash,
        sentiment_version: SENTIMENT_VERSION.into(),
        relevance_rule_version: RELEVANCE_RULE_VERSION.into(),
        files: vec![file],
        news_windows_minutes: config.news_windows_minutes.clone(),
        measurement_horizons_minutes: config.measurement_horizons_minutes.clone(),
        source_sets: config.source_sets.clone(),
    };
    let id = observation_set_id(&input)?;
    let final_dir = root.join("data").join("observation_sets").join(&id);
    if final_dir.exists() {
        fs::remove_dir_all(&temp_dir)?;
    } else {
        fs::rename(&temp_dir, &final_dir)?;
    }
    let manifest = ObservationSetManifest { observation_set_id: id.clone(), input };
    fs::write(final_dir.join("manifest.json"), serde_json::to_string_pretty(&manifest)?)?;
    println!("observation_set_id={id}");
    Ok(())
}
```

- [ ] **Step 6: Route the CLI command**

Modify `src/cli.rs`:

```rust
Commands::BuildObservations(args) => {
    let config = Stage0Config::load(&args.stage.config)?;
    pipeline::run_build_observations(&config, args.stage.output_root, args.stage.dry_run, &args.dataset_id)
}
```

- [ ] **Step 7: Add CLI test for observation artifacts**

Append to `tests/stage0_cli.rs`:

```rust
#[test]
fn build_observations_reuses_dataset_snapshot() {
    let temp = TempDir::new().unwrap();
    let dataset_id = run_fixture_and_extract_dataset_id(temp.path());

    let mut cmd = Command::cargo_bin("markets").unwrap();
    let output = cmd.args([
        "build-observations",
        "--config",
        "configs/stage0_fixture.json",
        "--output-root",
        temp.path().to_str().unwrap(),
        "--dataset-id",
        &dataset_id,
    ])
    .assert()
    .success()
    .stdout(predicate::str::contains("observation_set_id=obs_"))
    .get_output()
    .stdout
    .clone();
    let stdout = String::from_utf8(output).unwrap();
    let observation_set_id = stdout.trim().strip_prefix("observation_set_id=").unwrap();
    let observation_dir = temp.path().join("data").join("observation_sets").join(observation_set_id);

    assert!(observation_dir.join("news_signal_observations.parquet").exists());
    assert!(observation_dir.join("manifest.json").exists());
    assert!(temp.path().join("data").join("datasets").join(dataset_id).join("manifest.json").exists());
}

fn run_fixture_and_extract_dataset_id(root: &std::path::Path) -> String {
    let mut cmd = Command::cargo_bin("markets").unwrap();
    let output = cmd.args([
        "fixture",
        "--config",
        "configs/stage0_fixture.json",
        "--output-root",
        root.to_str().unwrap(),
    ])
    .assert()
    .success()
    .get_output()
    .stdout
    .clone();
    String::from_utf8(output).unwrap().trim().strip_prefix("dataset_id=").unwrap().to_string()
}
```

- [ ] **Step 8: Run tests and commit**

Run:

```bash
cargo fmt
cargo test observations_include_one_row_per_symbol_signal_window_horizon_source_set news_window_uses_available_at_and_excludes_future_articles observations_measure_future_returns_after_signal_time
cargo test --test stage0_cli build_observations_reuses_dataset_snapshot
```

Expected: all selected tests pass.

Commit:

```bash
git add src/lib.rs src/observations.rs src/pipeline.rs src/cli.rs tests/stage0_cli.rs
git commit -m "feat: build reusable stage0 observations"
```

## Task 7: Analysis Outputs And Baselines

**Files:**
- Modify: `src/lib.rs`
- Create: `src/analysis.rs`
- Modify: `src/pipeline.rs`
- Modify: `src/cli.rs`
- Test: `src/analysis.rs`
- Test: `tests/stage0_cli.rs`

- [ ] **Step 1: Add analysis tests**

Create `src/analysis.rs` with tests first:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::{config::Stage0Config, fixture::generate_fixture, normalize::normalize_articles, observations::build_observations};

    fn observations() -> Vec<crate::domain::observation::NewsSignalObservation> {
        let config = Stage0Config::load("configs/stage0_fixture.json").unwrap();
        let fixture = generate_fixture(&config).unwrap();
        let articles = normalize_articles(&config, &fixture.raw_articles).unwrap();
        build_observations(&config, "ds_test", &articles, &fixture.price_bars).unwrap()
    }

    #[test]
    fn coverage_counts_observations_and_articles_by_source_set_and_symbol() {
        let rows = coverage_rows(&observations());

        assert!(rows.iter().any(|row| row.symbol == "SPY" && row.source_set == "finance_only" && row.observation_count > 0));
        assert!(rows.iter().any(|row| row.symbol == "QQQ" && row.source_set == "broad_news" && row.article_count > 0));
    }

    #[test]
    fn sentiment_buckets_show_synthetic_top_minus_bottom_spread() {
        let rows = bucket_return_rows(&observations(), 3);
        let high = rows.iter().find(|row| row.bucket == "high").unwrap();
        let low = rows.iter().find(|row| row.bucket == "low").unwrap();

        assert!(high.mean_future_return > low.mean_future_return);
    }

    #[test]
    fn shuffled_baseline_is_worse_than_observed_spread() {
        let summary = analyze_observations(&observations());

        assert!(summary.observed_top_minus_bottom > summary.shuffled_top_minus_bottom);
    }
}
```

- [ ] **Step 2: Run tests and verify analysis functions are missing**

Run:

```bash
cargo test coverage_counts_observations_and_articles_by_source_set_and_symbol sentiment_buckets_show_synthetic_top_minus_bottom_spread shuffled_baseline_is_worse_than_observed_spread
```

Expected: compile failure for missing analysis functions.

- [ ] **Step 3: Export analysis module**

Modify `src/lib.rs`:

```rust
pub mod analysis;
pub mod calendar;
pub mod cli;
pub mod config;
pub mod domain;
pub mod fixture;
pub mod ids;
pub mod normalize;
pub mod observations;
pub mod pipeline;
pub mod sentiment;
pub mod storage;

pub use cli::run_cli;
```

- [ ] **Step 4: Implement analysis rows**

Create `src/analysis.rs` above tests:

```rust
use crate::domain::{observation::NewsSignalObservation, run::{BucketReturnRow, CoverageRow}};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AnalysisSummary {
    pub observed_top_minus_bottom: f64,
    pub shuffled_top_minus_bottom: f64,
    pub pearson_correlation: f64,
    pub recommendation: String,
}

pub fn coverage_rows(observations: &[NewsSignalObservation]) -> Vec<CoverageRow> {
    let mut grouped: BTreeMap<(String, String), (u32, u32)> = BTreeMap::new();
    for row in observations {
        let entry = grouped.entry((row.source_set.clone(), row.symbol.clone())).or_default();
        entry.0 += 1;
        entry.1 += row.article_count;
    }
    grouped.into_iter().map(|((source_set, symbol), (observation_count, article_count))| CoverageRow {
        source_set,
        symbol,
        observation_count,
        article_count,
    }).collect()
}

pub fn bucket_return_rows(observations: &[NewsSignalObservation], bucket_count: usize) -> Vec<BucketReturnRow> {
    let mut sorted = observations.to_vec();
    sorted.sort_by(|a, b| a.mean_sentiment.partial_cmp(&b.mean_sentiment).unwrap());
    let mut rows = Vec::new();
    for (idx, label) in ["low", "middle", "high"].iter().enumerate().take(bucket_count) {
        let start = idx * sorted.len() / bucket_count;
        let end = ((idx + 1) * sorted.len() / bucket_count).max(start + 1).min(sorted.len());
        let slice = &sorted[start..end];
        rows.push(BucketReturnRow {
            source_set: "all".into(),
            bucket: (*label).into(),
            observation_count: slice.len() as u32,
            mean_sentiment: mean(slice.iter().map(|row| row.mean_sentiment)),
            mean_future_return: mean(slice.iter().map(|row| row.future_return)),
        });
    }
    rows
}

pub fn analyze_observations(observations: &[NewsSignalObservation]) -> AnalysisSummary {
    let rows = bucket_return_rows(observations, 3);
    let low = rows.iter().find(|row| row.bucket == "low").map(|row| row.mean_future_return).unwrap_or(0.0);
    let high = rows.iter().find(|row| row.bucket == "high").map(|row| row.mean_future_return).unwrap_or(0.0);
    let shuffled = shuffled_spread(observations);
    let observed = high - low;
    let recommendation = if observed > shuffled && observed > 0.0 {
        "continue".to_string()
    } else {
        "revise".to_string()
    };
    AnalysisSummary {
        observed_top_minus_bottom: observed,
        shuffled_top_minus_bottom: shuffled,
        pearson_correlation: pearson(observations),
        recommendation,
    }
}

fn shuffled_spread(observations: &[NewsSignalObservation]) -> f64 {
    if observations.len() < 3 {
        return 0.0;
    }
    let mut shuffled = observations.to_vec();
    let sentiments: Vec<f64> = shuffled.iter().map(|row| row.mean_sentiment).collect();
    for (idx, row) in shuffled.iter_mut().enumerate() {
        row.mean_sentiment = sentiments[(idx + 1) % sentiments.len()];
    }
    let rows = bucket_return_rows(&shuffled, 3);
    let low = rows.iter().find(|row| row.bucket == "low").map(|row| row.mean_future_return).unwrap_or(0.0);
    let high = rows.iter().find(|row| row.bucket == "high").map(|row| row.mean_future_return).unwrap_or(0.0);
    high - low
}

fn pearson(observations: &[NewsSignalObservation]) -> f64 {
    let x_mean = mean(observations.iter().map(|row| row.mean_sentiment));
    let y_mean = mean(observations.iter().map(|row| row.future_return));
    let numerator: f64 = observations.iter().map(|row| (row.mean_sentiment - x_mean) * (row.future_return - y_mean)).sum();
    let x_var: f64 = observations.iter().map(|row| (row.mean_sentiment - x_mean).powi(2)).sum();
    let y_var: f64 = observations.iter().map(|row| (row.future_return - y_mean).powi(2)).sum();
    if x_var == 0.0 || y_var == 0.0 { 0.0 } else { numerator / (x_var.sqrt() * y_var.sqrt()) }
}

fn mean(values: impl Iterator<Item = f64>) -> f64 {
    let mut count = 0.0;
    let mut total = 0.0;
    for value in values {
        count += 1.0;
        total += value;
    }
    if count == 0.0 { 0.0 } else { total / count }
}
```

- [ ] **Step 5: Add CSV writing and analyze command**

Modify `src/pipeline.rs`:

```rust
use crate::{analysis::{analyze_observations, bucket_return_rows, coverage_rows}, domain::observation::NewsSignalObservation};

pub fn run_analyze(
    config: &Stage0Config,
    output_root: Option<PathBuf>,
    dry_run: bool,
    observation_set_id: &str,
) -> Result<()> {
    let root = output_root.unwrap_or_else(|| PathBuf::from(&config.output_root));
    let observation_dir = root.join("data").join("observation_sets").join(observation_set_id);
    let observations: Vec<NewsSignalObservation> = read_parquet(&observation_dir.join("news_signal_observations.parquet"))?;
    let summary = analyze_observations(&observations);
    if dry_run {
        println!("analysis dry_run=true recommendation={}", summary.recommendation);
        return Ok(());
    }
    let report_dir = root.join("runs").join(&config.run_id).join("reports");
    fs::create_dir_all(&report_dir)?;
    write_csv(&report_dir.join("coverage.csv"), &coverage_rows(&observations))?;
    write_csv(&report_dir.join("bucket_returns.csv"), &bucket_return_rows(&observations, 3))?;
    fs::write(report_dir.join("analysis_summary.json"), serde_json::to_string_pretty(&summary)?)?;
    println!("analysis_recommendation={}", summary.recommendation);
    Ok(())
}

fn write_csv<T: serde::Serialize>(path: &std::path::Path, rows: &[T]) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut writer = csv::Writer::from_path(path)?;
    for row in rows {
        writer.serialize(row)?;
    }
    writer.flush()?;
    Ok(())
}
```

- [ ] **Step 6: Route the CLI analyze command**

Modify `src/cli.rs`:

```rust
Commands::Analyze(args) => {
    let config = Stage0Config::load(&args.stage.config)?;
    pipeline::run_analyze(&config, args.stage.output_root, args.stage.dry_run, &args.observation_set_id)
}
```

- [ ] **Step 7: Add CLI test for reports**

Append to `tests/stage0_cli.rs`:

```rust
#[test]
fn analyze_writes_reusable_report_tables() {
    let temp = TempDir::new().unwrap();
    let dataset_id = run_fixture_and_extract_dataset_id(temp.path());
    let observation_set_id = run_build_observations_and_extract_observation_set_id(temp.path(), &dataset_id);

    let mut cmd = Command::cargo_bin("markets").unwrap();
    cmd.args([
        "analyze",
        "--config",
        "configs/stage0_fixture.json",
        "--output-root",
        temp.path().to_str().unwrap(),
        "--observation-set-id",
        &observation_set_id,
    ])
    .assert()
    .success()
    .stdout(predicate::str::contains("analysis_recommendation=continue"));

    let reports = temp.path().join("runs/stage0_fixture/reports");
    assert!(reports.join("coverage.csv").exists());
    assert!(reports.join("bucket_returns.csv").exists());
    assert!(reports.join("analysis_summary.json").exists());
}

fn run_build_observations_and_extract_observation_set_id(root: &std::path::Path, dataset_id: &str) -> String {
    let mut cmd = Command::cargo_bin("markets").unwrap();
    let output = cmd.args([
        "build-observations",
        "--config",
        "configs/stage0_fixture.json",
        "--output-root",
        root.to_str().unwrap(),
        "--dataset-id",
        dataset_id,
    ])
    .assert()
    .success()
    .get_output()
    .stdout
    .clone();
    String::from_utf8(output).unwrap().trim().strip_prefix("observation_set_id=").unwrap().to_string()
}
```

- [ ] **Step 8: Run tests and commit**

Run:

```bash
cargo fmt
cargo test coverage_counts_observations_and_articles_by_source_set_and_symbol sentiment_buckets_show_synthetic_top_minus_bottom_spread shuffled_baseline_is_worse_than_observed_spread
cargo test --test stage0_cli analyze_writes_reusable_report_tables
```

Expected: all selected tests pass.

Commit:

```bash
git add src/lib.rs src/analysis.rs src/pipeline.rs src/cli.rs tests/stage0_cli.rs
git commit -m "feat: analyze stage0 observation sets"
```

## Task 8: Long/Short/Flat Backtest Engine

**Files:**
- Modify: `src/lib.rs`
- Create: `src/backtest.rs`
- Modify: `src/pipeline.rs`
- Modify: `src/cli.rs`
- Test: `src/backtest.rs`
- Test: `tests/stage0_cli.rs`

- [ ] **Step 1: Add backtest tests**

Create `src/backtest.rs` with tests first:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::{config::Stage0Config, fixture::generate_fixture, normalize::normalize_articles, observations::build_observations};

    fn observations() -> Vec<crate::domain::observation::NewsSignalObservation> {
        let config = Stage0Config::load("configs/stage0_fixture.json").unwrap();
        let fixture = generate_fixture(&config).unwrap();
        let articles = normalize_articles(&config, &fixture.raw_articles).unwrap();
        build_observations(&config, "ds_test", &articles, &fixture.price_bars).unwrap()
    }

    #[test]
    fn backtest_takes_long_and_short_trades() {
        let result = run_backtest("stage0_fixture", &observations(), 0.8, 0.2, 5.0);

        assert!(result.metrics.long_count > 0);
        assert!(result.metrics.short_count > 0);
        assert_eq!(result.metrics.trade_count as usize, result.trades.len());
    }

    #[test]
    fn costs_reduce_net_returns() {
        let no_cost = run_backtest("stage0_fixture", &observations(), 0.8, 0.2, 0.0);
        let with_cost = run_backtest("stage0_fixture", &observations(), 0.8, 0.2, 10.0);

        assert!(with_cost.metrics.net_return_sum < no_cost.metrics.net_return_sum);
    }

    #[test]
    fn short_trade_profit_uses_negative_future_return() {
        let result = run_backtest("stage0_fixture", &observations(), 0.8, 0.2, 0.0);
        let profitable_short = result.trades.iter().find(|trade| trade.side == "short" && trade.gross_return > 0.0).unwrap();

        assert!(profitable_short.net_return > 0.0);
    }
}
```

- [ ] **Step 2: Run tests and verify missing backtest implementation**

Run:

```bash
cargo test backtest_takes_long_and_short_trades costs_reduce_net_returns short_trade_profit_uses_negative_future_return
```

Expected: compile failure for missing `run_backtest`.

- [ ] **Step 3: Export backtest module**

Modify `src/lib.rs`:

```rust
pub mod analysis;
pub mod backtest;
pub mod calendar;
pub mod cli;
pub mod config;
pub mod domain;
pub mod fixture;
pub mod ids;
pub mod normalize;
pub mod observations;
pub mod pipeline;
pub mod sentiment;
pub mod storage;

pub use cli::run_cli;
```

- [ ] **Step 4: Implement long/short/flat backtest**

Create `src/backtest.rs` above tests:

```rust
use crate::domain::{observation::NewsSignalObservation, run::{BacktestMetrics, Trade}};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BacktestResult {
    pub metrics: BacktestMetrics,
    pub trades: Vec<Trade>,
}

pub fn run_backtest(
    run_id: &str,
    observations: &[NewsSignalObservation],
    long_quantile: f64,
    short_quantile: f64,
    cost_bps: f64,
) -> BacktestResult {
    let long_threshold = quantile(observations.iter().map(|row| row.mean_sentiment).collect(), long_quantile);
    let short_threshold = quantile(observations.iter().map(|row| row.mean_sentiment).collect(), short_quantile);
    let mut last_exit_by_symbol = BTreeMap::<String, chrono::DateTime<chrono::Utc>>::new();
    let mut sorted = observations.to_vec();
    sorted.sort_by_key(|row| (row.entry_time, row.symbol.clone()));
    let mut trades = Vec::new();
    for row in sorted {
        if last_exit_by_symbol.get(&row.symbol).is_some_and(|last_exit| row.entry_time < *last_exit) {
            continue;
        }
        let side = if row.mean_sentiment >= long_threshold {
            "long"
        } else if row.mean_sentiment <= short_threshold {
            "short"
        } else {
            continue;
        };
        let gross_return = if side == "long" { row.future_return } else { -row.future_return };
        let net_return = gross_return - cost_bps / 10_000.0;
        last_exit_by_symbol.insert(row.symbol.clone(), row.exit_time);
        trades.push(Trade {
            run_id: run_id.into(),
            observation_id: row.observation_id,
            symbol: row.symbol,
            side: side.into(),
            signal_time: row.signal_time,
            entry_time: row.entry_time,
            exit_time: row.exit_time,
            sentiment: row.mean_sentiment,
            gross_return,
            cost_bps,
            net_return,
        });
    }
    let metrics = metrics(run_id, cost_bps, &trades);
    BacktestResult { metrics, trades }
}

fn quantile(mut values: Vec<f64>, q: f64) -> f64 {
    values.sort_by(|a, b| a.partial_cmp(b).unwrap());
    if values.is_empty() {
        return 0.0;
    }
    let idx = ((values.len() - 1) as f64 * q).round() as usize;
    values[idx]
}

fn metrics(run_id: &str, cost_bps: f64, trades: &[Trade]) -> BacktestMetrics {
    let gross_return_sum: f64 = trades.iter().map(|trade| trade.gross_return).sum();
    let net_return_sum: f64 = trades.iter().map(|trade| trade.net_return).sum();
    let wins = trades.iter().filter(|trade| trade.net_return > 0.0).count() as f64;
    let gains: f64 = trades.iter().filter(|trade| trade.net_return > 0.0).map(|trade| trade.net_return).sum();
    let losses: f64 = trades.iter().filter(|trade| trade.net_return < 0.0).map(|trade| trade.net_return.abs()).sum();
    BacktestMetrics {
        run_id: run_id.into(),
        cost_bps,
        trade_count: trades.len() as u32,
        long_count: trades.iter().filter(|trade| trade.side == "long").count() as u32,
        short_count: trades.iter().filter(|trade| trade.side == "short").count() as u32,
        gross_return_sum,
        net_return_sum,
        average_net_return: if trades.is_empty() { 0.0 } else { net_return_sum / trades.len() as f64 },
        win_rate: if trades.is_empty() { 0.0 } else { wins / trades.len() as f64 },
        profit_factor: if losses == 0.0 { gains } else { gains / losses },
        max_drawdown: max_drawdown(trades),
    }
}

fn max_drawdown(trades: &[Trade]) -> f64 {
    let mut equity = 0.0;
    let mut peak = 0.0;
    let mut worst = 0.0;
    for trade in trades {
        equity += trade.net_return;
        if equity > peak {
            peak = equity;
        }
        let drawdown = equity - peak;
        if drawdown < worst {
            worst = drawdown;
        }
    }
    worst
}
```

- [ ] **Step 5: Add backtest command outputs**

Modify `src/pipeline.rs`:

```rust
use crate::backtest::run_backtest;

pub fn run_backtest_command(
    config: &Stage0Config,
    output_root: Option<PathBuf>,
    dry_run: bool,
    observation_set_id: &str,
    cost_bps: Option<f64>,
) -> Result<()> {
    let root = output_root.unwrap_or_else(|| PathBuf::from(&config.output_root));
    let observation_dir = root.join("data").join("observation_sets").join(observation_set_id);
    let observations: Vec<NewsSignalObservation> = read_parquet(&observation_dir.join("news_signal_observations.parquet"))?;
    let cost_bps = cost_bps.unwrap_or_else(|| config.costs_bps.first().copied().unwrap_or(0.0));
    let result = run_backtest(&config.run_id, &observations, config.long_quantile, config.short_quantile, cost_bps);
    if dry_run {
        println!("backtest dry_run=true trades={} net_return_sum={}", result.metrics.trade_count, result.metrics.net_return_sum);
        return Ok(());
    }
    let report_dir = root.join("runs").join(&config.run_id).join("reports");
    fs::create_dir_all(&report_dir)?;
    write_csv(&report_dir.join("backtest_metrics.csv"), &[result.metrics])?;
    write_csv(&report_dir.join("trade_log.csv"), &result.trades)?;
    println!("backtest_trades={}", result.trades.len());
    Ok(())
}
```

- [ ] **Step 6: Route the CLI backtest command**

Modify `src/cli.rs`:

```rust
Commands::Backtest(args) => {
    let config = Stage0Config::load(&args.observation.stage.config)?;
    pipeline::run_backtest_command(
        &config,
        args.observation.stage.output_root,
        args.observation.stage.dry_run,
        &args.observation.observation_set_id,
        args.cost_bps,
    )
}
```

- [ ] **Step 7: Add CLI test for rerunning costs**

Append to `tests/stage0_cli.rs`:

```rust
#[test]
fn backtest_reruns_with_changed_cost_without_rebuilding_dataset() {
    let temp = TempDir::new().unwrap();
    let dataset_id = run_fixture_and_extract_dataset_id(temp.path());
    let observation_set_id = run_build_observations_and_extract_observation_set_id(temp.path(), &dataset_id);

    for cost in ["0", "10"] {
        let mut cmd = Command::cargo_bin("markets").unwrap();
        cmd.args([
            "backtest",
            "--config",
            "configs/stage0_fixture.json",
            "--output-root",
            temp.path().to_str().unwrap(),
            "--observation-set-id",
            &observation_set_id,
            "--cost-bps",
            cost,
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("backtest_trades="));
    }

    assert!(temp.path().join("data").join("datasets").join(dataset_id).join("manifest.json").exists());
    assert!(temp.path().join("runs/stage0_fixture/reports/backtest_metrics.csv").exists());
    assert!(temp.path().join("runs/stage0_fixture/reports/trade_log.csv").exists());
}
```

- [ ] **Step 8: Run tests and commit**

Run:

```bash
cargo fmt
cargo test backtest_takes_long_and_short_trades costs_reduce_net_returns short_trade_profit_uses_negative_future_return
cargo test --test stage0_cli backtest_reruns_with_changed_cost_without_rebuilding_dataset
```

Expected: all selected tests pass.

Commit:

```bash
git add src/lib.rs src/backtest.rs src/pipeline.rs src/cli.rs tests/stage0_cli.rs
git commit -m "feat: backtest stage0 long short rules"
```

## Task 9: Decision Report And Static Charts

**Files:**
- Modify: `src/lib.rs`
- Create: `src/report.rs`
- Modify: `src/pipeline.rs`
- Test: `src/report.rs`
- Test: `tests/stage0_cli.rs`

- [ ] **Step 1: Add report tests**

Create `src/report.rs` with tests first:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::{analysis::AnalysisSummary, domain::run::BacktestMetrics};
    use tempfile::TempDir;

    #[test]
    fn summary_markdown_contains_lineage_metrics_and_decision() {
        let temp = TempDir::new().unwrap();
        let path = temp.path().join("summary.md");
        let analysis = AnalysisSummary {
            observed_top_minus_bottom: 0.01,
            shuffled_top_minus_bottom: 0.0,
            pearson_correlation: 0.4,
            recommendation: "continue".into(),
        };
        let metrics = BacktestMetrics {
            run_id: "stage0_fixture".into(),
            cost_bps: 5.0,
            trade_count: 4,
            long_count: 2,
            short_count: 2,
            gross_return_sum: 0.03,
            net_return_sum: 0.028,
            average_net_return: 0.007,
            win_rate: 0.75,
            profit_factor: 3.0,
            max_drawdown: -0.002,
        };

        write_summary(&path, "ds_test", "obs_test", &analysis, &metrics).unwrap();
        let text = std::fs::read_to_string(path).unwrap();

        assert!(text.contains("dataset_id: ds_test"));
        assert!(text.contains("observation_set_id: obs_test"));
        assert!(text.contains("Decision: continue"));
    }

    #[test]
    fn svg_chart_files_are_written() {
        let temp = TempDir::new().unwrap();
        write_bucket_chart(&temp.path().join("bucket_returns.svg"), &[("low".into(), -0.01), ("high".into(), 0.02)]).unwrap();
        write_equity_curve(&temp.path().join("equity_curve.svg"), &[0.0, 0.01, 0.015]).unwrap();

        assert!(std::fs::read_to_string(temp.path().join("bucket_returns.svg")).unwrap().contains("<svg"));
        assert!(std::fs::read_to_string(temp.path().join("equity_curve.svg")).unwrap().contains("<svg"));
    }
}
```

- [ ] **Step 2: Run tests and verify missing report functions**

Run:

```bash
cargo test summary_markdown_contains_lineage_metrics_and_decision svg_chart_files_are_written
```

Expected: compile failure for missing report functions.

- [ ] **Step 3: Export report module**

Modify `src/lib.rs`:

```rust
pub mod analysis;
pub mod backtest;
pub mod calendar;
pub mod cli;
pub mod config;
pub mod domain;
pub mod fixture;
pub mod ids;
pub mod normalize;
pub mod observations;
pub mod pipeline;
pub mod report;
pub mod sentiment;
pub mod storage;

pub use cli::run_cli;
```

- [ ] **Step 4: Implement Markdown summary and SVG charts**

Create `src/report.rs` above tests:

```rust
use crate::{analysis::AnalysisSummary, domain::run::BacktestMetrics};
use anyhow::Result;
use plotters::prelude::*;
use std::{fs, path::Path};

pub fn write_summary(
    path: &Path,
    dataset_id: &str,
    observation_set_id: &str,
    analysis: &AnalysisSummary,
    metrics: &BacktestMetrics,
) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let text = format!(
        "# Stage 0 Research Summary\n\n\
         dataset_id: {dataset_id}\n\n\
         observation_set_id: {observation_set_id}\n\n\
         run_id: {}\n\n\
         Decision: {}\n\n\
         observed_top_minus_bottom: {:.6}\n\n\
         shuffled_top_minus_bottom: {:.6}\n\n\
         pearson_correlation: {:.6}\n\n\
         trade_count: {}\n\n\
         net_return_sum: {:.6}\n\n\
         win_rate: {:.2}\n",
        metrics.run_id,
        analysis.recommendation,
        analysis.observed_top_minus_bottom,
        analysis.shuffled_top_minus_bottom,
        analysis.pearson_correlation,
        metrics.trade_count,
        metrics.net_return_sum,
        metrics.win_rate,
    );
    fs::write(path, text)?;
    Ok(())
}

pub fn write_bucket_chart(path: &Path, rows: &[(String, f64)]) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let root = SVGBackend::new(path, (800, 420)).into_drawing_area();
    root.fill(&WHITE)?;
    let y_min = rows.iter().map(|(_, v)| *v).fold(0.0_f64, f64::min) - 0.005;
    let y_max = rows.iter().map(|(_, v)| *v).fold(0.0_f64, f64::max) + 0.005;
    let mut chart = ChartBuilder::on(&root)
        .caption("Bucket Returns", ("sans-serif", 24))
        .margin(20)
        .x_label_area_size(40)
        .y_label_area_size(60)
        .build_cartesian_2d(0..rows.len(), y_min..y_max)?;
    chart.configure_mesh().draw()?;
    chart.draw_series(rows.iter().enumerate().map(|(idx, (_, value))| {
        Rectangle::new([(idx, 0.0), (idx + 1, *value)], BLUE.mix(0.65).filled())
    }))?;
    root.present()?;
    Ok(())
}

pub fn write_equity_curve(path: &Path, equity: &[f64]) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let root = SVGBackend::new(path, (800, 420)).into_drawing_area();
    root.fill(&WHITE)?;
    let y_min = equity.iter().copied().fold(0.0_f64, f64::min) - 0.005;
    let y_max = equity.iter().copied().fold(0.0_f64, f64::max) + 0.005;
    let mut chart = ChartBuilder::on(&root)
        .caption("Equity Curve", ("sans-serif", 24))
        .margin(20)
        .x_label_area_size(40)
        .y_label_area_size(60)
        .build_cartesian_2d(0..equity.len(), y_min..y_max)?;
    chart.configure_mesh().draw()?;
    chart.draw_series(LineSeries::new(equity.iter().enumerate().map(|(idx, value)| (idx, *value)), &GREEN))?;
    root.present()?;
    Ok(())
}
```

- [ ] **Step 5: Integrate report generation into `run` orchestration**

Modify `src/pipeline.rs` by adding a complete orchestrator:

```rust
pub fn run_all(config: &Stage0Config, output_root: Option<PathBuf>, dry_run: bool) -> Result<()> {
    let root = output_root.unwrap_or_else(|| PathBuf::from(&config.output_root));
    let dataset_id = create_dataset_snapshot(config, &root, dry_run)?;
    let observation_set_id = create_observation_set(config, &root, &dataset_id, dry_run)?;
    let observations: Vec<NewsSignalObservation> = read_parquet(
        &root.join("data").join("observation_sets").join(&observation_set_id).join("news_signal_observations.parquet")
    )?;
    let analysis = analyze_observations(&observations);
    let backtest = run_backtest(&config.run_id, &observations, config.long_quantile, config.short_quantile, 5.0);
    let run_dir = root.join("runs").join(&config.run_id);
    let report_dir = run_dir.join("reports");
    let chart_dir = run_dir.join("charts");
    fs::create_dir_all(&report_dir)?;
    fs::create_dir_all(&chart_dir)?;
    write_summary(&report_dir.join("summary.md"), &dataset_id, &observation_set_id, &analysis, &backtest.metrics)?;
    write_bucket_chart(
        &chart_dir.join("bucket_returns.svg"),
        &bucket_return_rows(&observations, 3).into_iter().map(|row| (row.bucket, row.mean_future_return)).collect::<Vec<_>>(),
    )?;
    let mut equity = vec![0.0];
    for trade in &backtest.trades {
        equity.push(equity.last().copied().unwrap_or(0.0) + trade.net_return);
    }
    write_equity_curve(&chart_dir.join("equity_curve.svg"), &equity)?;
    println!("dataset_id={dataset_id}");
    println!("observation_set_id={observation_set_id}");
    println!("decision={}", analysis.recommendation);
    Ok(())
}
```

Refactor the existing `run_fixture` and `run_build_observations` internals into private helpers named `create_dataset_snapshot` and `create_observation_set` so the CLI commands and `run_all` share one implementation.

- [ ] **Step 6: Route `run` through full orchestration**

Modify `src/cli.rs`:

```rust
Commands::Run(args) => run_loaded_config(args, pipeline::run_all),
```

- [ ] **Step 7: Add end-to-end CLI test for summary and charts**

Append to `tests/stage0_cli.rs`:

```rust
#[test]
fn run_writes_summary_charts_and_decision() {
    let temp = TempDir::new().unwrap();
    let mut cmd = Command::cargo_bin("markets").unwrap();
    cmd.args([
        "run",
        "--config",
        "configs/stage0_fixture.json",
        "--output-root",
        temp.path().to_str().unwrap(),
    ])
    .assert()
    .success()
    .stdout(predicate::str::contains("dataset_id=ds_"))
    .stdout(predicate::str::contains("observation_set_id=obs_"))
    .stdout(predicate::str::contains("decision=continue"));

    let run_dir = temp.path().join("runs/stage0_fixture");
    assert!(run_dir.join("reports/summary.md").exists());
    assert!(run_dir.join("reports/coverage.csv").exists());
    assert!(run_dir.join("reports/bucket_returns.csv").exists());
    assert!(run_dir.join("reports/backtest_metrics.csv").exists());
    assert!(run_dir.join("reports/trade_log.csv").exists());
    assert!(run_dir.join("charts/bucket_returns.svg").exists());
    assert!(run_dir.join("charts/equity_curve.svg").exists());
}
```

- [ ] **Step 8: Run tests and commit**

Run:

```bash
cargo fmt
cargo test summary_markdown_contains_lineage_metrics_and_decision svg_chart_files_are_written
cargo test --test stage0_cli run_writes_summary_charts_and_decision
```

Expected: all selected tests pass.

Commit:

```bash
git add src/lib.rs src/report.rs src/pipeline.rs src/cli.rs tests/stage0_cli.rs
git commit -m "feat: report stage0 research decision"
```

## Task 10: Final Verification, Cleanup, And Demo Script

**Files:**
- Modify: `README.md` if it already exists, otherwise create `README.md`
- Modify: `project_summary.md`
- Modify: `tests/stage0_cli.rs`

- [ ] **Step 1: Add a final integration test that proves rerun semantics**

Append to `tests/stage0_cli.rs`:

```rust
#[test]
fn full_pipeline_reuses_snapshot_for_multiple_backtests() {
    let temp = TempDir::new().unwrap();
    let dataset_id = run_fixture_and_extract_dataset_id(temp.path());
    let observation_set_id = run_build_observations_and_extract_observation_set_id(temp.path(), &dataset_id);

    let dataset_manifest = temp.path().join("data").join("datasets").join(&dataset_id).join("manifest.json");
    let before = std::fs::metadata(&dataset_manifest).unwrap().modified().unwrap();

    for cost in ["0", "5", "10"] {
        let mut cmd = Command::cargo_bin("markets").unwrap();
        cmd.args([
            "backtest",
            "--config",
            "configs/stage0_fixture.json",
            "--output-root",
            temp.path().to_str().unwrap(),
            "--observation-set-id",
            &observation_set_id,
            "--cost-bps",
            cost,
        ])
        .assert()
        .success();
    }

    let after = std::fs::metadata(&dataset_manifest).unwrap().modified().unwrap();
    assert_eq!(before, after);
}
```

- [ ] **Step 2: Run the final integration test**

Run:

```bash
cargo test --test stage0_cli full_pipeline_reuses_snapshot_for_multiple_backtests
```

Expected: the test passes and the dataset manifest modification timestamp is unchanged.

- [ ] **Step 3: Add README demo commands**

Create or modify `README.md`:

```markdown
# Markets

Local research pipeline for testing whether news sentiment has predictive value for equity and ETF trades.

## Stage 0 Fixture Demo

Run the deterministic synthetic pipeline:

```bash
cargo run -- run --config configs/stage0_fixture.json
```

Generated outputs:

- `artifacts/data/datasets/<dataset_id>/`
- `artifacts/data/observation_sets/<observation_set_id>/`
- `artifacts/runs/stage0_fixture/reports/summary.md`
- `artifacts/runs/stage0_fixture/charts/`

Rerun a changed cost assumption without rebuilding the dataset:

```bash
cargo run -- backtest \
  --config configs/stage0_fixture.json \
  --observation-set-id <observation_set_id> \
  --cost-bps 10
```
```

- [ ] **Step 4: Update project summary with Stage 0 status**

Append to `project_summary.md`:

```markdown
## Stage 0 Implementation Plan

The first build is the deterministic fixture pipeline. It generates synthetic articles and price bars locally, writes immutable dataset snapshots, builds reusable `news_signal_observations`, and produces analysis/backtest reports and static charts without using external data.

This intentionally defers large-scale data engineering until the program can prove the research loop end-to-end.
```

- [ ] **Step 5: Run full verification**

Run:

```bash
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
cargo run -- run --config configs/stage0_fixture.json
```

Expected:

- `cargo fmt --check` passes.
- `cargo clippy --all-targets -- -D warnings` passes.
- `cargo test` passes.
- `cargo run -- run --config configs/stage0_fixture.json` prints `dataset_id=ds_`, `observation_set_id=obs_`, and `decision=continue`.

- [ ] **Step 6: Inspect generated outputs**

Run:

```bash
find artifacts/runs/stage0_fixture -maxdepth 3 -type f | sort
```

Expected output includes:

```text
artifacts/runs/stage0_fixture/charts/bucket_returns.svg
artifacts/runs/stage0_fixture/charts/equity_curve.svg
artifacts/runs/stage0_fixture/reports/backtest_metrics.csv
artifacts/runs/stage0_fixture/reports/bucket_returns.csv
artifacts/runs/stage0_fixture/reports/coverage.csv
artifacts/runs/stage0_fixture/reports/summary.md
artifacts/runs/stage0_fixture/reports/trade_log.csv
```

- [ ] **Step 7: Commit**

Run:

```bash
git add README.md project_summary.md tests/stage0_cli.rs
git commit -m "docs: document stage0 fixture demo"
```

## Implementation Notes

- Prefer implementing the public tests exactly as written first, then making the minimal implementation pass.
- Keep `pipeline.rs` thin by moving calculations into `fixture.rs`, `normalize.rs`, `observations.rs`, `analysis.rs`, `backtest.rs`, and `report.rs`.
- Do not add vendor API clients in Stage 0.
- Do not tune the fixture to hide failing edge cases. The fixture should include duplicate articles, after-hours news, macro/theme relevance, direct ticker relevance, positive sentiment, negative sentiment, neutral sentiment, long trades, and short trades.
- If `serde_arrow` type tracing has trouble with `Vec<String>` fields in Parquet, encode vector fields as JSON strings in the Parquet-facing record structs and keep the domain structs unchanged.
- If `plotters` feature names differ under the resolved lockfile, keep SVG output but adjust features to the smallest set that compiles.

## Self-Review Notes

Spec coverage:

- Stage 0 fixture generation is covered by Tasks 3 and 5.
- Dataset lineage with `dataset_id`, `observation_set_id`, and `run_id` is covered by Tasks 5, 6, 8, and 9.
- `news_signal_observations` rows are covered by Task 6.
- Long/short/flat and cost-sensitive backtests are covered by Task 8.
- Decision demo outputs, charts, and reruns are covered by Tasks 7, 9, and 10.
- External free data sources are intentionally excluded because this plan implements Stage 0 only.

Type consistency:

- Article records use `published_at` and `available_at`.
- Observation rows use `news_window_minutes`, `measurement_horizon_minutes`, `price_interval_minutes`, `source_set`, and `signal_time`.
- Backtests consume observation sets and do not regenerate datasets.

Known implementation risks:

- The Parquet helper may need a small adapter if nested vector fields are not supported by the selected `serde_arrow` path.
- `plotters` SVG features may require a lockfile adjustment depending on resolved transitive features.
- The fixture is intentionally small, so Stage 0 proves program behavior and leakage controls, not market signal robustness.
