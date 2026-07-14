pub mod fixture;
pub mod saved_files;
pub mod vendor;

use crate::{
    config::{PipelineConfig, SourceMode},
    domain::{article::RawArticle, market::PriceBar},
};
use anyhow::{Result, bail};
use serde::{Deserialize, Serialize};
use std::path::Path;

/// One row of `source_catalog.parquet`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceCatalogRow {
    pub source: String,
    pub source_kind: String,
}

/// A swappable supplier of news articles. Implementations return raw vendor
/// records only — normalization, scoring, and relevance mapping stay in
/// `normalize.rs` so every source is held to the same schema, which is the
/// spec's requirement that fixture, saved-file, and API modes be
/// indistinguishable downstream.
pub trait NewsSource {
    /// Vendor names recorded in the dataset manifest's `sources` field.
    /// This is the *adapter's* identity ("fixture", "massive"), not the
    /// per-publisher `source` field carried on each article.
    fn vendor_names(&self) -> Vec<String>;

    fn fetch_raw_articles(&self, config: &PipelineConfig) -> Result<Vec<RawArticle>>;
}

/// A swappable supplier of OHLCV price bars.
pub trait PriceSource {
    fn vendor_names(&self) -> Vec<String>;

    fn fetch_price_bars(&self, config: &PipelineConfig) -> Result<Vec<PriceBar>>;
}

fn saved_files_dir(config: &PipelineConfig) -> Result<&Path> {
    match config.saved_files_dir.as_deref() {
        Some(dir) => Ok(Path::new(dir)),
        None => bail!("saved_files_dir is required when a source is saved_files"),
    }
}

pub fn news_source(config: &PipelineConfig) -> Result<Box<dyn NewsSource>> {
    match config.news_source {
        SourceMode::Fixture => Ok(Box::new(fixture::FixtureSource)),
        SourceMode::SavedFiles => Ok(Box::new(saved_files::SavedFileSource::new(
            saved_files_dir(config)?,
        )?)),
    }
}

pub fn price_source(config: &PipelineConfig) -> Result<Box<dyn PriceSource>> {
    match config.price_source {
        SourceMode::Fixture => Ok(Box::new(fixture::FixtureSource)),
        SourceMode::SavedFiles => Ok(Box::new(saved_files::SavedFileSource::new(
            saved_files_dir(config)?,
        )?)),
    }
}

/// Derives the source catalog from the articles a source actually returned,
/// rather than from a hardcoded list. A hardcoded catalog silently
/// under-reports whichever publishers a source starts or stops emitting; this
/// cannot.
pub fn catalog_from_articles(articles: &[RawArticle]) -> Vec<SourceCatalogRow> {
    let mut seen = std::collections::BTreeSet::new();
    articles
        .iter()
        .filter(|article| seen.insert(article.source.clone()))
        .map(|article| SourceCatalogRow {
            source: article.source.clone(),
            source_kind: article.source_kind.as_str().to_string(),
        })
        .collect()
}
