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
    /// Derived from the data actually in the snapshot, never hardcoded. These
    /// were fixture literals until Stage 1 Task 4; on real data a hardcoded
    /// range makes the manifest silently lie, and every downstream
    /// reproducibility claim rests on this manifest.
    pub date_start: NaiveDate,
    pub date_end: NaiveDate,
    /// Counted apart on purpose. Quarantined = broken (drives the `stop`
    /// verdict); excluded = out of scope (must not). Merging them would let a
    /// sample boundary read as a data-quality failure.
    pub quarantined_articles: u64,
    pub excluded_articles: u64,
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
            quarantined_articles: 0,
            excluded_articles: 0,
            files: vec![FileManifest {
                relative_path: "a.parquet".into(),
                sha256: "abc".into(),
                rows: 1,
            }],
        };
        let changed = DatasetManifestInput {
            files: vec![FileManifest {
                relative_path: "a.parquet".into(),
                sha256: "def".into(),
                rows: 1,
            }],
            ..base.clone()
        };

        assert_ne!(dataset_id(&base).unwrap(), dataset_id(&changed).unwrap());
    }
}
