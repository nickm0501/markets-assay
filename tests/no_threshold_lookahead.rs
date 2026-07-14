//! The lookahead bug that `assert_no_lookahead` could not see.
//!
//! That check verifies `available_at <= entry_time` for every ARTICLE, and every
//! article was always clean. The leak was in the THRESHOLD: `quantile(signals,
//! 0.8)` was computed across the ENTIRE observation set, so a trade on day 1 used
//! a threshold derived from day 7's sentiment scores. Information from the future
//! decided whether you went long today.
//!
//! The spec has always demanded otherwise: "Quantile thresholds are learned from
//! development data and then frozen."

use markets::{
    backtest::{BacktestParams, Split, Strategy, run_backtests_by_configuration},
    domain::observation::NewsSignalObservation,
};

fn observation(i: usize, sentiment: f64) -> NewsSignalObservation {
    let base = chrono::DateTime::parse_from_rfc3339("2025-07-01T14:00:00Z")
        .unwrap()
        .with_timezone(&chrono::Utc);
    let t = base + chrono::Duration::hours(i as i64);
    NewsSignalObservation {
        observation_id: format!("obs-{i:03}"),
        dataset_id: "ds".into(),
        symbol: format!("SYM{}", i % 10),
        signal_time: t,
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
        mean_sentiment: sentiment,
        weighted_sentiment: sentiment,
        extreme_sentiment: sentiment,
        positive_article_count: 0,
        negative_article_count: 0,
        sentiment_dispersion: 0.0,
        prior_return: 0.0,
        prior_volatility: 0.0,
        market_session: "regular".into(),
        is_after_hours_signal: false,
        future_return: 0.01,
        future_volatility: 0.0,
        future_tail_event: false,
        future_max_drawdown: 0.0,
        future_max_runup: 0.0,
        entry_time: t,
        exit_time: t + chrono::Duration::hours(1),
        article_ids: vec![],
        price_bar_ids: vec![],
        created_by_run_id: "run".into(),
    }
}

#[test]
fn thresholds_are_learned_from_development_only_and_frozen_for_the_holdout() {
    // THE TEST THAT WOULD HAVE CAUGHT IT.
    //
    // First half (development): sentiment in [0.0, 0.1] — a narrow, low band.
    // Second half (holdout):    sentiment in [0.9, 1.0] — wildly different.
    //
    // If thresholds were fitted over the WHOLE sample (the old bug), the 80th
    // percentile would land up in the holdout's range (~0.9), and almost nothing
    // in development would qualify as "long".
    //
    // Correctly frozen from development, the threshold sits at ~0.08 — so EVERY
    // holdout observation blows past it. That asymmetry is the fingerprint.
    let mut observations = Vec::new();
    for i in 0..100 {
        observations.push(observation(i, (i as f64 / 99.0) * 0.1)); // dev: 0.0 -> 0.1
    }
    for i in 100..200 {
        observations.push(observation(i, 0.9 + ((i - 100) as f64 / 99.0) * 0.1)); // holdout: 0.9 -> 1.0
    }

    let results = run_backtests_by_configuration(
        "t",
        &observations,
        BacktestParams {
            long_quantile: 0.8,
            short_quantile: 0.2,
            cost_bps: 0.0,
            max_modal_share: 0.95,
            seed: 42,
            development_fraction: 0.5,
        },
    );

    let dev = results
        .iter()
        .find(|r| {
            r.metrics.strategy == Strategy::Sentiment.as_str()
                && r.metrics.split == Split::Development.as_str()
        })
        .expect("a development result must exist");
    let holdout = results
        .iter()
        .find(|r| {
            r.metrics.strategy == Strategy::Sentiment.as_str()
                && r.metrics.split == Split::Holdout.as_str()
        })
        .expect("a holdout result must exist");

    // Development trades both sides: the threshold was fitted to ITS OWN spread.
    assert!(dev.metrics.long_count > 0, "development must take longs");
    assert!(dev.metrics.short_count > 0, "development must take shorts");

    // The holdout sits entirely ABOVE the frozen long threshold, so every holdout
    // trade is a long and NONE is a short. Under the old whole-sample fitting this
    // would have been impossible — the threshold would have been dragged up into
    // the holdout's own range, which is precisely the lookahead.
    assert!(holdout.metrics.long_count > 0, "holdout must take longs");
    assert_eq!(
        holdout.metrics.short_count, 0,
        "the holdout's sentiment is ALL above the frozen threshold; a short here \
         would mean the threshold saw the holdout's own distribution — lookahead"
    );

    // And every holdout trade is stamped as such, so nobody can confuse the two.
    for trade in &holdout.trades {
        assert_eq!(trade.split, "holdout");
    }
}

#[test]
fn the_split_is_chronological_never_random() {
    // A random split would leak the future into the past just as surely as no
    // split at all. Time only runs one way.
    let observations: Vec<NewsSignalObservation> =
        (0..100).map(|i| observation(i, i as f64 / 100.0)).collect();

    let results = run_backtests_by_configuration(
        "t",
        &observations,
        BacktestParams {
            long_quantile: 0.8,
            short_quantile: 0.2,
            cost_bps: 0.0,
            max_modal_share: 0.95,
            seed: 42,
            development_fraction: 0.5,
        },
    );

    let latest_dev = results
        .iter()
        .filter(|r| r.metrics.split == Split::Development.as_str())
        .flat_map(|r| r.trades.iter())
        .map(|t| t.signal_time)
        .max();
    let earliest_holdout = results
        .iter()
        .filter(|r| r.metrics.split == Split::Holdout.as_str())
        .flat_map(|r| r.trades.iter())
        .map(|t| t.signal_time)
        .min();

    if let (Some(latest), Some(earliest)) = (latest_dev, earliest_holdout) {
        assert!(
            latest < earliest,
            "every development observation must precede every holdout observation: \
             dev ends {latest}, holdout starts {earliest}"
        );
    }
}

/// Vendor sentiment is a BENCHMARK, never a signal (design.md Decision 21).
///
/// If it ever reaches the strategy, the research question silently changes from
/// "does news sentiment predict returns" to "does Massive's black box predict
/// returns" — a question we cannot reproduce, version, explain, or debug, and
/// whose answer the vendor could invalidate by changing their model mid-experiment.
#[test]
fn vendor_sentiment_never_reaches_the_backtest() {
    let backtest = std::fs::read_to_string("src/backtest.rs").unwrap();

    assert!(
        !backtest.contains("vendor_sentiment"),
        "backtest.rs must never read vendor_sentiment — it is a yardstick, not a signal"
    );
}
