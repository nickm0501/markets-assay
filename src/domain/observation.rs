use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// `mean_sentiment`, `weighted_sentiment`, `extreme_sentiment`, and
/// `sentiment_dispersion` together satisfy the spec's Main Research Dataset
/// requirement for "mean, weighted, extreme, and dispersion sentiment
/// features" per row. `extreme_sentiment` is the eligible article's
/// sentiment score with the largest absolute value (sign preserved), i.e.
/// the single most extreme reading in the window, not just its magnitude.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NewsSignalObservation {
    pub observation_id: String,
    pub dataset_id: String,
    pub symbol: String,
    pub signal_time: DateTime<Utc>,
    pub news_window_minutes: i64,
    pub measurement_horizon_minutes: i64,
    pub price_interval_minutes: i64,
    pub source_set: String,
    pub article_count: u32,
    pub ticker_article_count: u32,
    pub sector_theme_article_count: u32,
    pub macro_article_count: u32,
    pub source_count: u32,
    pub publisher_count: u32,
    pub mean_sentiment: f64,
    pub weighted_sentiment: f64,
    pub extreme_sentiment: f64,
    pub positive_article_count: u32,
    pub negative_article_count: u32,
    pub sentiment_dispersion: f64,
    pub prior_return: f64,
    pub prior_volatility: f64,
    pub market_session: String,
    pub is_after_hours_signal: bool,
    pub future_return: f64,
    pub future_volatility: f64,
    pub future_tail_event: bool,
    pub future_max_drawdown: f64,
    pub future_max_runup: f64,
    pub entry_time: DateTime<Utc>,
    pub exit_time: DateTime<Utc>,
    pub article_ids: Vec<String>,
    pub price_bar_ids: Vec<String>,
    pub created_by_run_id: String,
}
