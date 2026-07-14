use crate::{
    analysis::{analyze_observations, bucket_return_rows, coverage_rows},
    backtest::run_backtests_by_configuration,
    config::Stage0Config,
    domain::{
        article::{NormalizedArticle, SourceKind},
        market::PriceBar,
        observation::NewsSignalObservation,
    },
    fixture::generate_fixture,
    normalize::{RELEVANCE_RULE_VERSION, normalize_articles},
    observations::build_observations,
    report::{write_bucket_chart, write_equity_curve, write_summary},
    sentiment::SENTIMENT_VERSION,
    storage::{
        jsonl::write_jsonl,
        manifest::{
            DatasetManifest, DatasetManifestInput, FileManifest, ObservationSetManifest,
            ObservationSetManifestInput, checksum_file, dataset_id, observation_set_id,
        },
        parquet::{read_parquet, write_parquet},
    },
};
use anyhow::{Context, Result};
use chrono::NaiveDate;
use serde::Serialize;
use std::{
    collections::BTreeSet,
    fs,
    path::{Path, PathBuf},
};

#[derive(Debug, Clone, Serialize)]
pub struct SourceCatalogRow {
    pub source: String,
    pub source_kind: String,
}

pub fn run_fixture(
    config: &Stage0Config,
    output_root: Option<PathBuf>,
    dry_run: bool,
) -> Result<()> {
    let root = output_root.unwrap_or_else(|| PathBuf::from(&config.output_root));
    let dataset_id = create_dataset_snapshot(config, &root, dry_run)?;
    if !dry_run {
        println!("dataset_id={dataset_id}");
    }
    Ok(())
}

/// Generates the fixture dataset snapshot and returns its `dataset_id`. In
/// dry-run mode it prints the preview line and returns an empty id (the caller
/// does not use it). `run_all` only ever calls this with `dry_run = false`
/// after its own dry-run guard has returned.
fn create_dataset_snapshot(config: &Stage0Config, root: &Path, dry_run: bool) -> Result<String> {
    let fixture = generate_fixture(config)?;
    let normalized = normalize_articles(config, &fixture.raw_articles)?;
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
        return Ok(String::new());
    }

    let temp_dir = root.join("data").join("datasets").join("_building");
    let raw_path = temp_dir.join("raw").join("raw_articles.jsonl");
    let normalized_path = temp_dir.join("normalized_articles.parquet");
    let price_path = temp_dir.join("price_bars.parquet");
    let source_catalog_path = temp_dir.join("source_catalog.parquet");
    write_jsonl(&raw_path, &fixture.raw_articles)?;
    write_parquet(&normalized_path, &normalized)?;
    write_parquet(&price_path, &fixture.price_bars)?;
    // Derived from the fixture's actual raw articles instead of a hardcoded
    // list, so source_catalog.parquet always reports every distinct source
    // present in raw_articles.jsonl (fixture_finance, fixture_wire,
    // fixture_macro, fixture_housing), not just a stale subset.
    let mut seen_sources = BTreeSet::new();
    let source_catalog: Vec<SourceCatalogRow> = fixture
        .raw_articles
        .iter()
        .filter(|article| seen_sources.insert(article.source.clone()))
        .map(|article| SourceCatalogRow {
            source: article.source.clone(),
            source_kind: match article.source_kind {
                SourceKind::Finance => "finance".to_string(),
                SourceKind::Broad => "broad".to_string(),
            },
        })
        .collect();
    write_parquet(&source_catalog_path, &source_catalog)?;

    let files = vec![
        FileManifest {
            relative_path: "raw/raw_articles.jsonl".into(),
            sha256: checksum_file(&raw_path)?,
            rows: fixture.raw_articles.len() as u64,
        },
        FileManifest {
            relative_path: "normalized_articles.parquet".into(),
            sha256: checksum_file(&normalized_path)?,
            rows: normalized.len() as u64,
        },
        FileManifest {
            relative_path: "price_bars.parquet".into(),
            sha256: checksum_file(&price_path)?,
            rows: fixture.price_bars.len() as u64,
        },
        FileManifest {
            relative_path: "source_catalog.parquet".into(),
            sha256: checksum_file(&source_catalog_path)?,
            rows: source_catalog.len() as u64,
        },
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
    let manifest = DatasetManifest {
        dataset_id: id.clone(),
        input,
    };
    fs::write(
        final_dir.join("manifest.json"),
        serde_json::to_string_pretty(&manifest)?,
    )?;
    Ok(id)
}

pub fn run_build_observations(
    config: &Stage0Config,
    output_root: Option<PathBuf>,
    dry_run: bool,
    dataset_id: &str,
) -> Result<()> {
    let root = output_root.unwrap_or_else(|| PathBuf::from(&config.output_root));
    let observation_set_id = create_observation_set(config, &root, dataset_id, dry_run)?;
    if !dry_run {
        println!("observation_set_id={observation_set_id}");
    }
    Ok(())
}

/// Builds the observation set from a stored dataset snapshot and returns its
/// `observation_set_id`. Dry-run prints the preview line and returns an empty
/// id (unused by the caller). `run_all` only calls this with `dry_run = false`.
fn create_observation_set(
    config: &Stage0Config,
    root: &Path,
    dataset_id: &str,
    dry_run: bool,
) -> Result<String> {
    let dataset_dir = root.join("data").join("datasets").join(dataset_id);
    let articles: Vec<NormalizedArticle> =
        read_parquet(&dataset_dir.join("normalized_articles.parquet"))?;
    let price_bars: Vec<PriceBar> = read_parquet(&dataset_dir.join("price_bars.parquet"))?;
    let observations = build_observations(config, dataset_id, &articles, &price_bars)?;
    let aggregation_config_hash = crate::ids::stable_id(
        "agg",
        &(
            &config.news_windows_minutes,
            &config.measurement_horizons_minutes,
            &config.source_sets,
            config.price_interval_minutes,
        ),
    )?;
    let temp_dir = root.join("data").join("observation_sets").join("_building");
    let observations_path = temp_dir.join("news_signal_observations.parquet");
    if dry_run {
        println!("observation_set dry_run=true rows={}", observations.len());
        return Ok(String::new());
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
    let manifest = ObservationSetManifest {
        observation_set_id: id.clone(),
        input,
    };
    fs::write(
        final_dir.join("manifest.json"),
        serde_json::to_string_pretty(&manifest)?,
    )?;
    Ok(id)
}

pub fn run_analyze(
    config: &Stage0Config,
    output_root: Option<PathBuf>,
    dry_run: bool,
    observation_set_id: &str,
    run_id: &str,
) -> Result<()> {
    let root = output_root.unwrap_or_else(|| PathBuf::from(&config.output_root));
    let observation_dir = root
        .join("data")
        .join("observation_sets")
        .join(observation_set_id);
    let observations: Vec<NewsSignalObservation> =
        read_parquet(&observation_dir.join("news_signal_observations.parquet"))?;
    let summaries = analyze_observations(&observations);
    let continue_count = summaries
        .iter()
        .filter(|summary| summary.recommendation == "continue")
        .count();
    if dry_run {
        println!(
            "analysis dry_run=true configurations={} continue={}",
            summaries.len(),
            continue_count
        );
        return Ok(());
    }
    let report_dir = root.join("runs").join(run_id).join("reports");
    fs::create_dir_all(&report_dir)?;
    write_csv(
        &report_dir.join("coverage.csv"),
        &coverage_rows(&observations),
    )?;
    write_csv(
        &report_dir.join("bucket_returns.csv"),
        &bucket_return_rows(&observations, 3),
    )?;
    fs::write(
        report_dir.join("analysis_summary.json"),
        serde_json::to_string_pretty(&summaries)?,
    )?;
    write_run_manifests(config, &root, run_id, observation_set_id)?;
    println!(
        "analysis_configurations={} analysis_continue={}",
        summaries.len(),
        continue_count
    );
    Ok(())
}

fn write_csv<T: serde::Serialize>(path: &Path, rows: &[T]) -> Result<()> {
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

/// Writes `runs/<run_id>/config.json`, `dataset_manifest.json`, and
/// `observation_set_manifest.json`, per the spec's Stored Data section
/// ("Each run lives under runs/<run_id>/: config.json, dataset_manifest.json,
/// observation_set_manifest.json, reports/..."). Shared by `run_analyze`
/// (this task), `run_backtest_command` (Task 8), and `run_all` (Task 9) —
/// all three write into `runs/<run_id>/` and must keep these files current.
/// `config.json` records the *effective* `run_id` actually used for this
/// invocation (which may differ from `config.run_id` when the caller passed
/// `--run-id`, see Task 1 Step 8's CLI changes), not the raw value loaded
/// from the config file, so the file on disk matches the directory it lives
/// in.
fn write_run_manifests(
    config: &Stage0Config,
    root: &Path,
    run_id: &str,
    observation_set_id: &str,
) -> Result<()> {
    let observation_dir = root
        .join("data")
        .join("observation_sets")
        .join(observation_set_id);
    let observation_manifest_bytes =
        fs::read(observation_dir.join("manifest.json")).with_context(|| {
            format!("failed to read observation set manifest for {observation_set_id}")
        })?;
    let observation_manifest: crate::storage::manifest::ObservationSetManifest =
        serde_json::from_slice(&observation_manifest_bytes)?;
    let dataset_dir = root
        .join("data")
        .join("datasets")
        .join(&observation_manifest.input.dataset_id);
    let dataset_manifest_bytes =
        fs::read(dataset_dir.join("manifest.json")).with_context(|| {
            format!(
                "failed to read dataset manifest for {}",
                observation_manifest.input.dataset_id
            )
        })?;

    let run_dir = root.join("runs").join(run_id);
    fs::create_dir_all(&run_dir)?;
    let mut config_snapshot = config.clone();
    config_snapshot.run_id = run_id.to_string();
    fs::write(
        run_dir.join("config.json"),
        serde_json::to_string_pretty(&config_snapshot)?,
    )?;
    fs::write(
        run_dir.join("dataset_manifest.json"),
        &dataset_manifest_bytes,
    )?;
    fs::write(
        run_dir.join("observation_set_manifest.json"),
        &observation_manifest_bytes,
    )?;
    Ok(())
}

pub fn run_backtest_command(
    config: &Stage0Config,
    output_root: Option<PathBuf>,
    dry_run: bool,
    observation_set_id: &str,
    cost_bps: Option<f64>,
    run_id: &str,
) -> Result<()> {
    let root = output_root.unwrap_or_else(|| PathBuf::from(&config.output_root));
    let observation_dir = root
        .join("data")
        .join("observation_sets")
        .join(observation_set_id);
    let observations: Vec<NewsSignalObservation> =
        read_parquet(&observation_dir.join("news_signal_observations.parquet"))?;
    let cost_bps = cost_bps.unwrap_or_else(|| config.costs_bps.first().copied().unwrap_or(0.0));
    let results = run_backtests_by_configuration(
        run_id,
        &observations,
        config.long_quantile,
        config.short_quantile,
        cost_bps,
    );
    let total_trades: usize = results.iter().map(|result| result.trades.len()).sum();
    if dry_run {
        let net_return_sum: f64 = results
            .iter()
            .map(|result| result.metrics.net_return_sum)
            .sum();
        println!(
            "backtest dry_run=true configurations={} trades={} net_return_sum={}",
            results.len(),
            total_trades,
            net_return_sum
        );
        return Ok(());
    }
    let report_dir = root.join("runs").join(run_id).join("reports");
    fs::create_dir_all(&report_dir)?;
    let metrics: Vec<_> = results
        .iter()
        .map(|result| result.metrics.clone())
        .collect();
    let trades: Vec<_> = results
        .iter()
        .flat_map(|result| result.trades.clone())
        .collect();
    write_csv(&report_dir.join("backtest_metrics.csv"), &metrics)?;
    write_csv(&report_dir.join("trade_log.csv"), &trades)?;
    write_run_manifests(config, &root, run_id, observation_set_id)?;
    println!(
        "backtest_configurations={} backtest_trades={}",
        results.len(),
        total_trades
    );
    Ok(())
}

pub fn run_all(
    config: &Stage0Config,
    output_root: Option<PathBuf>,
    dry_run: bool,
    run_id: &str,
) -> Result<()> {
    let root = output_root.unwrap_or_else(|| PathBuf::from(&config.output_root));
    // `dry_run` short-circuits before any snapshot/observation work, matching
    // the `run_accepts_checked_in_stage0_config` CLI test added in Task 1
    // (which asserts stdout contains the run_id and "dry_run=true"). This is
    // required because create_dataset_snapshot/create_observation_set only
    // compute a real dataset_id/observation_set_id after writing files and
    // hashing them — there is no meaningful id to produce in dry-run mode.
    if dry_run {
        println!(
            "run run_id={run_id} output_root={} dry_run=true",
            root.display()
        );
        return Ok(());
    }
    let dataset_id = create_dataset_snapshot(config, &root, false)?;
    let observation_set_id = create_observation_set(config, &root, &dataset_id, false)?;
    let observations: Vec<NewsSignalObservation> = read_parquet(
        &root
            .join("data")
            .join("observation_sets")
            .join(&observation_set_id)
            .join("news_signal_observations.parquet"),
    )?;
    let analyses = analyze_observations(&observations);
    let cost_bps = config.costs_bps.first().copied().unwrap_or(0.0);
    let backtests = run_backtests_by_configuration(
        run_id,
        &observations,
        config.long_quantile,
        config.short_quantile,
        cost_bps,
    );
    let metrics: Vec<_> = backtests
        .iter()
        .map(|result| result.metrics.clone())
        .collect();
    let trades: Vec<_> = backtests
        .iter()
        .flat_map(|result| result.trades.clone())
        .collect();
    let run_dir = root.join("runs").join(run_id);
    let report_dir = run_dir.join("reports");
    let chart_dir = run_dir.join("charts");
    fs::create_dir_all(&report_dir)?;
    fs::create_dir_all(&chart_dir)?;
    // run_all must write the same reports/*.csv files the standalone
    // `analyze` and `backtest` commands write (Tasks 7-8) — the Step 7 CLI
    // test below and Task 10 Step 6's expected file listing both assert
    // these exist after `run`, not just summary.md and the two charts.
    write_csv(
        &report_dir.join("coverage.csv"),
        &coverage_rows(&observations),
    )?;
    write_csv(
        &report_dir.join("bucket_returns.csv"),
        &bucket_return_rows(&observations, 3),
    )?;
    write_csv(&report_dir.join("backtest_metrics.csv"), &metrics)?;
    write_csv(&report_dir.join("trade_log.csv"), &trades)?;
    write_summary(
        &report_dir.join("summary.md"),
        &dataset_id,
        &observation_set_id,
        &analyses,
        &metrics,
    )?;
    // Same runs/<run_id>/{config.json,dataset_manifest.json,
    // observation_set_manifest.json} the standalone `analyze` and `backtest`
    // commands write (Task 7/8's write_run_manifests) — required by the
    // spec's Stored Data section and asserted by Task 10 Step 6's expected
    // file listing.
    write_run_manifests(config, &root, run_id, &observation_set_id)?;

    // Charts illustrate one representative configuration for the Stage 0 demo.
    // The summary table above, not the chart, is the source of truth across
    // all configurations (design.md Decision 1).
    let primary_key = (
        config.news_windows_minutes[0],
        config.measurement_horizons_minutes[0],
        config.source_sets[0].clone(),
    );
    let primary_bucket_rows: Vec<_> = bucket_return_rows(&observations, 3)
        .into_iter()
        .filter(|row| {
            (
                row.news_window_minutes,
                row.measurement_horizon_minutes,
                row.source_set.clone(),
            ) == primary_key
        })
        .map(|row| (row.bucket, row.mean_future_return))
        .collect();
    write_bucket_chart(&chart_dir.join("bucket_returns.svg"), &primary_bucket_rows)?;

    let primary_trades = backtests
        .iter()
        .find(|result| {
            (
                result.metrics.news_window_minutes,
                result.metrics.measurement_horizon_minutes,
                result.metrics.source_set.clone(),
            ) == primary_key
        })
        .map(|result| result.trades.as_slice())
        .unwrap_or(&[]);
    let mut equity = vec![0.0];
    for trade in primary_trades {
        equity.push(equity.last().copied().unwrap_or(0.0) + trade.net_return);
    }
    write_equity_curve(&chart_dir.join("equity_curve.svg"), &equity)?;

    let continue_count = analyses
        .iter()
        .filter(|analysis| analysis.recommendation == "continue")
        .count();
    println!("dataset_id={dataset_id}");
    println!("observation_set_id={observation_set_id}");
    println!("configurations={}", analyses.len());
    println!(
        "decisions_continue={continue_count} decisions_revise={}",
        analyses.len() - continue_count
    );
    Ok(())
}
