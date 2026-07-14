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
