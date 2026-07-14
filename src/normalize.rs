use crate::{
    calendar::{is_regular_session, next_regular_signal_time},
    config::PipelineConfig,
    domain::article::{NewsScope, NormalizedArticle, RawArticle, SetAsideArticle, SetAsideReason},
    ids::stable_id,
    sentiment::score_text,
};
use anyhow::Result;
use std::collections::{BTreeMap, BTreeSet};

/// Bumped from `stage0_relevance_v1` when the constituent->ETF rule was added
/// (2026-07-14). The spec requires the observation-set manifest record the
/// relevance-rule version precisely so two observation sets built under
/// *different* rules can never be mistaken for each other — the version is what
/// makes `observation_set_id` honest.
pub const RELEVANCE_RULE_VERSION: &str = "stage1_relevance_v2";

/// Every raw article ends up in exactly one of these two vectors. Nothing is
/// dropped on the floor — the spec forbids silently discarding rows, and a
/// timestamp bug you cannot see is the exact failure Stage 1 exists to catch.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct NormalizeOutcome {
    pub normalized: Vec<NormalizedArticle>,
    pub set_aside: Vec<SetAsideArticle>,
}

pub fn normalize_articles(
    config: &PipelineConfig,
    raw_articles: &[RawArticle],
) -> Result<NormalizeOutcome> {
    let mut outcome = NormalizeOutcome::default();

    // Quality gate first: an article with no usable timestamp cannot be
    // deduplicated by recency, dated, or placed in a news window, so nothing
    // downstream can meaningfully handle it.
    let mut usable = Vec::new();
    for article in raw_articles {
        if article.published_at.is_none() {
            let reason = if article.published_at_raw.trim().is_empty() {
                SetAsideReason::MissingPublishedAt
            } else {
                SetAsideReason::UnparseablePublishedAt
            };
            outcome
                .set_aside
                .push(SetAsideArticle::new(article, reason, ""));
            continue;
        }
        if article.title.trim().is_empty() && article.summary.trim().is_empty() {
            outcome.set_aside.push(SetAsideArticle::new(
                article,
                SetAsideReason::MissingTitleAndSummary,
                "",
            ));
            continue;
        }
        // Out of window is a SCOPE fact, not a quality failure: the article is
        // perfectly sound, it simply is not in this dataset. It must be
        // excluded rather than quarantined, because quarantine_rate drives the
        // `stop` verdict and a sample boundary must never read as unreliable
        // timestamps.
        if outside_configured_window(config, article) {
            outcome.set_aside.push(SetAsideArticle::new(
                article,
                SetAsideReason::OutsideDatasetWindow,
                "",
            ));
            continue;
        }
        usable.push(article);
    }

    // Deduplicate syndicated and republished articles. The survivor is the
    // EARLIEST publication — a later republication of the same story carries no
    // information the original did not, and trading the later timestamp would
    // be backdating a signal we could not have acted on.
    let mut survivors: BTreeMap<String, &RawArticle> = BTreeMap::new();
    for article in &usable {
        survivors
            .entry(dedupe_key(article))
            .and_modify(|kept| {
                if article.published_at < kept.published_at {
                    *kept = article;
                }
            })
            .or_insert(article);
    }
    for article in &usable {
        let kept = survivors[&dedupe_key(article)];
        if !std::ptr::eq(*article, kept) {
            outcome.set_aside.push(SetAsideArticle::new(
                article,
                SetAsideReason::Duplicate,
                &article_id(kept)?,
            ));
        }
    }

    for article in survivors.values() {
        let mut relevant_symbols = BTreeSet::new();
        for ticker in &article.tickers {
            // Direct: the article names a symbol we trade.
            if config.symbols.contains(ticker) {
                relevant_symbols.insert(ticker.clone());
            }
            // Indirect: the article names a constituent of an ETF we trade.
            // Without this, ETF news barely exists — the source probe found only
            // 4 of 677 articles tagged any of SPY/QQQ/DIA/IWM, because vendors
            // tag news to companies. An AMZN story moves QQQ whether or not
            // anyone wrote "QQQ" in the tags.
            if let Some(etfs) = config.constituent_etf_map.get(ticker) {
                for etf in etfs {
                    if config.symbols.contains(etf) {
                        relevant_symbols.insert(etf.clone());
                    }
                }
            }
        }
        for theme in &article.themes {
            if let Some(symbols) = config.theme_symbol_map.get(theme) {
                for symbol in symbols {
                    relevant_symbols.insert(symbol.clone());
                }
            }
        }
        if relevant_symbols.is_empty() && article.tickers.is_empty() {
            for symbol in &config.macro_symbols {
                relevant_symbols.insert(symbol.clone());
            }
        }
        // An article about symbols we do not trade is not defective — it is out
        // of scope. Before this, such rows were normalized and then contributed
        // to no observation, disappearing without ever being counted.
        if relevant_symbols.is_empty() {
            outcome.set_aside.push(SetAsideArticle::new(
                article,
                SetAsideReason::NoRelevantSymbol,
                "",
            ));
            continue;
        }

        let published_at = article
            .published_at
            .expect("articles without a timestamp were quarantined above");
        let scope = if !article.tickers.is_empty() {
            NewsScope::TickerSpecific
        } else if !article.themes.is_empty() {
            NewsScope::SectorTheme
        } else {
            NewsScope::MacroMarket
        };
        let combined_text = format!("{} {}", article.title, article.summary);
        let sentiment = score_text(&combined_text);
        let available_at =
            if is_regular_session(published_at, &config.holidays, &config.early_closes) {
                published_at
            } else {
                next_regular_signal_time(
                    published_at,
                    config.price_interval_minutes,
                    &config.holidays,
                    &config.early_closes,
                )
            };

        outcome.normalized.push(NormalizedArticle {
            article_id: article_id(article)?,
            vendor_id: article.vendor_id.clone(),
            source: article.source.clone(),
            source_kind: article.source_kind,
            published_at,
            available_at,
            title: article.title.clone(),
            summary: article.summary.clone(),
            url: article.url.clone(),
            tickers: article.tickers.clone(),
            themes: article.themes.clone(),
            scope,
            relevant_symbols: relevant_symbols.into_iter().collect(),
            sentiment_score: sentiment.score,
            sentiment_label: sentiment.label,
            // Carried through for comparison only. It must never reach the
            // strategy — see `vendor_sentiment_never_reaches_the_backtest`.
            vendor_sentiment: article.vendor_sentiment.unwrap_or(0.0),
            vendor_sentiment_available: article.vendor_sentiment.is_some(),
            dedupe_key: dedupe_key(article),
        });
    }
    outcome
        .normalized
        .sort_by_key(|article| (article.available_at, article.article_id.clone()));
    outcome.set_aside.sort_by(|a, b| {
        (&a.disposition, &a.reason, &a.vendor_id).cmp(&(&b.disposition, &b.reason, &b.vendor_id))
    });
    Ok(outcome)
}

fn outside_configured_window(config: &PipelineConfig, article: &RawArticle) -> bool {
    let Some(published_at) = article.published_at else {
        return false;
    };
    let date = published_at.date_naive();
    config.dataset_date_start.is_some_and(|start| date < start)
        || config.dataset_date_end.is_some_and(|end| date > end)
}

fn article_id(article: &RawArticle) -> Result<String> {
    stable_id(
        "art",
        &(
            article.source.as_str(),
            article.url.as_str(),
            article.published_at,
        ),
    )
}

fn dedupe_key(article: &RawArticle) -> String {
    format!(
        "{}::{}",
        article.url.trim().to_lowercase(),
        article.title.trim().to_lowercase()
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        config::PipelineConfig,
        domain::article::{Disposition, SourceKind},
        source::fixture::generate_fixture,
    };

    fn config() -> PipelineConfig {
        PipelineConfig::load("configs/stage0_fixture.json").unwrap()
    }

    fn raw(vendor_id: &str, published_at: Option<&str>, title: &str, url: &str) -> RawArticle {
        RawArticle {
            vendor_id: vendor_id.into(),
            source: "test_wire".into(),
            source_kind: SourceKind::Finance,
            published_at: published_at.and_then(|value| value.parse().ok()),
            published_at_raw: published_at.unwrap_or("").into(),
            title: title.into(),
            summary: "body".into(),
            url: url.into(),
            tickers: vec!["SPY".into()],
            themes: vec![],
            vendor_sentiment: None,
        }
    }

    fn quarantined(outcome: &NormalizeOutcome) -> Vec<&SetAsideArticle> {
        outcome
            .set_aside
            .iter()
            .filter(|row| row.disposition == Disposition::Quarantined.as_str())
            .collect()
    }

    fn excluded(outcome: &NormalizeOutcome) -> Vec<&SetAsideArticle> {
        outcome
            .set_aside
            .iter()
            .filter(|row| row.disposition == Disposition::Excluded.as_str())
            .collect()
    }

    #[test]
    fn article_with_no_published_at_is_quarantined_not_dropped() {
        let outcome = normalize_articles(&config(), &[raw("no-time", None, "T", "u://1")]).unwrap();

        assert!(outcome.normalized.is_empty());
        assert_eq!(quarantined(&outcome).len(), 1);
        assert_eq!(quarantined(&outcome)[0].reason, "missing_published_at");
    }

    #[test]
    fn article_with_unparseable_published_at_retains_its_original_vendor_string() {
        // The vendor's raw string is the only evidence a human has for WHY the
        // row failed, so it must survive into the quarantine report.
        let outcome = normalize_articles(
            &config(),
            &[raw("bad-time", Some("last Tuesday-ish"), "T", "u://1")],
        )
        .unwrap();

        let row = quarantined(&outcome)[0];
        assert_eq!(row.reason, "unparseable_published_at");
        assert_eq!(row.published_at_raw, "last Tuesday-ish");
    }

    #[test]
    fn every_raw_article_lands_in_exactly_one_of_normalized_quarantined_or_excluded() {
        // The no-silent-drop invariant. If this ever fails, an article vanished
        // without being counted anywhere — the exact class of bug that makes a
        // coverage report a lie.
        let raw_articles = vec![
            raw("good", Some("2026-06-29T14:05:00Z"), "Strong", "u://good"),
            raw("no-time", None, "T", "u://no-time"),
            raw("bad-time", Some("nonsense"), "T", "u://bad-time"),
            raw("dup", Some("2026-06-29T15:05:00Z"), "Strong", "u://good"),
        ];

        let outcome = normalize_articles(&config(), &raw_articles).unwrap();

        assert_eq!(
            outcome.normalized.len() + outcome.set_aside.len(),
            raw_articles.len()
        );
    }

    #[test]
    fn an_out_of_window_article_is_excluded_and_does_not_raise_the_quarantine_rate() {
        // The counter-test for the quarantine/exclusion split. An article
        // outside the sample window is FINE — it is simply not ours. Counting
        // it as quarantined would drive `quarantine_rate` up and could trip the
        // `stop` verdict ("timestamps unreliable") on what is only a boundary.
        let mut config = config();
        config.dataset_date_start = Some("2026-06-29".parse().unwrap());
        config.dataset_date_end = Some("2026-07-07".parse().unwrap());
        let raw_articles = vec![
            raw("inside", Some("2026-06-30T14:05:00Z"), "Strong", "u://in"),
            raw("before", Some("2020-01-02T14:05:00Z"), "Strong", "u://b"),
            raw("after", Some("2030-01-02T14:05:00Z"), "Strong", "u://a"),
        ];

        let outcome = normalize_articles(&config, &raw_articles).unwrap();

        assert_eq!(outcome.normalized.len(), 1);
        assert_eq!(
            quarantined(&outcome).len(),
            0,
            "these are scope, not quality"
        );
        assert_eq!(excluded(&outcome).len(), 2);
        assert!(
            excluded(&outcome)
                .iter()
                .all(|row| row.reason == "outside_dataset_window")
        );
    }

    #[test]
    fn deduplication_keeps_the_earliest_published_at_of_a_duplicate_group() {
        // The earliest publication is the only one we could actually have
        // traded on; keeping a later republication would backdate a signal.
        let raw_articles = vec![
            raw("late", Some("2026-06-29T16:05:00Z"), "Same story", "u://s"),
            raw("early", Some("2026-06-29T14:05:00Z"), "Same story", "u://s"),
        ];

        let outcome = normalize_articles(&config(), &raw_articles).unwrap();

        assert_eq!(outcome.normalized.len(), 1);
        assert_eq!(outcome.normalized[0].vendor_id, "early");
        let dropped = excluded(&outcome);
        assert_eq!(dropped.len(), 1);
        assert_eq!(dropped[0].vendor_id, "late");
        assert_eq!(dropped[0].reason, "duplicate");
        assert_eq!(dropped[0].duplicate_of, outcome.normalized[0].article_id);
    }

    #[test]
    fn the_fixtures_syndicated_duplicate_is_excluded_rather_than_vanishing() {
        let config = config();
        let fixture = generate_fixture(&config).unwrap();

        let outcome = normalize_articles(&config, &fixture.raw_articles).unwrap();

        assert_eq!(fixture.raw_articles.len(), 6);
        assert_eq!(outcome.normalized.len(), 5);
        assert_eq!(quarantined(&outcome).len(), 0);
        assert_eq!(excluded(&outcome).len(), 1);
        assert_eq!(excluded(&outcome)[0].vendor_id, "massive-1-dup");
    }

    #[test]
    fn normalization_deduplicates_by_canonical_url_and_title() {
        let config = config();
        let fixture = generate_fixture(&config).unwrap();
        let normalized = normalize_articles(&config, &fixture.raw_articles)
            .unwrap()
            .normalized;

        assert_eq!(fixture.raw_articles.len(), 6);
        assert_eq!(normalized.len(), 5);
        assert_eq!(
            normalized
                .iter()
                .filter(|a| a.url == "fixture://spy-breakout")
                .count(),
            1
        );
    }

    #[test]
    fn normalization_maps_direct_theme_and_macro_relevance() {
        let config = config();
        let fixture = generate_fixture(&config).unwrap();
        let normalized = normalize_articles(&config, &fixture.raw_articles)
            .unwrap()
            .normalized;

        let spy = normalized
            .iter()
            .find(|a| a.url == "fixture://spy-breakout")
            .unwrap();
        let rates = normalized
            .iter()
            .find(|a| a.url == "fixture://rates-relief")
            .unwrap();
        let tech = normalized
            .iter()
            .find(|a| a.url == "fixture://qqq-downgrade")
            .unwrap();

        assert_eq!(spy.relevant_symbols, vec!["SPY"]);
        assert_eq!(rates.relevant_symbols, vec!["QQQ", "SPY"]);
        assert!(tech.relevant_symbols.contains(&"QQQ".to_string()));
    }

    #[test]
    fn after_hours_article_gets_deferred_available_at() {
        let config = config();
        let fixture = generate_fixture(&config).unwrap();
        let normalized = normalize_articles(&config, &fixture.raw_articles)
            .unwrap()
            .normalized;
        let after_close = normalized
            .iter()
            .find(|a| a.url == "fixture://after-close")
            .unwrap();

        assert_eq!(
            after_close.published_at.to_rfc3339(),
            "2026-07-02T21:15:00+00:00"
        );
        assert_eq!(
            after_close.available_at.to_rfc3339(),
            "2026-07-06T13:30:00+00:00"
        );
    }
}
