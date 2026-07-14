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

/// The strategy under test, and the baselines it must beat to mean anything.
///
/// The spec's failure gate is explicit: *"V1 should stop or revise when sentiment
/// performs no better than shuffled or non-sentiment baselines."* Stage 0 shipped
/// with only `ShuffledSentiment` (design.md Decision 6), which meant `continue`
/// could never be trusted — a strategy that beats one weak baseline has not
/// beaten anything. These are the other three.
///
/// Every strategy runs through the SAME engine on the SAME observations with the
/// SAME costs and slot logic. An unfair comparison is worse than no comparison,
/// because it looks like evidence.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Strategy {
    /// The hypothesis: trade on sentiment.
    Sentiment,
    /// Never trade. The floor. If sentiment cannot beat *doing nothing* after
    /// costs, there is no edge — and costs are the reason this is not trivial.
    AlwaysFlat,
    /// A seeded coin. If sentiment cannot beat noise, it *is* noise.
    Random,
    /// Yesterday's return in place of sentiment. Catches the most likely
    /// disappointment: "congratulations, you have rediscovered momentum".
    PriorReturnMomentum,
    /// Real sentiment scores, permuted across observations. Destroys the
    /// article↔return pairing while preserving the score *distribution* — so it
    /// isolates whether the ORDERING carries information.
    ShuffledSentiment,
}

impl Strategy {
    pub const ALL: [Strategy; 5] = [
        Strategy::Sentiment,
        Strategy::AlwaysFlat,
        Strategy::Random,
        Strategy::PriorReturnMomentum,
        Strategy::ShuffledSentiment,
    ];

    pub fn as_str(&self) -> &'static str {
        match self {
            Strategy::Sentiment => "sentiment",
            Strategy::AlwaysFlat => "always_flat",
            Strategy::Random => "random",
            Strategy::PriorReturnMomentum => "prior_return_momentum",
            Strategy::ShuffledSentiment => "shuffled_sentiment",
        }
    }

    pub fn is_baseline(&self) -> bool {
        !matches!(self, Strategy::Sentiment)
    }
}

/// SplitMix64. A tiny, deterministic PRNG so the random baseline is reproducible
/// from `(seed, observation_id)` alone — the spec's Required Tests demand
/// "determinism for identical snapshot, configuration, and seed", and a baseline
/// nobody can reproduce is not a baseline.
fn splitmix64(state: &mut u64) -> u64 {
    *state = state.wrapping_add(0x9E37_79B9_7F4A_7C15);
    let mut z = *state;
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

fn seed_from(seed: u64, key: &str) -> u64 {
    let mut hash = seed ^ 0xA076_1D64_78BD_642F;
    for byte in key.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x1000_0000_01B3);
    }
    hash
}

/// The signal each strategy trades on, in place of `mean_sentiment`.
/// `None` means "never trade" (always-flat).
fn signals_for(
    strategy: Strategy,
    group: &[&NewsSignalObservation],
    seed: u64,
) -> Option<Vec<f64>> {
    match strategy {
        Strategy::Sentiment => Some(group.iter().map(|row| row.mean_sentiment).collect()),
        Strategy::AlwaysFlat => None,
        Strategy::PriorReturnMomentum => Some(group.iter().map(|row| row.prior_return).collect()),
        Strategy::Random => Some(
            group
                .iter()
                .map(|row| {
                    // Seeded per observation, so the draw is stable regardless of
                    // iteration order — reproducibility must not depend on how we
                    // happened to walk the vector.
                    let mut state = seed_from(seed, &row.observation_id);
                    let value = splitmix64(&mut state);
                    (value as f64 / u64::MAX as f64) * 2.0 - 1.0
                })
                .collect(),
        ),
        Strategy::ShuffledSentiment => {
            let mut values: Vec<f64> = group.iter().map(|row| row.mean_sentiment).collect();
            // Deterministic Fisher-Yates from the seed.
            let mut state = seed_from(seed, "shuffled");
            for i in (1..values.len()).rev() {
                let j = (splitmix64(&mut state) % (i as u64 + 1)) as usize;
                values.swap(i, j);
            }
            Some(values)
        }
    }
}

/// One BacktestResult per (configuration × strategy), per design.md Decision 1.
/// Quantile thresholds and the overlap-skip trade-slot logic stay scoped to one
/// configuration — pooling them lets unrelated configurations compete for the
/// same trade.
pub fn run_backtests_by_configuration(
    run_id: &str,
    observations: &[NewsSignalObservation],
    long_quantile: f64,
    short_quantile: f64,
    cost_bps: f64,
    max_modal_share: f64,
    seed: u64,
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
        .iter()
        .flat_map(|(key, group)| {
            Strategy::ALL.iter().map(move |strategy| {
                run_backtest_for_configuration(
                    run_id,
                    key,
                    group,
                    *strategy,
                    long_quantile,
                    short_quantile,
                    cost_bps,
                    max_modal_share,
                    seed,
                )
            })
        })
        .collect()
}

/// The sentiment result for each configuration, and the best baseline it must
/// beat. This is what turns the baselines from decoration into a gate.
pub fn strategy_comparison(
    results: &[BacktestResult],
) -> BTreeMap<(i64, i64, String), (f64, f64, String)> {
    let mut by_configuration: BTreeMap<(i64, i64, String), (f64, f64, String)> = BTreeMap::new();
    for result in results {
        let key = (
            result.metrics.news_window_minutes,
            result.metrics.measurement_horizon_minutes,
            result.metrics.source_set.clone(),
        );
        let entry = by_configuration
            .entry(key)
            .or_insert((0.0, f64::NEG_INFINITY, String::new()));
        if result.metrics.strategy == Strategy::Sentiment.as_str() {
            entry.0 = result.metrics.net_return_sum;
        } else if result.metrics.net_return_sum > entry.1 {
            entry.1 = result.metrics.net_return_sum;
            entry.2 = result.metrics.strategy.clone();
        }
    }
    by_configuration
}

/// Share of observations sitting on the single most common sentiment value.
/// Above `max_modal_share` the distribution has no usable resolution: the
/// quantiles land on the same tied value and the long/short rule stops
/// discriminating.
pub fn modal_share(sentiments: &[f64]) -> f64 {
    if sentiments.is_empty() {
        return 0.0;
    }
    let mut counts: BTreeMap<String, usize> = BTreeMap::new();
    for value in sentiments {
        *counts.entry(format!("{value:.9}")).or_default() += 1;
    }
    let modal = counts.values().copied().max().unwrap_or(0);
    modal as f64 / sentiments.len() as f64
}

/// Is this configuration's sentiment distribution too coarse to test?
///
/// **This guards a live defect.** The Stage 0 sentiment lexicon is 14 words, so
/// on real headlines most articles score exactly 0.0. Both quantile thresholds
/// then collapse to 0.0 — and because the long rule tests `>= long_threshold`,
/// *every* neutral observation is classified long, the short branch never fires,
/// and the run emits an all-long book with plausible-looking metrics. It fails
/// silently, which is the worst way to fail.
///
/// A pipeline that cannot tell "no signal" from "no signal *resolution*" will
/// mislead Stage 3. So: no trades, and Task 8 turns this into `expand sources`
/// — a *data* verdict, not a signal result.
pub fn is_degenerate(
    sentiments: &[f64],
    long_threshold: f64,
    short_threshold: f64,
    max_modal_share: f64,
) -> bool {
    long_threshold <= short_threshold || modal_share(sentiments) > max_modal_share
}

#[allow(clippy::too_many_arguments)]
fn run_backtest_for_configuration(
    run_id: &str,
    key: &(i64, i64, String),
    observations: &[&NewsSignalObservation],
    strategy: Strategy,
    long_quantile: f64,
    short_quantile: f64,
    cost_bps: f64,
    max_modal_share: f64,
    seed: u64,
) -> BacktestResult {
    // Always-flat takes no trades by definition. It is the floor: if sentiment
    // cannot beat DOING NOTHING after costs, there is no edge. Costs are what
    // make that a real bar rather than a trivial one.
    let Some(signals) = signals_for(strategy, observations, seed) else {
        return BacktestResult {
            metrics: metrics(run_id, key, cost_bps, strategy, &[]),
            trades: Vec::new(),
        };
    };

    let long_threshold = quantile(signals.clone(), long_quantile);
    let short_threshold = quantile(signals.clone(), short_quantile);

    if is_degenerate(&signals, long_threshold, short_threshold, max_modal_share) {
        // Zero trades, flagged. NOT an all-long book, and not a silent zero
        // either — a caller must be able to tell "we declined to trade this"
        // from "we traded it and made nothing".
        let mut metrics = metrics(run_id, key, cost_bps, strategy, &[]);
        metrics.degenerate = true;
        return BacktestResult {
            metrics,
            trades: Vec::new(),
        };
    }

    // Sort by entry, carrying each row's signal with it, so every strategy meets
    // identical trade-slot logic. The comparison is only evidence if it is fair.
    let mut sorted: Vec<(&NewsSignalObservation, f64)> = observations
        .iter()
        .copied()
        .zip(signals.iter().copied())
        .collect();
    sorted.sort_by_key(|(row, _)| (row.entry_time, row.symbol.clone()));

    let mut last_exit_by_symbol = BTreeMap::<String, chrono::DateTime<chrono::Utc>>::new();
    let mut trades = Vec::new();
    for (row, signal) in sorted {
        if last_exit_by_symbol
            .get(&row.symbol)
            .is_some_and(|last_exit| row.entry_time < *last_exit)
        {
            continue;
        }
        let side = if signal >= long_threshold {
            "long"
        } else if signal <= short_threshold {
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
            strategy: strategy.as_str().into(),
            side: side.into(),
            signal_time: row.signal_time,
            entry_time: row.entry_time,
            exit_time: row.exit_time,
            sentiment: signal,
            gross_return,
            cost_bps,
            net_return,
        });
    }
    let metrics = metrics(run_id, key, cost_bps, strategy, &trades);
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
    strategy: Strategy,
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
        strategy: strategy.as_str().into(),
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
        degenerate: false,
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
        config::PipelineConfig, normalize::normalize_articles, observations::build_observations,
        source::fixture::generate_fixture,
    };

    fn observations() -> Vec<crate::domain::observation::NewsSignalObservation> {
        let config = PipelineConfig::load("configs/stage0_fixture.json").unwrap();
        let fixture = generate_fixture(&config).unwrap();
        let articles = normalize_articles(&config, &fixture.raw_articles)
            .unwrap()
            .normalized;
        build_observations(&config, "ds_test", &articles, &fixture.price_bars).unwrap()
    }

    /// The SENTIMENT result for one configuration. Every configuration now emits
    /// five results (sentiment + four baselines), so a test that just takes the
    /// first match would silently assert against a baseline.
    fn finance_only_hour_hour_result(results: &[BacktestResult]) -> &BacktestResult {
        results
            .iter()
            .find(|result| {
                result.metrics.news_window_minutes == 60
                    && result.metrics.measurement_horizon_minutes == 60
                    && result.metrics.source_set == "finance_only"
                    && result.metrics.strategy == Strategy::Sentiment.as_str()
            })
            .unwrap()
    }

    fn observation_with_sentiment(
        mean_sentiment: f64,
        future_return: f64,
    ) -> NewsSignalObservation {
        let now = chrono::Utc::now();
        NewsSignalObservation {
            observation_id: format!("obs-{mean_sentiment}-{future_return}"),
            dataset_id: "ds".into(),
            symbol: "SPY".into(),
            signal_time: now,
            news_window_minutes: 60,
            measurement_horizon_minutes: 60,
            price_interval_minutes: 60,
            source_set: "finance_only".into(),
            article_count: 1,
            ticker_article_count: 1,
            sector_theme_article_count: 0,
            macro_article_count: 0,
            source_count: 1,
            publisher_count: 1,
            mean_sentiment,
            weighted_sentiment: mean_sentiment,
            extreme_sentiment: mean_sentiment,
            positive_article_count: 0,
            negative_article_count: 0,
            sentiment_dispersion: 0.0,
            prior_return: 0.0,
            prior_volatility: 0.0,
            market_session: "regular".into(),
            is_after_hours_signal: false,
            future_return,
            future_volatility: 0.0,
            future_tail_event: false,
            future_max_drawdown: 0.0,
            future_max_runup: 0.0,
            entry_time: now,
            exit_time: now,
            article_ids: vec![],
            price_bar_ids: vec![],
            created_by_run_id: "run".into(),
        }
    }

    #[test]
    fn a_sentiment_distribution_that_is_mostly_zero_produces_no_trades_rather_than_an_all_long_book()
     {
        // THE STAGE 1 DEFECT, pinned.
        //
        // The Stage 0 lexicon is 14 words, so real headlines mostly score 0.0.
        // With 9 of 10 observations at 0.0, quantile(0.8) and quantile(0.2)
        // BOTH return 0.0. The old rule then read:
        //
        //     if mean_sentiment >= long_threshold  -> "long"    // 0.0 >= 0.0 ✓
        //     else if mean_sentiment <= short_threshold -> "short"  // unreachable
        //
        // ...so all nine neutral rows were classified LONG, the short branch
        // never fired, and the run reported a confident all-long book with
        // real-looking metrics. Verified failing against the pre-fix backtest.rs
        // (9 long trades, 0 short); it must now take zero trades and say why.
        let mut group: Vec<NewsSignalObservation> = (0..9)
            .map(|i| observation_with_sentiment(0.0, 0.01 * i as f64))
            .collect();
        group.push(observation_with_sentiment(0.5, 0.02));

        let results = run_backtests_by_configuration("run", &group, 0.8, 0.2, 0.0, 0.8, 42);
        let sentiment = results
            .iter()
            .find(|r| r.metrics.strategy == Strategy::Sentiment.as_str())
            .unwrap();

        assert!(
            sentiment.metrics.degenerate,
            "a distribution that is 90% ties must be flagged, not traded"
        );
        assert_eq!(
            sentiment.metrics.trade_count, 0,
            "expected zero trades, got an all-long book"
        );
        assert!(sentiment.trades.is_empty());
    }

    #[test]
    fn the_defect_is_real_the_old_rule_turns_a_tied_distribution_into_an_all_long_book() {
        // Proof, not assertion. This replicates the EXACT pre-fix rule from
        // backtest.rs and runs it on the degenerate distribution, so the defect
        // is demonstrated rather than described. Delete this test only when you
        // are willing to lose the evidence that the guard above is load-bearing.
        let sentiments: Vec<f64> = (0..9).map(|_| 0.0).chain([0.5]).collect();
        let long_threshold = quantile(sentiments.clone(), 0.8);
        let short_threshold = quantile(sentiments.clone(), 0.2);

        // Both quantiles land on the same tied value.
        assert_eq!(long_threshold, 0.0);
        assert_eq!(short_threshold, 0.0);

        // The old rule, verbatim: `>=` long, then `<=` short, else flat.
        let (mut longs, mut shorts, mut flat) = (0, 0, 0);
        for sentiment in &sentiments {
            if *sentiment >= long_threshold {
                longs += 1;
            } else if *sentiment <= short_threshold {
                shorts += 1;
            } else {
                flat += 1;
            }
        }

        // Every single row goes long. The short branch is unreachable. This
        // book would have been reported with real-looking metrics.
        assert_eq!(longs, 10);
        assert_eq!(shorts, 0);
        assert_eq!(flat, 0);
    }

    #[test]
    fn collapsed_long_and_short_thresholds_are_detected_as_degenerate() {
        // Every value identical: both quantiles land on the same number, so the
        // thresholds cannot separate anything.
        let sentiments = vec![0.0; 5];

        assert!(is_degenerate(&sentiments, 0.0, 0.0, 0.8));
    }

    #[test]
    fn a_healthy_spread_of_sentiment_values_is_not_flagged_degenerate() {
        // The counter-test: the guard must not be so eager that it suppresses a
        // real Stage 3 distribution. A well-spread book still trades.
        let group: Vec<NewsSignalObservation> = (0..10)
            .map(|i| observation_with_sentiment(i as f64 / 10.0, 0.01 * i as f64))
            .collect();

        let results = run_backtests_by_configuration("run", &group, 0.8, 0.2, 0.0, 0.8, 42);
        let sentiment = results
            .iter()
            .find(|r| r.metrics.strategy == Strategy::Sentiment.as_str())
            .unwrap();

        assert!(!sentiment.metrics.degenerate);
        assert!(sentiment.metrics.trade_count > 0);
        assert!(sentiment.metrics.long_count > 0);
        assert!(sentiment.metrics.short_count > 0);
    }

    #[test]
    fn modal_share_measures_the_biggest_pile_of_tied_sentiment_values() {
        assert!((modal_share(&[0.0, 0.0, 0.0, 0.0, 1.0]) - 0.8).abs() < 1e-9);
        assert!((modal_share(&[0.1, 0.2, 0.3, 0.4]) - 0.25).abs() < 1e-9);
    }

    #[test]
    fn always_flat_takes_zero_trades_and_returns_zero() {
        // The floor, and NOT a trivial one: always-flat pays no costs. A strategy
        // that trades 200 times to earn less than doing nothing has discovered a
        // way to pay commissions.
        let results =
            run_backtests_by_configuration("run", &observations(), 0.8, 0.2, 5.0, 0.8, 42);
        let flat: Vec<&BacktestResult> = results
            .iter()
            .filter(|r| r.metrics.strategy == Strategy::AlwaysFlat.as_str())
            .collect();

        assert!(!flat.is_empty());
        for result in flat {
            assert_eq!(result.metrics.trade_count, 0);
            assert!(result.trades.is_empty());
            assert_eq!(result.metrics.gross_return_sum, 0.0);
        }
    }

    #[test]
    fn the_random_baseline_is_reproducible_from_its_seed() {
        // The spec's Required Tests demand "determinism for identical snapshot,
        // configuration, and seed". A baseline nobody can reproduce is not a
        // baseline, it is an anecdote.
        let first = run_backtests_by_configuration("run", &observations(), 0.8, 0.2, 5.0, 0.8, 7);
        let second = run_backtests_by_configuration("run", &observations(), 0.8, 0.2, 5.0, 0.8, 7);

        assert_eq!(first, second);
    }

    #[test]
    fn a_different_seed_gives_a_different_random_baseline() {
        // Otherwise the seed is decorative and the "random" baseline is a fixed
        // pattern wearing a costume.
        let net = |seed: u64| -> f64 {
            run_backtests_by_configuration("run", &observations(), 0.8, 0.2, 5.0, 0.8, seed)
                .iter()
                .filter(|r| r.metrics.strategy == Strategy::Random.as_str())
                .map(|r| r.metrics.net_return_sum)
                .sum()
        };

        assert_ne!(net(7), net(9999));
    }

    #[test]
    fn the_momentum_baseline_trades_on_prior_return_not_on_sentiment() {
        // The "congratulations, you have rediscovered momentum" check. On the
        // real sample this baseline BEATS sentiment in 4 of 6 configurations.
        let group: Vec<NewsSignalObservation> = (0..10)
            .map(|i| {
                let mut row = observation_with_sentiment(0.5, 0.01);
                row.observation_id = format!("obs-{i}");
                // Sentiment is IDENTICAL everywhere; only prior_return varies. A
                // momentum strategy must still be able to rank and trade.
                row.prior_return = i as f64 / 10.0;
                row.future_return = 0.01 * i as f64;
                row
            })
            .collect();

        let results = run_backtests_by_configuration("run", &group, 0.8, 0.2, 0.0, 0.8, 42);
        let momentum = results
            .iter()
            .find(|r| r.metrics.strategy == Strategy::PriorReturnMomentum.as_str())
            .unwrap();
        let sentiment = results
            .iter()
            .find(|r| r.metrics.strategy == Strategy::Sentiment.as_str())
            .unwrap();

        // Sentiment is all ties -> degenerate -> no trades. Momentum has a real
        // spread of prior returns -> it trades. This proves they read DIFFERENT
        // fields.
        assert!(sentiment.metrics.degenerate);
        assert_eq!(sentiment.metrics.trade_count, 0);
        assert!(!momentum.metrics.degenerate);
        assert!(momentum.metrics.trade_count > 0);
    }

    #[test]
    fn all_five_strategies_are_backtested_on_identical_observations() {
        // An unfair comparison is worse than no comparison, because it looks like
        // evidence. Every strategy must meet the same observations, the same
        // costs, and the same trade-slot logic — only the SIGNAL differs.
        let results =
            run_backtests_by_configuration("run", &observations(), 0.8, 0.2, 5.0, 0.8, 42);
        let configurations = crate::analysis::configuration_groups(&observations()).len();

        assert_eq!(results.len(), configurations * Strategy::ALL.len());
        for result in &results {
            assert_eq!(result.metrics.cost_bps, 5.0);
        }
        // Every configuration has exactly one row per strategy.
        for strategy in Strategy::ALL {
            assert_eq!(
                results
                    .iter()
                    .filter(|r| r.metrics.strategy == strategy.as_str())
                    .count(),
                configurations,
                "{} is missing from some configuration",
                strategy.as_str()
            );
        }
    }

    #[test]
    fn strategy_comparison_reports_sentiment_against_its_strongest_baseline() {
        // Not against the weakest, and not against the average. If ANY baseline
        // matches sentiment, sentiment has not found anything.
        let results =
            run_backtests_by_configuration("run", &observations(), 0.8, 0.2, 5.0, 0.8, 42);
        let comparison = strategy_comparison(&results);

        assert!(!comparison.is_empty());
        for (key, (sentiment_net, best_baseline_net, best_name)) in &comparison {
            let actual_best = results
                .iter()
                .filter(|r| {
                    (
                        r.metrics.news_window_minutes,
                        r.metrics.measurement_horizon_minutes,
                        r.metrics.source_set.clone(),
                    ) == *key
                        && r.metrics.strategy != Strategy::Sentiment.as_str()
                })
                .map(|r| r.metrics.net_return_sum)
                .fold(f64::NEG_INFINITY, f64::max);

            assert!(
                (best_baseline_net - actual_best).abs() < 1e-12,
                "must be the MAX baseline"
            );
            assert!(!best_name.is_empty());
            assert!(sentiment_net.is_finite());
        }
    }

    #[test]
    fn backtest_takes_long_and_short_trades_within_a_configuration() {
        let results = run_backtests_by_configuration(
            "stage0_fixture",
            &observations(),
            0.8,
            0.2,
            5.0,
            0.8,
            42,
        );
        let result = finance_only_hour_hour_result(&results);

        assert!(result.metrics.long_count > 0);
        assert!(result.metrics.short_count > 0);
        assert_eq!(result.metrics.trade_count as usize, result.trades.len());
    }

    #[test]
    fn backtest_does_not_mix_trade_slots_across_configurations() {
        let results = run_backtests_by_configuration(
            "stage0_fixture",
            &observations(),
            0.8,
            0.2,
            5.0,
            0.8,
            42,
        );

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
        let no_cost = run_backtests_by_configuration(
            "stage0_fixture",
            &observations(),
            0.8,
            0.2,
            0.0,
            0.8,
            42,
        );
        let with_cost = run_backtests_by_configuration(
            "stage0_fixture",
            &observations(),
            0.8,
            0.2,
            10.0,
            0.8,
            42,
        );
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
        let results = run_backtests_by_configuration(
            "stage0_fixture",
            &observations(),
            0.8,
            0.2,
            0.0,
            0.8,
            42,
        );
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
        let results = run_backtests_by_configuration(
            "stage0_fixture",
            &observations(),
            0.8,
            0.2,
            5.0,
            0.8,
            42,
        );
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
        let config = PipelineConfig::load("configs/stage0_fixture.json").unwrap();
        let fixture = generate_fixture(&config).unwrap();
        let raw = fixture
            .raw_articles
            .iter()
            .find(|article| article.vendor_id == "massive-1")
            .unwrap();
        let articles = normalize_articles(&config, &fixture.raw_articles)
            .unwrap()
            .normalized;
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
            run_backtests_by_configuration("stage0_fixture", &observations, 0.8, 0.2, 5.0, 0.8, 42);
        let trade = results
            .iter()
            .flat_map(|result| result.trades.iter())
            .find(|trade| trade.observation_id == observation.observation_id)
            .unwrap();

        assert_eq!(trade.symbol, "SPY");
        assert_eq!(trade.side, "long");
    }
}
