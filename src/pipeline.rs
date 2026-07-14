use crate::{
    analysis::{AnalysisContext, analyze_observations, bucket_return_rows, coverage_rows},
    audit::{assert_no_lookahead, timestamp_audit_rows},
    backtest::{BacktestParams, run_backtests_by_configuration, strategy_comparison},
    config::PipelineConfig,
    domain::{
        article::{Disposition, NormalizedArticle, SetAsideArticle},
        market::PriceBar,
        observation::NewsSignalObservation,
    },
    normalize::{RELEVANCE_RULE_VERSION, normalize_articles},
    observations::build_observations,
    report::{write_bucket_chart, write_equity_curve, write_summary},
    sentiment::{SENTIMENT_VERSION, has_lexicon_hit},
    source::{SourceCatalogRow, catalog_from_articles, news_source, price_source},
    storage::{
        jsonl::write_jsonl,
        manifest::{
            DatasetManifest, DatasetManifestInput, FileManifest, ObservationSetManifest,
            ObservationSetManifestInput, checksum_file, dataset_id, observation_set_id,
        },
        parquet::{read_parquet, write_parquet},
    },
};
use anyhow::{Context, Result, bail};
use chrono::NaiveDate;
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

pub fn run_ingest(
    config: &PipelineConfig,
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

/// Builds a dataset snapshot from whichever sources the config selects and
/// returns its `dataset_id`. In dry-run mode it prints the preview line and
/// returns an empty id (the caller does not use it). `run_all` only ever calls
/// this with `dry_run = false` after its own dry-run guard has returned.
fn create_dataset_snapshot(config: &PipelineConfig, root: &Path, dry_run: bool) -> Result<String> {
    let news = news_source(config)?;
    let prices = price_source(config)?;
    let raw_articles = news.fetch_raw_articles(config)?;
    let price_bars = prices.fetch_price_bars(config)?;
    let outcome = normalize_articles(config, &raw_articles)?;
    let normalized = outcome.normalized;
    let set_aside = outcome.set_aside;
    let quarantined = count_disposition(&set_aside, Disposition::Quarantined);
    let excluded = count_disposition(&set_aside, Disposition::Excluded);
    if dry_run {
        println!(
            "ingest run_id={} output_root={} dry_run={} articles={} normalized_articles={} quarantined={} excluded={} price_bars={}",
            config.run_id,
            root.display(),
            dry_run,
            raw_articles.len(),
            normalized.len(),
            quarantined,
            excluded,
            price_bars.len()
        );
        return Ok(String::new());
    }

    let temp_dir = root.join("data").join("datasets").join("_building");
    let raw_path = temp_dir.join("raw").join("raw_articles.jsonl");
    let normalized_path = temp_dir.join("normalized_articles.parquet");
    let set_aside_path = temp_dir.join("set_aside_articles.parquet");
    let price_path = temp_dir.join("price_bars.parquet");
    let source_catalog_path = temp_dir.join("source_catalog.parquet");
    write_jsonl(&raw_path, &raw_articles)?;
    write_parquet(&normalized_path, &normalized)?;
    write_parquet(&set_aside_path, &set_aside)?;
    write_parquet(&price_path, &price_bars)?;
    let source_catalog = catalog_from_articles(&raw_articles);
    write_parquet(&source_catalog_path, &source_catalog)?;

    let files = vec![
        FileManifest {
            relative_path: "raw/raw_articles.jsonl".into(),
            sha256: checksum_file(&raw_path)?,
            rows: raw_articles.len() as u64,
        },
        FileManifest {
            relative_path: "normalized_articles.parquet".into(),
            sha256: checksum_file(&normalized_path)?,
            rows: normalized.len() as u64,
        },
        FileManifest {
            relative_path: "set_aside_articles.parquet".into(),
            sha256: checksum_file(&set_aside_path)?,
            rows: set_aside.len() as u64,
        },
        FileManifest {
            relative_path: "price_bars.parquet".into(),
            sha256: checksum_file(&price_path)?,
            rows: price_bars.len() as u64,
        },
        FileManifest {
            relative_path: "source_catalog.parquet".into(),
            sha256: checksum_file(&source_catalog_path)?,
            rows: source_catalog.len() as u64,
        },
    ];
    // Vendor identity comes from the adapters, so a snapshot always names the
    // sources that actually produced it.
    let mut sources = news.vendor_names();
    for name in prices.vendor_names() {
        if !sources.contains(&name) {
            sources.push(name);
        }
    }
    let (date_start, date_end) = derive_date_range(&normalized, &price_bars)?;
    let input = DatasetManifestInput {
        created_at: config.generated_at,
        schema_version: "stage0_dataset_v1".into(),
        sources,
        symbols: config.symbols.clone(),
        date_start,
        date_end,
        quarantined_articles: quarantined as u64,
        excluded_articles: excluded as u64,
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

/// Assembles the dataset-wide facts the verdict needs but the observations do
/// not carry: how much of the news was broken, and how much of it the sentiment
/// scorer could actually read.
///
/// These are precisely the inputs a synthetic fixture cannot produce, which is
/// why `stop`/`expand data`/`expand sources` were unverifiable before Stage 1
/// (design.md Decisions 6 and 14).
fn build_analysis_context(
    config: &PipelineConfig,
    root: &Path,
    dataset_id: &str,
    observations: &[NewsSignalObservation],
) -> Result<AnalysisContext> {
    // Run every strategy — sentiment and its four baselines — through the same
    // engine BEFORE analysis, so the verdict can apply the spec's baseline gate:
    // "stop or revise when sentiment performs no better than shuffled or
    // non-sentiment baselines". Without this, `continue` means only "beat one
    // weak baseline", which is not a finding.
    let cost_bps = config.costs_bps.first().copied().unwrap_or(0.0);
    let all_strategies = run_backtests_by_configuration(
        &config.run_id,
        observations,
        BacktestParams {
            long_quantile: config.long_quantile,
            short_quantile: config.short_quantile,
            cost_bps,
            max_modal_share: config.max_modal_share,
            seed: config.seed,
            development_fraction: config.development_fraction,
        },
    );
    let strategy_nets = strategy_comparison(&all_strategies);

    let dataset_dir = root.join("data").join("datasets").join(dataset_id);
    let manifest: DatasetManifest =
        serde_json::from_slice(&fs::read(dataset_dir.join("manifest.json"))?)?;
    let raw_rows = manifest
        .input
        .files
        .iter()
        .find(|file| file.relative_path == "raw/raw_articles.jsonl")
        .map(|file| file.rows)
        .unwrap_or(0);
    // Quarantined only. Excluded articles are out of scope, not broken, and
    // must never inflate this rate — otherwise a sample boundary would trip the
    // `stop` gate.
    let quarantine_rate = if raw_rows == 0 {
        0.0
    } else {
        manifest.input.quarantined_articles as f64 / raw_rows as f64
    };

    let articles: Vec<NormalizedArticle> =
        read_parquet(&dataset_dir.join("normalized_articles.parquet"))?;
    let lexicon_hit_rate = if articles.is_empty() {
        0.0
    } else {
        articles
            .iter()
            .filter(|article| has_lexicon_hit(&format!("{} {}", article.title, article.summary)))
            .count() as f64
            / articles.len() as f64
    };

    let catalog: Vec<SourceCatalogRow> = read_parquet(&dataset_dir.join("source_catalog.parquet"))?;
    let finance = catalog
        .iter()
        .filter(|row| row.source_kind == "finance")
        .count();
    let broad = catalog
        .iter()
        .filter(|row| row.source_kind == "broad")
        .count();
    let expected_sources = BTreeMap::from([
        ("finance_only".to_string(), finance),
        ("broad_news".to_string(), broad),
        ("finance_plus_broad".to_string(), finance + broad),
    ]);

    let article_sources = articles
        .iter()
        .map(|article| (article.article_id.clone(), article.source.clone()))
        .collect();

    Ok(AnalysisContext {
        quarantine_rate,
        lexicon_hit_rate,
        expected_sources,
        article_sources,
        strategy_nets,
        long_quantile: config.long_quantile,
        short_quantile: config.short_quantile,
        max_modal_share: config.max_modal_share,
        seed: config.seed,
        development_fraction: config.development_fraction,
        thresholds: config.verdict_thresholds.clone(),
    })
}

/// Writes the Stage 1 inspection artifacts and enforces the leakage guarantee.
///
/// This runs on every analyze/backtest/run — not just in tests — because
/// design.md Decision 4's no-lookahead promise is only worth something if it is
/// checked against the data actually in front of us. A violation fails the run:
/// a leakage bug that still prints a report is worse than no report, because
/// somebody will believe the report.
fn write_audit_reports(
    root: &Path,
    run_id: &str,
    dataset_id: &str,
    observations: &[NewsSignalObservation],
) -> Result<()> {
    let dataset_dir = root.join("data").join("datasets").join(dataset_id);
    let articles: Vec<NormalizedArticle> =
        read_parquet(&dataset_dir.join("normalized_articles.parquet"))?;
    let set_aside: Vec<SetAsideArticle> =
        read_parquet(&dataset_dir.join("set_aside_articles.parquet"))?;

    assert_no_lookahead(&articles, observations)?;

    let report_dir = root.join("runs").join(run_id).join("reports");
    fs::create_dir_all(&report_dir)?;
    write_csv(
        &report_dir.join("timestamp_audit.csv"),
        &timestamp_audit_rows(&articles, observations),
    )?;
    // Already sorted by (disposition, reason, vendor_id) in normalize, so a
    // human can scan a whole failure class at once and a quality problem never
    // hides inside a pile of scope exclusions.
    write_csv(&report_dir.join("set_aside.csv"), &set_aside)?;
    Ok(())
}

fn count_disposition(set_aside: &[SetAsideArticle], disposition: Disposition) -> usize {
    set_aside
        .iter()
        .filter(|row| row.disposition == disposition.as_str())
        .count()
}

/// The snapshot's true date bounds: the first and last UTC day on which it
/// actually holds data. An empty snapshot is an error rather than a manifest
/// claiming a range it does not cover.
fn derive_date_range(
    normalized: &[NormalizedArticle],
    price_bars: &[PriceBar],
) -> Result<(NaiveDate, NaiveDate)> {
    let dates = normalized
        .iter()
        .map(|article| article.published_at.date_naive())
        .chain(price_bars.iter().map(|bar| bar.start_time.date_naive()));
    let mut start: Option<NaiveDate> = None;
    let mut end: Option<NaiveDate> = None;
    for date in dates {
        start = Some(start.map_or(date, |current: NaiveDate| current.min(date)));
        end = Some(end.map_or(date, |current: NaiveDate| current.max(date)));
    }
    match (start, end) {
        (Some(start), Some(end)) => Ok((start, end)),
        _ => bail!(
            "snapshot contains no articles and no price bars; refusing to date an empty dataset"
        ),
    }
}

pub fn run_build_observations(
    config: &PipelineConfig,
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
    config: &PipelineConfig,
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
    config: &PipelineConfig,
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
    let observation_manifest: ObservationSetManifest =
        serde_json::from_slice(&fs::read(observation_dir.join("manifest.json"))?)?;
    let context = build_analysis_context(
        config,
        &root,
        &observation_manifest.input.dataset_id,
        &observations,
    )?;
    let summaries = analyze_observations(&observations, &context);
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
    write_audit_reports(
        &root,
        run_id,
        &observation_manifest.input.dataset_id,
        &observations,
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
    config: &PipelineConfig,
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
    config: &PipelineConfig,
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
        BacktestParams {
            long_quantile: config.long_quantile,
            short_quantile: config.short_quantile,
            cost_bps,
            max_modal_share: config.max_modal_share,
            seed: config.seed,
            development_fraction: config.development_fraction,
        },
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
    // The leakage check lives in write_audit_reports, so it must run here too.
    // Without this, `backtest` was the one path that could produce a trade log
    // WITHOUT ever verifying no-lookahead — and a trade log is exactly the
    // artifact somebody would believe.
    let observation_manifest: ObservationSetManifest =
        serde_json::from_slice(&fs::read(observation_dir.join("manifest.json"))?)?;
    write_audit_reports(
        &root,
        run_id,
        &observation_manifest.input.dataset_id,
        &observations,
    )?;
    write_run_manifests(config, &root, run_id, observation_set_id)?;
    println!(
        "backtest_configurations={} backtest_trades={}",
        results.len(),
        total_trades
    );
    Ok(())
}

pub fn run_all(
    config: &PipelineConfig,
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
    let context = build_analysis_context(config, &root, &dataset_id, &observations)?;
    let analyses = analyze_observations(&observations, &context);
    let cost_bps = config.costs_bps.first().copied().unwrap_or(0.0);
    let backtests = run_backtests_by_configuration(
        run_id,
        &observations,
        BacktestParams {
            long_quantile: config.long_quantile,
            short_quantile: config.short_quantile,
            cost_bps,
            max_modal_share: config.max_modal_share,
            seed: config.seed,
            development_fraction: config.development_fraction,
        },
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
    write_audit_reports(&root, run_id, &dataset_id, &observations)?;
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

    // Report every verdict value that actually occurred. The old line computed
    // "revise" as `total - continue`, which silently folded `stop`,
    // `expand data`, and `expand sources` into `revise` the moment the full
    // vocabulary existed — turning three distinct diagnoses into one wrong word.
    let mut decisions: BTreeMap<&str, usize> = BTreeMap::new();
    for analysis in &analyses {
        *decisions
            .entry(analysis.recommendation.as_str())
            .or_default() += 1;
    }
    println!("dataset_id={dataset_id}");
    println!("observation_set_id={observation_set_id}");
    println!("configurations={}", analyses.len());
    for (recommendation, count) in &decisions {
        println!("decisions_{}={count}", recommendation.replace(' ', "_"));
    }
    Ok(())
}
