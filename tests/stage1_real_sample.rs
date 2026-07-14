//! Stage 1's Required Test: one REAL article traced from the vendor's raw JSON
//! all the way through to the trade it caused.
//!
//! The spec lists "a manually traceable observation from raw article through
//! trade" under Required Tests. Stage 0 did it on synthetic data, where every
//! number was one we had chosen ourselves. Doing it on real data is the point of
//! Stage 1: it is the only check that the whole chain — vendor payload, timestamp
//! deferral, news window, sentiment score, entry bar, return, trade side — is
//! wired together correctly rather than merely passing its own unit tests.

use markets::{
    backtest::run_backtests_by_configuration,
    config::PipelineConfig,
    normalize::normalize_articles,
    observations::build_observations,
    source::{NewsSource, PriceSource, saved_files::SavedFileSource},
};
use std::path::Path;

/// The article. Real, fetched 2026-07-14, committed in `fixtures/saved_sample/`.
///
/// Motley Fool, "Which AI Stocks May Soar After Reaching Record Highs?",
/// tagged [AVGO], published 2025-07-01T08:10:00Z.
const VENDOR_ID: &str = "588f94412297a756f73826c044c2ea7a20dafa963d07e629bed24d98e587faac";

#[test]
fn trace_one_real_article_from_vendor_json_through_to_its_trade() {
    let config = PipelineConfig::load("configs/stage1_saved_sample.json").unwrap();
    let source = SavedFileSource::new(Path::new("fixtures/saved_sample")).unwrap();

    // ---- 1. RAW: the vendor's own JSON, unmodified. -----------------------
    let raw_articles = source.fetch_raw_articles(&config).unwrap();
    let raw = raw_articles
        .iter()
        .find(|article| article.vendor_id == VENDOR_ID)
        .expect("the traced article must be in the saved sample");

    assert_eq!(raw.source, "The Motley Fool");
    assert_eq!(raw.tickers, vec!["AVGO"]);
    // Published 04:10 ET — overnight, hours before the market opened.
    assert_eq!(raw.published_at_raw, "2025-07-01T08:10:00Z");

    // ---- 2. NORMALIZED: deferred to the next tradable moment. -------------
    let outcome = normalize_articles(&config, &raw_articles).unwrap();
    let article = outcome
        .normalized
        .iter()
        .find(|a| a.vendor_id == VENDOR_ID)
        .expect("a well-formed article must survive normalization");

    // THE DEFERRAL. Published 08:10Z, but the market was shut. It cannot inform
    // a signal until the session opens at 13:30Z (09:30 ET). This is design.md
    // Decision 4's `available_at` doing its job on real data — and it is the
    // common case, not an edge case: 79% of articles in this sample are
    // published outside the regular session.
    assert_eq!(
        article.published_at.to_rfc3339(),
        "2025-07-01T08:10:00+00:00"
    );
    assert_eq!(
        article.available_at.to_rfc3339(),
        "2025-07-01T13:30:00+00:00",
        "an overnight article must wait for the session open"
    );

    // Strongly positive: +0.527 under LM+VADER (design.md Decision 20).
    //
    // Worth dwelling on, because the lesson CHANGED when the scorer improved.
    // The old 14-word lexicon scored this +0.5 by matching two tokens ("growth",
    // "strong") out of a paragraph — a lucky guess. The new scorer reads the
    // whole thing and lands in almost the same place, because the article
    // genuinely IS bullish: "Which AI Stocks May Soar After Reaching Record
    // Highs?", "rebounded with optimism about future growth".
    //
    // So this is no longer a story about a stupid scorer. A GOOD scorer reads
    // this as positive, the strategy goes long, and AVGO falls 2.1% in the hour.
    // That is not a bug in the reader — that is what it looks like when upbeat
    // news carries no predictive information. Which is exactly the hypothesis
    // under test, and exactly why baselines and an untouched holdout exist.
    assert!(
        article.sentiment_score > 0.4,
        "expected a strongly positive read, got {}",
        article.sentiment_score
    );

    // ---- 3. OBSERVATION: the signal that used it. -------------------------
    let price_bars = source.fetch_price_bars(&config).unwrap();
    let observations =
        build_observations(&config, "ds_trace", &outcome.normalized, &price_bars).unwrap();

    // Signal at 14:00Z (10:00 ET) — a regular-session bar. With a 60-minute
    // news window, the window is (13:00Z, 14:00Z], and available_at 13:30Z falls
    // inside it. The article reaches this signal *because* it was deferred, not
    // despite it.
    let observation = observations
        .iter()
        .find(|row| {
            row.symbol == "AVGO"
                && row.source_set == "finance_only"
                && row.news_window_minutes == 60
                && row.measurement_horizon_minutes == 60
                && row.signal_time.to_rfc3339() == "2025-07-01T14:00:00+00:00"
        })
        .expect("the 14:00Z AVGO signal must exist");

    // It is the ONLY eligible article, so the observation's mean sentiment is
    // exactly this article's score. That is what makes the trace unambiguous.
    assert_eq!(observation.article_ids, vec![article.article_id.clone()]);
    assert!((observation.mean_sentiment - article.sentiment_score).abs() < 1e-9);

    // No lookahead: the article was usable strictly before we traded on it.
    assert!(article.available_at <= observation.entry_time);
    assert_eq!(observation.entry_time, observation.signal_time);

    // The measured return, straight off the real 14:00Z bar:
    //   open 269.69 -> close 263.95  =>  (263.95 - 269.69) / 269.69 = -0.02128
    assert!(
        (observation.future_return - (263.95 - 269.69) / 269.69).abs() < 1e-9,
        "future_return must equal the real bar's open-to-close move"
    );

    // ---- 4. TRADE: what the strategy actually did. ------------------------
    let results = run_backtests_by_configuration(
        "trace",
        &observations,
        config.long_quantile,
        config.short_quantile,
        10.0,
        config.max_modal_share,
    );
    let trade = results
        .iter()
        .flat_map(|result| result.trades.iter())
        .find(|trade| trade.observation_id == observation.observation_id)
        .expect("a +0.5 sentiment observation must clear the long threshold");

    assert_eq!(trade.symbol, "AVGO");
    assert_eq!(trade.side, "long");
    // Long, so the gross return IS the future return. The strategy read an
    // upbeat headline, bought, and AVGO fell 2.1% in the hour.
    assert!((trade.gross_return - observation.future_return).abs() < 1e-9);
    assert!(trade.gross_return < 0.0);
    // Cost is subtracted, never added, for a long.
    assert!((trade.net_return - (trade.gross_return - 10.0 / 10_000.0)).abs() < 1e-9);
}

/// The empirical claim the Stage 2 plan demanded, pinned.
///
/// Stage 1 measured `lexicon_hit_rate = 0.2021` — the 14-word scorer read one
/// real headline in five. The bake-off promised LM+VADER would fix that. This is
/// where the promise is kept or broken: if it fails, the scorer regressed and no
/// downstream number can be trusted.
#[test]
fn the_new_scorer_reads_most_of_the_real_sample() {
    let config = PipelineConfig::load("configs/stage1_saved_sample.json").unwrap();
    let source = SavedFileSource::new(Path::new("fixtures/saved_sample")).unwrap();
    let raw = source.fetch_raw_articles(&config).unwrap();
    let articles = normalize_articles(&config, &raw).unwrap().normalized;

    let hits = articles
        .iter()
        .filter(|a| markets::sentiment::has_lexicon_hit(&format!("{} {}", a.title, a.summary)))
        .count();
    let rate = hits as f64 / articles.len() as f64;

    assert!(
        rate > 0.60,
        "lexicon_hit_rate regressed to {rate:.4}; Stage 1 measured 0.2021 with the \
         14-word lexicon and the bake-off promised >0.60"
    );
}

/// Half of the corpus is GDELT: title only, ~11 words. The old scorer was 94%
/// blind on it. The new one must actually read a real one.
#[test]
fn an_eleven_word_gdelt_headline_still_gets_a_real_score() {
    // Verbatim from fixtures/saved_sample/gdelt_macro.json.
    let headline = "CNBC Daily Open : Tariffs led us down a different timeline for interest rates";

    assert!(markets::sentiment::has_lexicon_hit(headline));
    assert_ne!(markets::sentiment::score_text(headline).score, 0.0);
}
