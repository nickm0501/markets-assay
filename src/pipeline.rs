use crate::{
    config::Stage0Config,
    domain::article::SourceKind,
    fixture::generate_fixture,
    normalize::normalize_articles,
    storage::{
        jsonl::write_jsonl,
        manifest::{
            DatasetManifest, DatasetManifestInput, FileManifest, checksum_file, dataset_id,
        },
        parquet::write_parquet,
    },
};
use anyhow::Result;
use chrono::NaiveDate;
use serde::Serialize;
use std::{collections::BTreeSet, fs, path::PathBuf};

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
    println!("dataset_id={id}");
    Ok(())
}
