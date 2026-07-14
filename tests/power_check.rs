//! Can this pipeline detect a signal that IS there?
//!
//! Every result so far has been `revise`. That is reassuring only if the pipeline
//! CAN say `continue` — a detector hardcoded to `revise` would also produce a
//! clean run, and would also pass every test we have. This plants a strong,
//! unambiguous signal and checks the machine finds it.

use markets::{
    analysis::{AnalysisContext, analyze_observations},
    backtest::{BacktestParams, run_backtests_by_configuration, strategy_comparison},
    config::VerdictThresholds,
    domain::observation::NewsSignalObservation,
};
use std::collections::BTreeMap;

/// Genuinely uncorrelated noise.
///
/// The first version of this used `((i * 6151) % 100)`, which is a STRUCTURED
/// sequence, not noise — and since `mean_sentiment` is also a function of `i`,
/// the "noise" was correlated with the signal by construction. It made the
/// pipeline look like it was crying wolf when the test was lying to it. Bad noise
/// is worse than no control.
fn noise(i: usize, salt: u64) -> f64 {
    let mut z = (i as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ salt;
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^= z >> 31;
    (z as f64 / u64::MAX as f64) * 0.04 - 0.02
}

/// n observations where sentiment PERFECTLY predicts the next hour's return.
/// If the pipeline cannot find this, it cannot find anything.
fn planted_signal(n: usize, strength: f64) -> Vec<NewsSignalObservation> {
    let base = chrono::DateTime::parse_from_rfc3339("2025-07-01T14:00:00Z")
        .unwrap()
        .with_timezone(&chrono::Utc);
    (0..n)
        .map(|i| {
            // Sentiment spread evenly across [-1, 1].
            let sentiment = (i as f64 / (n - 1) as f64) * 2.0 - 1.0;
            NewsSignalObservation {
                observation_id: format!("obs-{i}"),
                dataset_id: "ds".into(),
                // Distinct symbols + times so the one-trade-per-symbol slot rule
                // does not suppress trades and confound the test.
                symbol: format!("SYM{}", i % 20),
                signal_time: base + chrono::Duration::hours(i as i64),
                news_window_minutes: 60,
                measurement_horizon_minutes: 60,
                price_interval_minutes: 60,
                source_set: "finance_only".into(),
                article_count: 3,
                ticker_article_count: 3,
                sector_theme_article_count: 0,
                macro_article_count: 0,
                source_count: 2,
                publisher_count: 2,
                mean_sentiment: sentiment,
                weighted_sentiment: sentiment,
                extreme_sentiment: sentiment,
                positive_article_count: 0,
                negative_article_count: 0,
                sentiment_dispersion: 0.0,
                // Deliberately UNCORRELATED with the future return, so the
                // momentum baseline has nothing to find and cannot win by luck.
                prior_return: noise(i, 0xBEEF),
                prior_volatility: 0.0,
                market_session: "regular".into(),
                is_after_hours_signal: false,
                // THE PLANT: future return is sentiment * strength.
                future_return: sentiment * strength,
                future_volatility: 0.0,
                future_tail_event: false,
                future_max_drawdown: 0.0,
                future_max_runup: 0.0,
                entry_time: base + chrono::Duration::hours(i as i64),
                exit_time: base + chrono::Duration::hours(i as i64 + 1),
                article_ids: vec![format!("art-{i}")],
                price_bar_ids: vec![],
                created_by_run_id: "power".into(),
            }
        })
        .collect()
}

fn context_for(observations: &[NewsSignalObservation]) -> AnalysisContext {
    let results = run_backtests_by_configuration(
        "power",
        observations,
        BacktestParams {
            long_quantile: 0.8,
            short_quantile: 0.2,
            cost_bps: 5.0,
            max_modal_share: 0.8,
            seed: 42,
            development_fraction: 0.5,
        },
    );
    AnalysisContext {
        quarantine_rate: 0.0,
        lexicon_hit_rate: 0.9,
        vendor_agreement: 0.5,
        expected_sources: BTreeMap::from([("finance_only".to_string(), 2)]),
        article_sources: observations
            .iter()
            .flat_map(|o| o.article_ids.iter().cloned())
            .enumerate()
            .map(|(i, id)| (id, format!("pub{}", i % 2)))
            .collect(),
        strategy_nets: strategy_comparison(&results),
        long_quantile: 0.8,
        short_quantile: 0.2,
        max_modal_share: 0.8,
        seed: 42,
        development_fraction: 0.5,
        thresholds: VerdictThresholds::default(),
    }
}

#[test]
fn the_pipeline_says_continue_when_a_real_signal_is_actually_there() {
    // 200 observations, sentiment perfectly predicting a 2% move. This is a
    // signal a blind man could find. If the answer is not `continue`, the gates
    // are broken in the FALSE-NEGATIVE direction — and a Stage 3 run would report
    // "no signal" on two years of data while the detector was simply dead.
    let observations = planted_signal(200, 0.02);
    let context = context_for(&observations);

    let summaries = analyze_observations(&observations, &context);

    assert_eq!(summaries.len(), 1);
    let summary = &summaries[0];
    assert_eq!(
        summary.recommendation, "continue",
        "THE DETECTOR IS DEAD: a perfect planted signal produced '{}' ({})",
        summary.recommendation, summary.reason
    );
    assert!(summary.sentiment_net_return > summary.best_baseline_net_return);
}

#[test]
fn the_pipeline_says_revise_when_the_same_machinery_sees_pure_noise() {
    // The control. Same shape, same size, same gates — but the future return is
    // uncorrelated with sentiment. This must NOT be `continue`, or the test above
    // proves nothing (a detector that always says `continue` would pass it).
    let mut observations = planted_signal(200, 0.02);
    for (i, row) in observations.iter_mut().enumerate() {
        // Scramble the pairing: sentiment no longer predicts the return.
        row.future_return = noise(i, 0xC0FFEE);
    }
    let context = context_for(&observations);

    let summaries = analyze_observations(&observations, &context);

    assert_ne!(
        summaries[0].recommendation, "continue",
        "the detector cried wolf on pure noise: {}",
        summaries[0].reason
    );
}

/// How weak a signal can this pipeline actually see?
///
/// A detector that only finds perfect correlations is useless — real sentiment
/// edges, if they exist at all, are faint. This sweeps the plant strength and
/// prints where the verdict flips, which is the pipeline's DETECTION FLOOR.
///
/// Run with `cargo test --test power_check -- --nocapture detection_floor`.
#[test]
fn detection_floor_sweep() {
    println!("\n  n     signal->return   verdict     sentiment_net  best_baseline");
    println!("  {}", "-".repeat(66));
    for n in [50usize, 100, 200, 500] {
        for corr in [0.0, 0.05, 0.1, 0.25, 0.5, 1.0] {
            let mut observations = planted_signal(n, 0.02);
            // Blend the planted signal with noise: corr=1.0 is perfect,
            // corr=0.0 is pure noise.
            for (i, row) in observations.iter_mut().enumerate() {
                let n = noise(i, 0xC0FFEE);
                row.future_return = corr * (row.mean_sentiment * 0.02) + (1.0 - corr) * n;
            }
            let context = context_for(&observations);
            let s = &analyze_observations(&observations, &context)[0];
            println!(
                "  {n:<5} {corr:<15.2} {:<11} {:>13.5} {:>14.5}",
                s.recommendation, s.sentiment_net_return, s.best_baseline_net_return
            );
        }
        println!();
    }
}
