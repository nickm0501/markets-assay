use crate::domain::{
    observation::NewsSignalObservation,
    run::{BacktestMetrics, Trade},
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BacktestResult {
    pub metrics: BacktestMetrics,
    pub trades: Vec<Trade>,
}

/// One BacktestResult per (news_window, measurement_horizon, source_set)
/// configuration, per design.md Decision 1. Quantile thresholds and the
/// overlap-skip trade-slot logic must stay scoped to one configuration —
/// pooling them lets unrelated configurations compete for the same trade.
pub fn run_backtests_by_configuration(
    run_id: &str,
    observations: &[NewsSignalObservation],
    long_quantile: f64,
    short_quantile: f64,
    cost_bps: f64,
) -> Vec<BacktestResult> {
    let mut groups: BTreeMap<(i64, i64, String), Vec<&NewsSignalObservation>> = BTreeMap::new();
    for row in observations {
        groups
            .entry((
                row.news_window_minutes,
                row.measurement_horizon_minutes,
                row.source_set.clone(),
            ))
            .or_default()
            .push(row);
    }
    groups
        .into_iter()
        .map(|(key, group)| {
            run_backtest_for_configuration(
                run_id,
                &key,
                &group,
                long_quantile,
                short_quantile,
                cost_bps,
            )
        })
        .collect()
}

fn run_backtest_for_configuration(
    run_id: &str,
    key: &(i64, i64, String),
    observations: &[&NewsSignalObservation],
    long_quantile: f64,
    short_quantile: f64,
    cost_bps: f64,
) -> BacktestResult {
    let long_threshold = quantile(
        observations.iter().map(|row| row.mean_sentiment).collect(),
        long_quantile,
    );
    let short_threshold = quantile(
        observations.iter().map(|row| row.mean_sentiment).collect(),
        short_quantile,
    );
    let mut last_exit_by_symbol = BTreeMap::<String, chrono::DateTime<chrono::Utc>>::new();
    let mut sorted: Vec<&NewsSignalObservation> = observations.to_vec();
    sorted.sort_by_key(|row| (row.entry_time, row.symbol.clone()));
    let mut trades = Vec::new();
    for row in sorted {
        if last_exit_by_symbol
            .get(&row.symbol)
            .is_some_and(|last_exit| row.entry_time < *last_exit)
        {
            continue;
        }
        let side = if row.mean_sentiment >= long_threshold {
            "long"
        } else if row.mean_sentiment <= short_threshold {
            "short"
        } else {
            continue;
        };
        let gross_return = if side == "long" {
            row.future_return
        } else {
            -row.future_return
        };
        let net_return = gross_return - cost_bps / 10_000.0;
        last_exit_by_symbol.insert(row.symbol.clone(), row.exit_time);
        trades.push(Trade {
            run_id: run_id.into(),
            observation_id: row.observation_id.clone(),
            symbol: row.symbol.clone(),
            news_window_minutes: key.0,
            measurement_horizon_minutes: key.1,
            source_set: key.2.clone(),
            side: side.into(),
            signal_time: row.signal_time,
            entry_time: row.entry_time,
            exit_time: row.exit_time,
            sentiment: row.mean_sentiment,
            gross_return,
            cost_bps,
            net_return,
        });
    }
    let metrics = metrics(run_id, key, cost_bps, &trades);
    BacktestResult { metrics, trades }
}

fn quantile(mut values: Vec<f64>, q: f64) -> f64 {
    values.sort_by(|a, b| a.partial_cmp(b).unwrap());
    if values.is_empty() {
        return 0.0;
    }
    let idx = ((values.len() - 1) as f64 * q).round() as usize;
    values[idx]
}

fn metrics(
    run_id: &str,
    key: &(i64, i64, String),
    cost_bps: f64,
    trades: &[Trade],
) -> BacktestMetrics {
    let gross_return_sum: f64 = trades.iter().map(|trade| trade.gross_return).sum();
    let net_return_sum: f64 = trades.iter().map(|trade| trade.net_return).sum();
    let wins = trades.iter().filter(|trade| trade.net_return > 0.0).count() as f64;
    let gains: f64 = trades
        .iter()
        .filter(|trade| trade.net_return > 0.0)
        .map(|trade| trade.net_return)
        .sum();
    let losses: f64 = trades
        .iter()
        .filter(|trade| trade.net_return < 0.0)
        .map(|trade| trade.net_return.abs())
        .sum();
    let (long_gross_return_sum, long_net_return_sum, long_win_rate, long_profit_factor) =
        side_summary(trades, "long");
    let (short_gross_return_sum, short_net_return_sum, short_win_rate, short_profit_factor) =
        side_summary(trades, "short");
    BacktestMetrics {
        run_id: run_id.into(),
        news_window_minutes: key.0,
        measurement_horizon_minutes: key.1,
        source_set: key.2.clone(),
        cost_bps,
        trade_count: trades.len() as u32,
        long_count: trades.iter().filter(|trade| trade.side == "long").count() as u32,
        short_count: trades.iter().filter(|trade| trade.side == "short").count() as u32,
        gross_return_sum,
        net_return_sum,
        average_net_return: if trades.is_empty() {
            0.0
        } else {
            net_return_sum / trades.len() as f64
        },
        win_rate: if trades.is_empty() {
            0.0
        } else {
            wins / trades.len() as f64
        },
        profit_factor: if losses == 0.0 { gains } else { gains / losses },
        max_drawdown: max_drawdown(trades),
        long_gross_return_sum,
        long_net_return_sum,
        long_win_rate,
        long_profit_factor,
        short_gross_return_sum,
        short_net_return_sum,
        short_win_rate,
        short_profit_factor,
    }
}

/// Returns `(gross_return_sum, net_return_sum, win_rate, profit_factor)`
/// scoped to only the given `side` ("long" or "short"), mirroring the
/// combined computation above so `BacktestMetrics` can report both — spec's
/// Backtest Rules: "Report long and short sides separately as well as
/// combined."
fn side_summary(trades: &[Trade], side: &str) -> (f64, f64, f64, f64) {
    let side_trades: Vec<&Trade> = trades.iter().filter(|trade| trade.side == side).collect();
    let gross_return_sum: f64 = side_trades.iter().map(|trade| trade.gross_return).sum();
    let net_return_sum: f64 = side_trades.iter().map(|trade| trade.net_return).sum();
    let wins = side_trades
        .iter()
        .filter(|trade| trade.net_return > 0.0)
        .count() as f64;
    let gains: f64 = side_trades
        .iter()
        .filter(|trade| trade.net_return > 0.0)
        .map(|trade| trade.net_return)
        .sum();
    let losses: f64 = side_trades
        .iter()
        .filter(|trade| trade.net_return < 0.0)
        .map(|trade| trade.net_return.abs())
        .sum();
    let win_rate = if side_trades.is_empty() {
        0.0
    } else {
        wins / side_trades.len() as f64
    };
    let profit_factor = if losses == 0.0 { gains } else { gains / losses };
    (gross_return_sum, net_return_sum, win_rate, profit_factor)
}

fn max_drawdown(trades: &[Trade]) -> f64 {
    let mut equity = 0.0;
    let mut peak = 0.0;
    let mut worst = 0.0;
    for trade in trades {
        equity += trade.net_return;
        if equity > peak {
            peak = equity;
        }
        let drawdown = equity - peak;
        if drawdown < worst {
            worst = drawdown;
        }
    }
    worst
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        config::Stage0Config, fixture::generate_fixture, normalize::normalize_articles,
        observations::build_observations,
    };

    fn observations() -> Vec<crate::domain::observation::NewsSignalObservation> {
        let config = Stage0Config::load("configs/stage0_fixture.json").unwrap();
        let fixture = generate_fixture(&config).unwrap();
        let articles = normalize_articles(&config, &fixture.raw_articles).unwrap();
        build_observations(&config, "ds_test", &articles, &fixture.price_bars).unwrap()
    }

    fn finance_only_hour_hour_result(results: &[BacktestResult]) -> &BacktestResult {
        results
            .iter()
            .find(|result| {
                result.metrics.news_window_minutes == 60
                    && result.metrics.measurement_horizon_minutes == 60
                    && result.metrics.source_set == "finance_only"
            })
            .unwrap()
    }

    #[test]
    fn backtest_takes_long_and_short_trades_within_a_configuration() {
        let results =
            run_backtests_by_configuration("stage0_fixture", &observations(), 0.8, 0.2, 5.0);
        let result = finance_only_hour_hour_result(&results);

        assert!(result.metrics.long_count > 0);
        assert!(result.metrics.short_count > 0);
        assert_eq!(result.metrics.trade_count as usize, result.trades.len());
    }

    #[test]
    fn backtest_does_not_mix_trade_slots_across_configurations() {
        let results =
            run_backtests_by_configuration("stage0_fixture", &observations(), 0.8, 0.2, 5.0);

        assert!(results.len() > 1);
        assert!(
            results
                .iter()
                .all(|result| result.trades.iter().all(|trade| {
                    trade.news_window_minutes == result.metrics.news_window_minutes
                        && trade.measurement_horizon_minutes
                            == result.metrics.measurement_horizon_minutes
                        && trade.source_set == result.metrics.source_set
                }))
        );
    }

    #[test]
    fn costs_reduce_net_returns() {
        let no_cost =
            run_backtests_by_configuration("stage0_fixture", &observations(), 0.8, 0.2, 0.0);
        let with_cost =
            run_backtests_by_configuration("stage0_fixture", &observations(), 0.8, 0.2, 10.0);
        let no_cost_total: f64 = no_cost
            .iter()
            .map(|result| result.metrics.net_return_sum)
            .sum();
        let with_cost_total: f64 = with_cost
            .iter()
            .map(|result| result.metrics.net_return_sum)
            .sum();

        assert!(with_cost_total < no_cost_total);
    }

    #[test]
    fn short_trade_profit_uses_negative_future_return() {
        let results =
            run_backtests_by_configuration("stage0_fixture", &observations(), 0.8, 0.2, 0.0);
        let profitable_short = results
            .iter()
            .flat_map(|result| result.trades.iter())
            .find(|trade| trade.side == "short" && trade.gross_return > 0.0)
            .unwrap();

        assert!(profitable_short.net_return > 0.0);
    }

    #[test]
    fn per_side_metrics_sum_to_combined_metrics() {
        // Spec's Backtest Rules: "Report long and short sides separately as
        // well as combined." The combined fields must always equal the sum
        // of the long-only and short-only fields for the same trade set.
        let results =
            run_backtests_by_configuration("stage0_fixture", &observations(), 0.8, 0.2, 5.0);
        let result = finance_only_hour_hour_result(&results);
        let metrics = &result.metrics;

        assert!(
            (metrics.gross_return_sum
                - (metrics.long_gross_return_sum + metrics.short_gross_return_sum))
                .abs()
                < 1e-9
        );
        assert!(
            (metrics.net_return_sum - (metrics.long_net_return_sum + metrics.short_net_return_sum))
                .abs()
                < 1e-9
        );
        assert!(metrics.long_count > 0);
        assert!(metrics.short_count > 0);
    }

    #[test]
    fn traces_one_fixture_article_from_raw_input_through_normalization_observation_and_trade() {
        // Required Tests: "A manually traceable fixture observation from raw
        // article through trade." massive-1 (SPY, finance, published
        // 2026-06-29T14:05:00Z) is the sole finance-only article eligible
        // for the SPY 2026-06-29T15:30:00Z / 240-minute-window /
        // 60-minute-horizon observation: window_start is 11:30, so
        // available_at 14:05 falls inside (11:30, 15:30], and massive-2 (the
        // only other finance article) is published on a different day so it
        // can never appear in this window. A long_quantile of 0.0 makes
        // every observation in the (240, 60, finance_only) group qualify as
        // "long" (threshold == the group's minimum sentiment), so the trade
        // outcome does not depend on the rest of the fixture's sentiment
        // distribution.
        let config = Stage0Config::load("configs/stage0_fixture.json").unwrap();
        let fixture = generate_fixture(&config).unwrap();
        let raw = fixture
            .raw_articles
            .iter()
            .find(|article| article.vendor_id == "massive-1")
            .unwrap();
        let articles = normalize_articles(&config, &fixture.raw_articles).unwrap();
        let normalized = articles
            .iter()
            .find(|article| article.vendor_id == "massive-1")
            .unwrap();
        assert_eq!(raw.url, normalized.url);

        let observations =
            build_observations(&config, "ds_test", &articles, &fixture.price_bars).unwrap();
        let observation = observations
            .iter()
            .find(|row| {
                row.symbol == "SPY"
                    && row.source_set == "finance_only"
                    && row.news_window_minutes == 240
                    && row.measurement_horizon_minutes == 60
                    && row.signal_time.to_rfc3339() == "2026-06-29T15:30:00+00:00"
            })
            .unwrap();
        assert_eq!(observation.article_ids, vec![normalized.article_id.clone()]);

        let results =
            run_backtests_by_configuration("stage0_fixture", &observations, 0.0, 0.0, 5.0);
        let trade = results
            .iter()
            .flat_map(|result| result.trades.iter())
            .find(|trade| trade.observation_id == observation.observation_id)
            .unwrap();

        assert_eq!(trade.symbol, "SPY");
        assert_eq!(trade.side, "long");
    }
}
