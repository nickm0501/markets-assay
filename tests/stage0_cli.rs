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

/// The fixture's snapshot ids, pinned.
///
/// `dataset_id` is a content hash, so it is *supposed* to move when the
/// snapshot's contents change — and to stay put when they do not. That makes it
/// a precise tripwire, but only if every change to it is deliberate. Do not
/// update these constants to make a red test go green: work out which change
/// moved the hash and confirm you meant it. Changelog:
///
/// | id                  | why it moved                                       |
/// |---------------------|----------------------------------------------------|
/// | ds_9fc5b9a291fd38fd | Stage 0 baseline.                                  |
/// | ds_9fc5b9a291fd38fd | Task 1 (swappable sources) — UNCHANGED, as intended.|
/// | ds_32872c03f39b4cd6 | Task 2 + 4: set_aside_articles.parquet added to the |
/// |                     | manifest; quarantined/excluded counts added; date   |
/// |                     | bounds now derived rather than hardcoded.          |
/// | ds_963e4e90a5a10027 | Real-data fixes: the domain enums now serialize as  |
/// |                     | strings, so their Parquet column type changed from  |
/// |                     | Dictionary to Utf8 (the old schema depended on      |
/// |                     | WHICH enum variants the data happened to contain,   |
/// |                     | and broke on real data that never produced a        |
/// |                     | SectorTheme article). Plus RELEVANCE_RULE_VERSION   |
/// |                     | bumped to stage1_relevance_v2 for the               |
/// |                     | constituent->ETF mapping.                          |
/// | ds_64a221a5a8ec40a7 | Stage 2: the 14-word lexicon was replaced by        |
/// |                     | Loughran-McDonald + VADER, so every sentiment score |
/// |                     | changed and SENTIMENT_VERSION bumped.              |
/// | ds_f54fedeadaa2bd75 | Vendor sentiment stored as a benchmark, adding two  |
/// |                     | columns to normalized_articles.parquet.            |
///
/// THE VERDICTS ALSO MOVED, and not cosmetically. The fixture went 5 -> 3 -> 1 -> 0
/// `continue` across Stage 2. Every step down was a bug fixed or a missing gate
/// added, never a regression:
///
///   5  Stage 0/1 baseline. TWO WERE FLOATING-POINT NOISE — `observed > shuffled`
///      was true for spreads equal to fifteen decimal places, differing only in
///      the order the floats were summed.
///   3  after `min_spread_margin`.
///   1  after the four baselines landed (Decision 6's open half): `continue` now
///      also requires beating EVERY baseline, not just the shuffled one.
///   0  after the null was fixed (it was a ROTATION, not a shuffle) and the bar
///      became the null's 95th percentile rather than its mean — the mean is a
///      coin flip, since ~50% of noise draws beat the mean of their own null.
///      Plus the dev/holdout split: the spread is now measured on HELD-OUT data
///      with thresholds frozen from development.
///
/// **The fixture can no longer demonstrate a `continue`, and that is CORRECT.**
/// Six synthetic articles cannot support a statistically significant result, and
/// the pipeline now says so instead of pretending otherwise. `continue` is
/// demonstrated in tests/power_check.rs, on data large enough to earn it.
const FIXTURE_DATASET_ID: &str = "ds_f54fedeadaa2bd75";
const FIXTURE_OBSERVATION_SET_ID: &str = "obs_51695fe1ee301377";

/// The research loop's output must be unmoved by all of Stage 1's plumbing
/// work. If the *verdicts* ever change, the refactor broke the science, not
/// just the storage layout — and that is a different and much worse bug than a
/// moved hash.
#[test]
fn stage_1_plumbing_leaves_the_fixtures_research_verdicts_untouched() {
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
    .stdout(predicate::str::contains(format!(
        "dataset_id={FIXTURE_DATASET_ID}"
    )))
    .stdout(predicate::str::contains(format!(
        "observation_set_id={FIXTURE_OBSERVATION_SET_ID}"
    )))
    .stdout(predicate::str::contains("configurations=6"))
    // One line per verdict value that actually occurred. `run_all` used to
    // print "revise" as `total - continue`, which folded stop/expand data/
    // expand sources into `revise` the moment the full vocabulary existed.
    .stdout(predicate::str::contains("decisions_revise=6"));
}

/// Task 9: the leakage check and inspection tables run on every invocation, not
/// only in unit tests. On the real sample these are the deliverable — Stage 1 is
/// named "timestamp and leakage inspection", and inspection needs a table.
#[test]
fn a_run_writes_the_timestamp_audit_and_set_aside_reports() {
    let temp = TempDir::new().unwrap();
    Command::cargo_bin("markets")
        .unwrap()
        .args([
            "run",
            "--config",
            "configs/stage0_fixture.json",
            "--output-root",
            temp.path().to_str().unwrap(),
        ])
        .assert()
        .success();

    let reports = temp.path().join("runs/stage0_fixture/reports");
    let audit = std::fs::read_to_string(reports.join("timestamp_audit.csv")).unwrap();
    let set_aside = std::fs::read_to_string(reports.join("set_aside.csv")).unwrap();

    // The fixture's after-hours article (published 21:15 UTC) must show as
    // deferred — that column is where a timezone or DST bug becomes visible.
    assert!(audit.contains("was_deferred"));
    assert!(audit.contains("gdelt-2"));
    assert!(audit.contains("true"));
    // The syndicated duplicate is EXCLUDED (scope), not quarantined (quality).
    assert!(set_aside.contains("massive-1-dup"));
    assert!(set_aside.contains("excluded"));
    assert!(set_aside.contains("duplicate"));
    assert!(!set_aside.contains("quarantined"));
}

/// The spec's architecture lists `ingest` as its own stage; Stage 0 never built
/// it, so the fixture generator was the only way to produce a snapshot.
#[test]
fn ingest_builds_a_snapshot_from_the_configured_source() {
    let temp = TempDir::new().unwrap();
    let mut cmd = Command::cargo_bin("markets").unwrap();
    cmd.args([
        "ingest",
        "--config",
        "configs/stage0_fixture.json",
        "--output-root",
        temp.path().to_str().unwrap(),
    ])
    .assert()
    .success()
    .stdout(predicate::str::contains(format!(
        "dataset_id={FIXTURE_DATASET_ID}"
    )));

    assert!(
        temp.path()
            .join(format!("data/datasets/{FIXTURE_DATASET_ID}/manifest.json"))
            .exists()
    );
}

/// Task 4: the manifest's date bounds were fixture literals
/// (`2026-06-29`/`2026-07-07`) hardcoded in `pipeline.rs`. They are now derived
/// from the data actually in the snapshot. Deriving them must reproduce those
/// literals exactly for the fixture — which proves the fix is a no-op on the
/// fixture path while being *correct* on real data, where a hardcoded range
/// would have made the manifest silently lie.
#[test]
fn the_manifest_date_range_is_derived_from_data_and_reproduces_the_fixture_window() {
    let temp = TempDir::new().unwrap();
    Command::cargo_bin("markets")
        .unwrap()
        .args([
            "ingest",
            "--config",
            "configs/stage0_fixture.json",
            "--output-root",
            temp.path().to_str().unwrap(),
        ])
        .assert()
        .success();

    let manifest: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(
            temp.path()
                .join(format!("data/datasets/{FIXTURE_DATASET_ID}/manifest.json")),
        )
        .unwrap(),
    )
    .unwrap();

    assert_eq!(manifest["date_start"], "2026-06-29");
    assert_eq!(manifest["date_end"], "2026-07-07");
    // The fixture's one syndicated duplicate: excluded (scope), not
    // quarantined (quality). Merging these counters would let a sample
    // boundary trip the `stop` verdict.
    assert_eq!(manifest["quarantined_articles"], 0);
    assert_eq!(manifest["excluded_articles"], 1);
}

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

#[test]
fn fixture_command_writes_dataset_snapshot() {
    let temp = TempDir::new().unwrap();
    let mut cmd = Command::cargo_bin("markets").unwrap();
    let output = cmd
        .args([
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

#[test]
fn build_observations_reuses_dataset_snapshot() {
    let temp = TempDir::new().unwrap();
    let dataset_id = run_fixture_and_extract_dataset_id(temp.path());

    let mut cmd = Command::cargo_bin("markets").unwrap();
    let output = cmd
        .args([
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
    let observation_dir = temp
        .path()
        .join("data")
        .join("observation_sets")
        .join(observation_set_id);

    assert!(
        observation_dir
            .join("news_signal_observations.parquet")
            .exists()
    );
    assert!(observation_dir.join("manifest.json").exists());
    assert!(
        temp.path()
            .join("data")
            .join("datasets")
            .join(dataset_id)
            .join("manifest.json")
            .exists()
    );
}

fn run_fixture_and_extract_dataset_id(root: &std::path::Path) -> String {
    let mut cmd = Command::cargo_bin("markets").unwrap();
    let output = cmd
        .args([
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
    String::from_utf8(output)
        .unwrap()
        .trim()
        .strip_prefix("dataset_id=")
        .unwrap()
        .to_string()
}

#[test]
fn analyze_writes_reusable_report_tables() {
    let temp = TempDir::new().unwrap();
    let dataset_id = run_fixture_and_extract_dataset_id(temp.path());
    let observation_set_id =
        run_build_observations_and_extract_observation_set_id(temp.path(), &dataset_id);

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
    .stdout(predicate::str::contains("analysis_configurations="));

    let reports = temp.path().join("runs/stage0_fixture/reports");
    assert!(reports.join("coverage.csv").exists());
    assert!(reports.join("bucket_returns.csv").exists());
    assert!(reports.join("analysis_summary.json").exists());

    let run_dir = temp.path().join("runs/stage0_fixture");
    assert!(run_dir.join("config.json").exists());
    assert!(run_dir.join("dataset_manifest.json").exists());
    assert!(run_dir.join("observation_set_manifest.json").exists());
}

fn run_build_observations_and_extract_observation_set_id(
    root: &std::path::Path,
    dataset_id: &str,
) -> String {
    let mut cmd = Command::cargo_bin("markets").unwrap();
    let output = cmd
        .args([
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
    String::from_utf8(output)
        .unwrap()
        .trim()
        .strip_prefix("observation_set_id=")
        .unwrap()
        .to_string()
}

#[test]
fn backtest_reruns_with_changed_cost_without_rebuilding_dataset() {
    let temp = TempDir::new().unwrap();
    let dataset_id = run_fixture_and_extract_dataset_id(temp.path());
    let observation_set_id =
        run_build_observations_and_extract_observation_set_id(temp.path(), &dataset_id);

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

    assert!(
        temp.path()
            .join("data")
            .join("datasets")
            .join(dataset_id)
            .join("manifest.json")
            .exists()
    );
    assert!(
        temp.path()
            .join("runs/stage0_fixture/reports/backtest_metrics.csv")
            .exists()
    );
    assert!(
        temp.path()
            .join("runs/stage0_fixture/reports/trade_log.csv")
            .exists()
    );
}

#[test]
fn distinct_run_ids_keep_separate_backtest_reports_for_comparison() {
    // Decision Demo step 6 ("compare both runs") requires that changing a
    // cost assumption and rerunning does not overwrite the first run's
    // reports. Passing a distinct --run-id per cost keeps both on disk.
    let temp = TempDir::new().unwrap();
    let dataset_id = run_fixture_and_extract_dataset_id(temp.path());
    let observation_set_id =
        run_build_observations_and_extract_observation_set_id(temp.path(), &dataset_id);

    for (run_id, cost) in [
        ("stage0_fixture_cost0", "0"),
        ("stage0_fixture_cost10", "10"),
    ] {
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
            "--run-id",
            run_id,
        ])
        .assert()
        .success();
    }

    let low_cost_metrics = std::fs::read_to_string(
        temp.path()
            .join("runs/stage0_fixture_cost0/reports/backtest_metrics.csv"),
    )
    .unwrap();
    let high_cost_metrics = std::fs::read_to_string(
        temp.path()
            .join("runs/stage0_fixture_cost10/reports/backtest_metrics.csv"),
    )
    .unwrap();

    assert!(
        temp.path()
            .join("runs/stage0_fixture_cost0/config.json")
            .exists()
    );
    assert!(
        temp.path()
            .join("runs/stage0_fixture_cost10/config.json")
            .exists()
    );
    assert_ne!(low_cost_metrics, high_cost_metrics);
}

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
    .stdout(predicate::str::contains("configurations="));

    let run_dir = temp.path().join("runs/stage0_fixture");
    assert!(run_dir.join("reports/summary.md").exists());
    assert!(run_dir.join("reports/coverage.csv").exists());
    assert!(run_dir.join("reports/bucket_returns.csv").exists());
    assert!(run_dir.join("reports/backtest_metrics.csv").exists());
    assert!(run_dir.join("reports/trade_log.csv").exists());
    assert!(run_dir.join("charts/bucket_returns.svg").exists());
    assert!(run_dir.join("charts/equity_curve.svg").exists());
    assert!(run_dir.join("config.json").exists());
    assert!(run_dir.join("dataset_manifest.json").exists());
    assert!(run_dir.join("observation_set_manifest.json").exists());
}

#[test]
fn full_pipeline_reuses_snapshot_for_multiple_backtests() {
    let temp = TempDir::new().unwrap();
    let dataset_id = run_fixture_and_extract_dataset_id(temp.path());
    let observation_set_id =
        run_build_observations_and_extract_observation_set_id(temp.path(), &dataset_id);

    let dataset_manifest = temp
        .path()
        .join("data")
        .join("datasets")
        .join(&dataset_id)
        .join("manifest.json");
    let before = std::fs::metadata(&dataset_manifest)
        .unwrap()
        .modified()
        .unwrap();

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

    let after = std::fs::metadata(&dataset_manifest)
        .unwrap()
        .modified()
        .unwrap();
    assert_eq!(before, after);
}

/// The no-lookahead check lives in `write_audit_reports`. `backtest` was the one
/// command that produced a trade log WITHOUT calling it — and a trade log is
/// exactly the artifact somebody would believe. Every command that emits results
/// must first verify that no observation used information that did not exist yet.
#[test]
fn a_standalone_backtest_still_runs_the_leakage_check() {
    let temp = TempDir::new().unwrap();
    let root = temp.path().to_str().unwrap();

    let dataset_id = run_fixture_and_extract_dataset_id(temp.path());
    let observation_set_id =
        run_build_observations_and_extract_observation_set_id(temp.path(), &dataset_id);

    Command::cargo_bin("markets")
        .unwrap()
        .args([
            "backtest",
            "--config",
            "configs/stage0_fixture.json",
            "--output-root",
            root,
            "--observation-set-id",
            &observation_set_id,
            "--run-id",
            "backtest_only",
            "--cost-bps",
            "5",
        ])
        .assert()
        .success();

    // These two files are written by `write_audit_reports`, which calls
    // `assert_no_lookahead` first. Their presence is the evidence the check ran.
    let reports = temp.path().join("runs/backtest_only/reports");
    assert!(
        reports.join("timestamp_audit.csv").exists(),
        "backtest must run the leakage check, not just emit a trade log"
    );
    assert!(reports.join("set_aside.csv").exists());
    assert!(reports.join("trade_log.csv").exists());
}
