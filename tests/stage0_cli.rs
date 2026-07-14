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
