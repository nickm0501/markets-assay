use crate::{
    config::PipelineConfig,
    domain::{article::RawArticle, market::PriceBar},
    source::{
        NewsSource, PriceSource,
        vendor::{alpaca::parse_alpaca, gdelt::parse_gdelt, massive::parse_massive},
    },
};
use anyhow::{Context, Result, bail};
use std::{
    fs,
    path::{Path, PathBuf},
};

/// Reads real vendor payloads off disk. This is Stage 1's data source: a human
/// fetched these once (`scripts/fetch_sample.sh`) and committed them, so the
/// sample is reproducible forever and cannot rot when a vendor changes an
/// endpoint. There is no network code here, and no HTTP dependency in the
/// binary — live fetching is Stage 2's job.
///
/// Files are dispatched by filename prefix:
///
/// ```text
/// massive_*.json  -> Massive news        (finance)
/// gdelt_*.json    -> GDELT article list  (broad)
/// alpaca_*.json   -> Alpaca price bars
/// ```
#[derive(Debug)]
pub struct SavedFileSource {
    dir: PathBuf,
}

impl SavedFileSource {
    pub fn new(dir: &Path) -> Result<Self> {
        if !dir.is_dir() {
            bail!(
                "saved_files_dir {} does not exist. Fetch the sample first: \
                 scripts/fetch_sample.sh",
                dir.display()
            );
        }
        Ok(Self {
            dir: dir.to_path_buf(),
        })
    }

    /// Files whose name starts with `prefix`, sorted so a run is reproducible
    /// regardless of directory-iteration order.
    fn payloads(&self, prefix: &str) -> Result<Vec<PathBuf>> {
        let mut paths: Vec<PathBuf> = fs::read_dir(&self.dir)
            .with_context(|| format!("failed to read {}", self.dir.display()))?
            .filter_map(|entry| entry.ok().map(|entry| entry.path()))
            .filter(|path| {
                path.file_name()
                    .and_then(|name| name.to_str())
                    .is_some_and(|name| name.starts_with(prefix) && name.ends_with(".json"))
            })
            .collect();
        paths.sort();
        Ok(paths)
    }
}

impl NewsSource for SavedFileSource {
    fn vendor_names(&self) -> Vec<String> {
        vec!["massive".to_string(), "gdelt".to_string()]
    }

    fn fetch_raw_articles(&self, _config: &PipelineConfig) -> Result<Vec<RawArticle>> {
        let mut articles = Vec::new();
        for path in self.payloads("massive_")? {
            let json = fs::read_to_string(&path)
                .with_context(|| format!("failed to read {}", path.display()))?;
            articles
                .extend(parse_massive(&json).with_context(|| format!("in {}", path.display()))?);
        }
        for path in self.payloads("gdelt_")? {
            let json = fs::read_to_string(&path)
                .with_context(|| format!("failed to read {}", path.display()))?;
            articles.extend(parse_gdelt(&json).with_context(|| format!("in {}", path.display()))?);
        }
        // An empty news sample is a fetch that silently failed, not a valid
        // dataset. Reporting "0 articles, no signal" would be a lie dressed as
        // a result.
        if articles.is_empty() {
            bail!(
                "no news articles found in {}. Expected massive_*.json or gdelt_*.json. \
                 Run scripts/fetch_sample.sh",
                self.dir.display()
            );
        }
        Ok(articles)
    }
}

impl PriceSource for SavedFileSource {
    fn vendor_names(&self) -> Vec<String> {
        vec!["alpaca".to_string()]
    }

    fn fetch_price_bars(&self, config: &PipelineConfig) -> Result<Vec<PriceBar>> {
        let mut bars = Vec::new();
        for path in self.payloads("alpaca_")? {
            let json = fs::read_to_string(&path)
                .with_context(|| format!("failed to read {}", path.display()))?;
            bars.extend(
                parse_alpaca(&json, config.price_interval_minutes)
                    .with_context(|| format!("in {}", path.display()))?,
            );
        }
        if bars.is_empty() {
            bail!(
                "no price bars found in {}. Expected alpaca_*.json. Run scripts/fetch_sample.sh",
                self.dir.display()
            );
        }
        Ok(bars)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::PipelineConfig;
    use tempfile::TempDir;

    fn config() -> PipelineConfig {
        PipelineConfig::load("configs/stage0_fixture.json").unwrap()
    }

    #[test]
    fn a_missing_directory_is_an_error_not_an_empty_dataset() {
        let error = SavedFileSource::new(Path::new("does/not/exist"))
            .unwrap_err()
            .to_string();

        assert!(error.contains("does not exist"), "got: {error}");
    }

    #[test]
    fn saved_file_source_with_an_empty_directory_is_an_error_not_an_empty_dataset() {
        // A pipeline that happily reports "no signal found" from zero articles
        // is worse than one that fails: it produces a confident, wrong answer.
        let temp = TempDir::new().unwrap();
        let source = SavedFileSource::new(temp.path()).unwrap();

        let error = source
            .fetch_raw_articles(&config())
            .unwrap_err()
            .to_string();

        assert!(error.contains("no news articles found"), "got: {error}");
    }

    #[test]
    fn the_reader_combines_massive_and_gdelt_payloads_into_one_article_stream() {
        let temp = TempDir::new().unwrap();
        fs::write(
            temp.path().join("massive_spy.json"),
            r#"{"results":[{"id":"m1","publisher":{"name":"Reuters"},"title":"SPY up",
                "published_utc":"2025-03-04T14:35:00Z","tickers":["SPY"]}]}"#,
        )
        .unwrap();
        fs::write(
            temp.path().join("gdelt_macro.json"),
            r#"{"articles":[{"url":"u","title":"Fed holds","domain":"reuters.com",
                "seendate":"20250304T143000Z"}]}"#,
        )
        .unwrap();
        let source = SavedFileSource::new(temp.path()).unwrap();

        let articles = source.fetch_raw_articles(&config()).unwrap();

        assert_eq!(articles.len(), 2);
        assert!(articles.iter().any(|a| a.source == "Reuters"));
        assert!(articles.iter().any(|a| a.source == "reuters.com"));
    }

    #[test]
    fn fixture_source_and_saved_file_source_produce_the_same_normalized_schema() {
        // The spec's requirement, made mechanical: "Fixture, curated-file, and
        // API modes must produce the same normalized schemas." Both sources
        // feed the identical normalize step and yield identical row types, so
        // nothing downstream can tell them apart.
        use crate::{normalize::normalize_articles, source::fixture::FixtureSource};

        let temp = TempDir::new().unwrap();
        fs::write(
            temp.path().join("massive_spy.json"),
            r#"{"results":[{"id":"m1","publisher":{"name":"Reuters"},
                "title":"SPY strong breakout","description":"constructive",
                "published_utc":"2026-06-29T14:35:00Z","tickers":["SPY"]}]}"#,
        )
        .unwrap();
        let config = config();

        let from_fixture =
            normalize_articles(&config, &FixtureSource.fetch_raw_articles(&config).unwrap())
                .unwrap();
        let from_saved = normalize_articles(
            &config,
            &SavedFileSource::new(temp.path())
                .unwrap()
                .fetch_raw_articles(&config)
                .unwrap(),
        )
        .unwrap();

        assert!(!from_fixture.normalized.is_empty());
        assert!(!from_saved.normalized.is_empty());
        // Same type, same fields, same invariants — the schemas cannot diverge
        // because there is exactly one normalize path.
        let fixture_row = &from_fixture.normalized[0];
        let saved_row = &from_saved.normalized[0];
        assert!(!fixture_row.article_id.is_empty() && !saved_row.article_id.is_empty());
        assert!(!fixture_row.relevant_symbols.is_empty() && !saved_row.relevant_symbols.is_empty());
        assert!(saved_row.available_at >= saved_row.published_at);
    }
}
