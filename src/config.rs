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
        if self.news_windows_minutes.is_empty() {
            bail!("news_windows_minutes must not be empty");
        }
        if self
            .news_windows_minutes
            .iter()
            .any(|minutes| *minutes <= 0)
        {
            bail!("news_windows_minutes must be positive");
        }
        if self.measurement_horizons_minutes.is_empty() {
            bail!("measurement_horizons_minutes must not be empty");
        }
        if self
            .measurement_horizons_minutes
            .iter()
            .any(|minutes| *minutes <= 0)
        {
            bail!("measurement_horizons_minutes must be positive");
        }
        if self.source_sets.is_empty() {
            bail!("source_sets must not be empty");
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
