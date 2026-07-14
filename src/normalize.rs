use crate::{
    calendar::{is_regular_session, next_regular_signal_time},
    config::Stage0Config,
    domain::article::{NewsScope, NormalizedArticle, RawArticle},
    ids::stable_id,
    sentiment::score_text,
};
use anyhow::Result;
use std::collections::{BTreeMap, BTreeSet};

pub const RELEVANCE_RULE_VERSION: &str = "stage0_relevance_v1";

pub fn normalize_articles(
    config: &Stage0Config,
    raw_articles: &[RawArticle],
) -> Result<Vec<NormalizedArticle>> {
    let mut by_dedupe_key: BTreeMap<String, &RawArticle> = BTreeMap::new();
    for article in raw_articles {
        by_dedupe_key.entry(dedupe_key(article)).or_insert(article);
    }

    let mut normalized = Vec::new();
    for article in by_dedupe_key.values() {
        let mut relevant_symbols = BTreeSet::new();
        for ticker in &article.tickers {
            if config.symbols.contains(ticker) {
                relevant_symbols.insert(ticker.clone());
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
            if is_regular_session(article.published_at, &config.holidays, &config.early_closes) {
                article.published_at
            } else {
                next_regular_signal_time(
                    article.published_at,
                    config.price_interval_minutes,
                    &config.holidays,
                    &config.early_closes,
                )
            };
        let dedupe_key = dedupe_key(article);
        let article_id = stable_id(
            "art",
            &(
                article.source.as_str(),
                article.url.as_str(),
                article.published_at,
            ),
        )?;

        normalized.push(NormalizedArticle {
            article_id,
            vendor_id: article.vendor_id.clone(),
            source: article.source.clone(),
            source_kind: article.source_kind.clone(),
            published_at: article.published_at,
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
            dedupe_key,
        });
    }
    normalized.sort_by_key(|article| (article.available_at, article.article_id.clone()));
    Ok(normalized)
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
    use crate::{config::Stage0Config, fixture::generate_fixture};

    fn config() -> Stage0Config {
        Stage0Config::load("configs/stage0_fixture.json").unwrap()
    }

    #[test]
    fn normalization_deduplicates_by_canonical_url_and_title() {
        let config = config();
        let fixture = generate_fixture(&config).unwrap();
        let normalized = normalize_articles(&config, &fixture.raw_articles).unwrap();

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
        let normalized = normalize_articles(&config, &fixture.raw_articles).unwrap();

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
        let normalized = normalize_articles(&config, &fixture.raw_articles).unwrap();
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
