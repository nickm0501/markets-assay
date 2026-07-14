use crate::{
    calendar::is_regular_session,
    config::Stage0Config,
    domain::{
        article::{NewsScope, NormalizedArticle, SentimentLabel, SourceKind},
        market::PriceBar,
        observation::NewsSignalObservation,
    },
    ids::stable_id,
};
use anyhow::Result;
use chrono::Duration;
use std::collections::BTreeSet;

pub fn build_observations(
    config: &Stage0Config,
    dataset_id: &str,
    articles: &[NormalizedArticle],
    price_bars: &[PriceBar],
) -> Result<Vec<NewsSignalObservation>> {
    let mut observations = Vec::new();
    for bar in price_bars {
        for news_window_minutes in &config.news_windows_minutes {
            for measurement_horizon_minutes in &config.measurement_horizons_minutes {
                for source_set in &config.source_sets {
                    if let Some(row) = build_one(
                        config,
                        dataset_id,
                        articles,
                        price_bars,
                        bar,
                        *news_window_minutes,
                        *measurement_horizon_minutes,
                        source_set,
                    )? {
                        observations.push(row);
                    }
                }
            }
        }
    }
    observations.sort_by_key(|row| {
        (
            row.signal_time,
            row.symbol.clone(),
            row.news_window_minutes,
            row.source_set.clone(),
        )
    });
    Ok(observations)
}

#[allow(clippy::too_many_arguments)]
fn build_one(
    config: &Stage0Config,
    dataset_id: &str,
    articles: &[NormalizedArticle],
    price_bars: &[PriceBar],
    signal_bar: &PriceBar,
    news_window_minutes: i64,
    measurement_horizon_minutes: i64,
    source_set: &str,
) -> Result<Option<NewsSignalObservation>> {
    let signal_time = signal_bar.start_time;
    let window_start = signal_time - Duration::minutes(news_window_minutes);
    let eligible: Vec<&NormalizedArticle> = articles
        .iter()
        .filter(|article| {
            article.available_at > window_start && article.available_at <= signal_time
        })
        .filter(|article| article.relevant_symbols.contains(&signal_bar.symbol))
        .filter(|article| source_set_includes(source_set, article))
        .collect();
    if eligible.is_empty() {
        return Ok(None);
    }
    // Sum every contiguous price bar from signal_time through exit_time so
    // horizons wider than one price_interval (e.g. spec's "next four hours"
    // over 1h bars) are measured correctly instead of silently producing no
    // observation. If the bars covering the window aren't fully contiguous
    // (e.g. a holiday or session close truncates it), the observation is
    // dropped rather than aggregated over a partial, misleading window.
    let exit_time = signal_time + Duration::minutes(measurement_horizon_minutes);
    let mut future_bars: Vec<&PriceBar> = price_bars
        .iter()
        .filter(|bar| {
            bar.symbol == signal_bar.symbol
                && bar.start_time >= signal_time
                && bar.end_time <= exit_time
        })
        .collect();
    future_bars.sort_by_key(|bar| bar.start_time);
    let spans_full_horizon = future_bars
        .first()
        .is_some_and(|bar| bar.start_time == signal_time)
        && future_bars
            .last()
            .is_some_and(|bar| bar.end_time == exit_time)
        && future_bars
            .windows(2)
            .all(|pair| pair[0].end_time == pair[1].start_time);
    if !spans_full_horizon {
        return Ok(None);
    }
    let future_open = future_bars.first().unwrap().open;
    let future_close = future_bars.last().unwrap().close;
    let future_high = future_bars
        .iter()
        .map(|bar| bar.high)
        .fold(f64::MIN, f64::max);
    let future_low = future_bars
        .iter()
        .map(|bar| bar.low)
        .fold(f64::MAX, f64::min);
    let prior_bar = price_bars
        .iter()
        .filter(|bar| bar.symbol == signal_bar.symbol && bar.end_time <= signal_time)
        .max_by_key(|bar| bar.end_time);
    let article_count = eligible.len() as u32;
    let mean_sentiment = eligible
        .iter()
        .map(|article| article.sentiment_score)
        .sum::<f64>()
        / article_count as f64;
    let dispersion = eligible
        .iter()
        .map(|article| (article.sentiment_score - mean_sentiment).abs())
        .sum::<f64>()
        / article_count as f64;
    // Sign-preserving: the single eligible article whose score is furthest
    // from zero, not just the largest magnitude on its own.
    let extreme_sentiment = eligible
        .iter()
        .map(|article| article.sentiment_score)
        .max_by(|a, b| a.abs().partial_cmp(&b.abs()).unwrap())
        .unwrap_or(0.0);
    let sources: BTreeSet<_> = eligible
        .iter()
        .map(|article| article.source.clone())
        .collect();
    let article_ids: Vec<_> = eligible
        .iter()
        .map(|article| article.article_id.clone())
        .collect();
    let price_bar_ids: Vec<String> = future_bars.iter().map(|bar| bar.bar_id.clone()).collect();
    let observation_id = stable_id(
        "sig",
        &(
            dataset_id,
            signal_bar.symbol.as_str(),
            signal_time,
            news_window_minutes,
            measurement_horizon_minutes,
            source_set,
            &article_ids,
        ),
    )?;
    let future_return = (future_close - future_open) / future_open;
    // Computed rather than stubbed so the fixture's after-hours article
    // (deferred available_at != published_at) actually flows through to the
    // observation instead of being silently discarded.
    let is_after_hours_signal = eligible
        .iter()
        .any(|article| article.available_at != article.published_at);
    let market_session = if is_regular_session(signal_time, &config.holidays, &config.early_closes)
    {
        "regular"
    } else {
        "after_hours"
    }
    .to_string();

    Ok(Some(NewsSignalObservation {
        observation_id,
        dataset_id: dataset_id.into(),
        symbol: signal_bar.symbol.clone(),
        signal_time,
        news_window_minutes,
        measurement_horizon_minutes,
        price_interval_minutes: config.price_interval_minutes,
        source_set: source_set.into(),
        article_count,
        ticker_article_count: eligible
            .iter()
            .filter(|article| article.scope == NewsScope::TickerSpecific)
            .count() as u32,
        sector_theme_article_count: eligible
            .iter()
            .filter(|article| article.scope == NewsScope::SectorTheme)
            .count() as u32,
        macro_article_count: eligible
            .iter()
            .filter(|article| article.scope == NewsScope::MacroMarket)
            .count() as u32,
        source_count: sources.len() as u32,
        publisher_count: sources.len() as u32,
        mean_sentiment,
        weighted_sentiment: mean_sentiment,
        extreme_sentiment,
        positive_article_count: eligible
            .iter()
            .filter(|article| article.sentiment_label == SentimentLabel::Positive)
            .count() as u32,
        negative_article_count: eligible
            .iter()
            .filter(|article| article.sentiment_label == SentimentLabel::Negative)
            .count() as u32,
        sentiment_dispersion: dispersion,
        prior_return: prior_bar.map(|bar| bar.return_pct()).unwrap_or(0.0),
        prior_volatility: prior_bar.map(|bar| bar.high / bar.low - 1.0).unwrap_or(0.0),
        market_session,
        is_after_hours_signal,
        future_return,
        future_volatility: future_high / future_low - 1.0,
        future_tail_event: future_return.abs() >= 0.006,
        future_max_drawdown: (future_low - future_open) / future_open,
        future_max_runup: (future_high - future_open) / future_open,
        entry_time: signal_time,
        exit_time,
        article_ids,
        price_bar_ids,
        created_by_run_id: config.run_id.clone(),
    }))
}

fn source_set_includes(source_set: &str, article: &NormalizedArticle) -> bool {
    match source_set {
        "finance_only" => article.source_kind == SourceKind::Finance,
        "broad_news" => article.source_kind == SourceKind::Broad,
        "finance_plus_broad" => true,
        other => {
            eprintln!("unknown source_set={other}");
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{config::Stage0Config, fixture::generate_fixture, normalize::normalize_articles};

    fn config() -> Stage0Config {
        Stage0Config::load("configs/stage0_fixture.json").unwrap()
    }

    #[test]
    fn observations_include_one_row_per_symbol_signal_window_horizon_source_set() {
        let config = config();
        let fixture = generate_fixture(&config).unwrap();
        let articles = normalize_articles(&config, &fixture.raw_articles).unwrap();
        let observations =
            build_observations(&config, "ds_test", &articles, &fixture.price_bars).unwrap();

        assert!(observations.iter().any(|row| row.symbol == "SPY"
            && row.news_window_minutes == 60
            && row.source_set == "finance_only"));
        assert!(observations.iter().any(|row| row.symbol == "QQQ"
            && row.news_window_minutes == 240
            && row.source_set == "finance_plus_broad"));
    }

    #[test]
    fn news_window_uses_available_at_and_excludes_future_articles() {
        // gdelt-1 (broad, rates theme, published 2026-07-01T13:40:00Z) is
        // published during the regular session, so available_at ==
        // published_at and it is eligible at the very next SPY bar
        // (2026-07-01T14:30:00Z) without ever being deferred. gdelt-2
        // (broad, rates theme, published 2026-07-02T21:15:00Z, after the
        // regular close) is deferred to the next regular-session signal
        // time, which lands on 2026-07-06T13:30:00Z because 2026-07-03 is a
        // configured fixture holiday and 07-04/07-05 are a weekend (see
        // calendar.rs's next_regular_signal_skips_weekend_and_fixture_holiday
        // test). This test pins both the window boundary (before_deferral
        // must not see gdelt-2) and the after-hours flag (design.md
        // Decision 5).
        let config = config();
        let fixture = generate_fixture(&config).unwrap();
        let articles = normalize_articles(&config, &fixture.raw_articles).unwrap();
        let observations =
            build_observations(&config, "ds_test", &articles, &fixture.price_bars).unwrap();
        let before_deferral = observations
            .iter()
            .find(|row| {
                row.signal_time.to_rfc3339() == "2026-07-01T14:30:00+00:00"
                    && row.symbol == "SPY"
                    && row.source_set == "broad_news"
            })
            .unwrap();
        let after_holiday = observations
            .iter()
            .find(|row| {
                row.signal_time.to_rfc3339() == "2026-07-06T13:30:00+00:00"
                    && row.symbol == "SPY"
                    && row.source_set == "broad_news"
            })
            .unwrap();

        assert!(
            !before_deferral
                .article_ids
                .iter()
                .any(|id| after_holiday.article_ids.contains(id))
        );
        assert!(after_holiday.article_count > 0);
        // is_after_hours_signal and market_session are computed, not stubbed:
        // the deferred after-hours article must actually flow through to the
        // observation it lands in.
        assert!(after_holiday.is_after_hours_signal);
        assert!(!before_deferral.is_after_hours_signal);
        assert_eq!(after_holiday.market_session, "regular");
    }

    #[test]
    fn observations_measure_future_returns_after_signal_time() {
        let config = config();
        let fixture = generate_fixture(&config).unwrap();
        let articles = normalize_articles(&config, &fixture.raw_articles).unwrap();
        let observations =
            build_observations(&config, "ds_test", &articles, &fixture.price_bars).unwrap();
        let spy_positive = observations
            .iter()
            .find(|row| {
                row.symbol == "SPY"
                    && row.signal_time.to_rfc3339() == "2026-06-29T15:30:00+00:00"
                    && row.source_set == "finance_only"
            })
            .unwrap();

        assert!(spy_positive.mean_sentiment > 0.0);
        assert!(spy_positive.future_return > 0.0);
        assert_eq!(spy_positive.entry_time, spy_positive.signal_time);
        assert!(spy_positive.exit_time > spy_positive.entry_time);
    }

    #[test]
    fn build_observations_aggregates_multi_bar_measurement_horizons() {
        // The spec's First Experiment Matrix includes horizons like "next
        // four hours" over one-hour bars. build_one must compound multiple
        // contiguous bars for these, and must drop (not silently truncate)
        // any horizon a session close or gap prevents from being fully
        // covered by contiguous bars.
        let mut config = config();
        config.measurement_horizons_minutes = vec![240];
        let fixture = generate_fixture(&config).unwrap();
        let articles = normalize_articles(&config, &fixture.raw_articles).unwrap();
        let observations =
            build_observations(&config, "ds_test", &articles, &fixture.price_bars).unwrap();

        let mid_session = observations
            .iter()
            .find(|row| {
                row.symbol == "SPY"
                    && row.source_set == "finance_only"
                    && row.news_window_minutes == 240
                    && row.measurement_horizon_minutes == 240
                    && row.signal_time.to_rfc3339() == "2026-06-29T15:30:00+00:00"
            })
            .unwrap();
        assert_eq!(mid_session.price_bar_ids.len(), 4);
        assert_eq!(
            mid_session.exit_time,
            mid_session.signal_time + chrono::Duration::minutes(240)
        );

        let near_close_has_no_row = !observations.iter().any(|row| {
            row.symbol == "SPY"
                && row.measurement_horizon_minutes == 240
                && row.signal_time.to_rfc3339() == "2026-06-29T19:30:00+00:00"
        });
        assert!(
            near_close_has_no_row,
            "a horizon that runs past the session close must be dropped as a coverage gap, not silently truncated"
        );
    }

    #[test]
    fn no_observation_ever_uses_an_article_published_after_its_entry_time() {
        // Pins down the spec's "enter at the next configured tradable bar
        // after signal_time" rule (Required Tests: "next-bar and after-hours
        // execution tests"). entry_time == signal_time == the eligible bar's
        // open, so this asserts the no-lookahead guarantee directly instead
        // of relying on the wording alone: every article that contributed to
        // an observation must have been available at or before that
        // observation's entry.
        let config = config();
        let fixture = generate_fixture(&config).unwrap();
        let articles = normalize_articles(&config, &fixture.raw_articles).unwrap();
        let observations =
            build_observations(&config, "ds_test", &articles, &fixture.price_bars).unwrap();
        let articles_by_id: std::collections::BTreeMap<_, _> = articles
            .iter()
            .map(|article| (article.article_id.clone(), article))
            .collect();

        assert!(!observations.is_empty());
        for observation in &observations {
            for article_id in &observation.article_ids {
                let article = articles_by_id.get(article_id).unwrap();
                assert!(article.available_at <= observation.entry_time);
            }
        }
    }
}
