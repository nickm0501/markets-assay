//! The permutation null must be valid on the observations REAL data produces —
//! not just on the independent ones `power_check.rs` tests.
//!
//! Real observations are not exchangeable: `normalize.rs` fans one macro article
//! out to every macro symbol, a multi-bar news window puts the same article in
//! consecutive observations, and a multi-bar horizon makes adjacent
//! `future_return`s overlap. An i.i.d. permutation shuffles single observations,
//! which destroys that structure — so its p95 sits far too low on realistic
//! noise, and the significance gate fired ~30% of the time on PURE NOISE built
//! with exactly this shape (full `continue` ~22%).
//!
//! This started life as an adversarial probe that FAILED, demonstrating the bug.
//! `analysis::shuffled_spread` now permutes whole time-BLOCKS spanning
//! (news_window + horizon), keeping same-time symbols together, so the null
//! carries the same autocorrelation the data does and p95 rises to the true
//! effective sample size. These assertions now PASS; they are the regression
//! guard that keeps it that way.

use markets::{
    analysis::{AnalysisContext, analyze_observations},
    backtest::{BacktestParams, run_backtests_by_configuration, strategy_comparison},
    config::VerdictThresholds,
    domain::observation::NewsSignalObservation,
};
use std::collections::BTreeMap;

fn splitmix(state: &mut u64) -> f64 {
    *state = state.wrapping_add(0x9E37_79B9_7F4A_7C15);
    let mut z = *state;
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^= z >> 31;
    (z as f64 / u64::MAX as f64) * 2.0 - 1.0
}

fn observation(i: usize, symbol: &str, sentiment: f64, future_return: f64) -> NewsSignalObservation {
    let base = chrono::DateTime::parse_from_rfc3339("2025-07-01T14:00:00Z")
        .unwrap()
        .with_timezone(&chrono::Utc);
    let t = base + chrono::Duration::hours(i as i64);
    NewsSignalObservation {
        observation_id: format!("obs-{symbol}-{i:04}"),
        dataset_id: "ds".into(),
        symbol: symbol.into(),
        signal_time: t,
        news_window_minutes: 240,
        measurement_horizon_minutes: 240,
        price_interval_minutes: 60,
        source_set: "finance_only".into(),
        article_count: 4,
        ticker_article_count: 0,
        sector_theme_article_count: 0,
        macro_article_count: 4,
        source_count: 2,
        publisher_count: 2,
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
        future_return,
        future_volatility: 0.0,
        future_tail_event: false,
        future_max_drawdown: 0.0,
        future_max_runup: 0.0,
        entry_time: t,
        exit_time: t + chrono::Duration::hours(4),
        article_ids: vec![format!("art-{}", i)],
        price_bar_ids: vec![],
        created_by_run_id: "adv".into(),
    }
}

/// Pure noise with the REAL correlation structure:
/// - one hourly "article" sentiment stream; each observation's mean_sentiment
///   averages the last 4 articles (240-min window over hourly bars)
/// - hourly market shocks; each future_return sums the next 4 shocks
///   (240-min horizon over hourly bars)
/// - the same sentiment and near-identical return fanned to 4 symbols
///   (macro fan-out; index ETFs are ~perfectly correlated)
fn clustered_noise(seed: u64, hours: usize) -> Vec<NewsSignalObservation> {
    let mut article_state = seed.wrapping_mul(0xA5A5_5A5A_1234_5678);
    let mut market_state = seed.wrapping_mul(0x0FED_CBA9_8765_4321).wrapping_add(7);
    let articles: Vec<f64> = (0..hours + 4).map(|_| splitmix(&mut article_state)).collect();
    let shocks: Vec<f64> = (0..hours + 4)
        .map(|_| splitmix(&mut market_state) * 0.003)
        .collect();

    let mut rows = Vec::new();
    for t in 4..hours {
        let sentiment = (articles[t - 3..=t].iter().sum::<f64>()) / 4.0;
        let market_return: f64 = shocks[t..t + 4].iter().sum();
        for (s, symbol) in ["SPY", "QQQ", "DIA", "IWM"].iter().enumerate() {
            let mut idio_state = seed
                .wrapping_mul(0x1111_2222_3333_4444)
                .wrapping_add((t * 7 + s) as u64);
            let idio = splitmix(&mut idio_state) * 0.0003;
            rows.push(observation(t, symbol, sentiment, market_return + idio));
        }
    }
    rows
}

/// The control: same size, but every observation is INDEPENDENT — its own
/// article, its own non-overlapping return. This is the structure
/// `power_check.rs` tests, and the permutation null's actual assumption.
fn independent_noise(seed: u64, n: usize) -> Vec<NewsSignalObservation> {
    let mut article_state = seed.wrapping_mul(0xA5A5_5A5A_1234_5678);
    let mut market_state = seed.wrapping_mul(0x0FED_CBA9_8765_4321).wrapping_add(7);
    (0..n)
        .map(|i| {
            let sentiment = splitmix(&mut article_state);
            let ret = splitmix(&mut market_state) * 0.006;
            observation(i, &format!("SYM{}", i % 20), sentiment, ret)
        })
        .collect()
}

fn context_for(observations: &[NewsSignalObservation]) -> AnalysisContext {
    let results = run_backtests_by_configuration(
        "adv",
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

fn false_positive_rates(make: impl Fn(u64) -> Vec<NewsSignalObservation>) -> (f64, f64) {
    let trials = 100;
    let mut significance_hits = 0;
    let mut continues = 0;
    for seed in 0..trials {
        let observations = make(seed as u64 + 1);
        let context = context_for(&observations);
        let summaries = analyze_observations(&observations, &context).unwrap();
        assert_eq!(summaries.len(), 1);
        let s = &summaries[0];
        let margin = s.observed_top_minus_bottom - s.shuffled_p95;
        if margin > 0.0001 && s.observed_top_minus_bottom > 0.0001 {
            significance_hits += 1;
        }
        if s.recommendation == "continue" {
            continues += 1;
        }
    }
    (
        significance_hits as f64 / trials as f64,
        continues as f64 / trials as f64,
    )
}

#[test]
fn the_significance_gate_holds_on_independent_noise_the_nulls_assumption() {
    let (sig, cont) = false_positive_rates(|seed| independent_noise(seed, 320));
    println!(
        "\nINDEPENDENT noise (the null's assumption): significance-gate fires {:.0}%, full continue {:.0}%",
        sig * 100.0,
        cont * 100.0
    );
    assert!(
        sig <= 0.12,
        "even on independent noise the gate fires {:.0}%",
        sig * 100.0
    );
}

#[test]
fn the_block_permutation_null_holds_on_noise_with_the_real_correlation_structure() {
    // The finding that was once a FAILURE. With an i.i.d. shuffle this fired ~30%
    // on pure noise; the block-permutation null must bring it back to ~5%.
    let (sig, cont) = false_positive_rates(|seed| clustered_noise(seed, 84));
    println!(
        "\nCLUSTERED noise (the structure real data has): significance-gate fires {:.0}%, full continue {:.0}%",
        sig * 100.0,
        cont * 100.0
    );
    assert!(
        sig <= 0.12,
        "the significance gate fires {:.0}% on pure noise with realistic clustering — the block-permutation null is not holding",
        sig * 100.0
    );
    assert!(
        cont <= 0.12,
        "the full verdict says continue {:.0}% of the time on pure clustered noise",
        cont * 100.0
    );
}
