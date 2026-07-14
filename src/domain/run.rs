use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CoverageRow {
    pub news_window_minutes: i64,
    pub measurement_horizon_minutes: i64,
    pub source_set: String,
    pub symbol: String,
    pub observation_count: u32,
    pub article_count: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BucketReturnRow {
    pub news_window_minutes: i64,
    pub measurement_horizon_minutes: i64,
    pub source_set: String,
    pub bucket: String,
    pub observation_count: u32,
    pub mean_sentiment: f64,
    pub mean_future_return: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Trade {
    pub run_id: String,
    pub observation_id: String,
    pub symbol: String,
    pub news_window_minutes: i64,
    pub measurement_horizon_minutes: i64,
    pub source_set: String,
    /// Which strategy took this trade: `sentiment` or one of the baselines.
    /// Every strategy runs through the same engine on the same observations, so
    /// their trade logs are directly comparable.
    pub strategy: String,
    pub side: String,
    pub signal_time: DateTime<Utc>,
    pub entry_time: DateTime<Utc>,
    pub exit_time: DateTime<Utc>,
    pub sentiment: f64,
    pub gross_return: f64,
    pub cost_bps: f64,
    pub net_return: f64,
}

/// `gross_return_sum`/`net_return_sum`/`win_rate`/`profit_factor` are
/// combined across both sides. `long_*`/`short_*` report the same measures
/// scoped to only long or only short trades, satisfying the spec's Backtest
/// Rules requirement to "report long and short sides separately as well as
/// combined."
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BacktestMetrics {
    pub run_id: String,
    pub news_window_minutes: i64,
    pub measurement_horizon_minutes: i64,
    pub source_set: String,
    /// `sentiment` | `always_flat` | `random` | `prior_return_momentum` |
    /// `shuffled_sentiment`. The spec's failure gate turns on comparing these:
    /// "stop or revise when sentiment performs no better than shuffled or
    /// non-sentiment baselines".
    pub strategy: String,
    pub cost_bps: f64,
    pub trade_count: u32,
    pub long_count: u32,
    pub short_count: u32,
    pub gross_return_sum: f64,
    pub net_return_sum: f64,
    pub average_net_return: f64,
    pub win_rate: f64,
    pub profit_factor: f64,
    pub max_drawdown: f64,
    pub long_gross_return_sum: f64,
    pub long_net_return_sum: f64,
    pub long_win_rate: f64,
    pub long_profit_factor: f64,
    pub short_gross_return_sum: f64,
    pub short_net_return_sum: f64,
    pub short_win_rate: f64,
    pub short_profit_factor: f64,
    /// The sentiment distribution was too coarse to test, so this configuration
    /// took NO trades. Distinct from a configuration that traded and made
    /// nothing — an explicit flag, never a silent zero, because a reader must be
    /// able to tell "we declined" from "we tried and it was flat".
    pub degenerate: bool,
}
