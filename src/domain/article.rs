use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceKind {
    Finance,
    Broad,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NewsScope {
    TickerSpecific,
    SectorTheme,
    MacroMarket,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SentimentLabel {
    Positive,
    Neutral,
    Negative,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RawArticle {
    pub vendor_id: String,
    pub source: String,
    pub source_kind: SourceKind,
    pub published_at: DateTime<Utc>,
    pub title: String,
    pub summary: String,
    pub url: String,
    pub tickers: Vec<String>,
    pub themes: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NormalizedArticle {
    pub article_id: String,
    pub vendor_id: String,
    pub source: String,
    pub source_kind: SourceKind,
    pub published_at: DateTime<Utc>,
    pub available_at: DateTime<Utc>,
    pub title: String,
    pub summary: String,
    pub url: String,
    pub tickers: Vec<String>,
    pub themes: Vec<String>,
    pub scope: NewsScope,
    pub relevant_symbols: Vec<String>,
    pub sentiment_score: f64,
    pub sentiment_label: SentimentLabel,
    pub dedupe_key: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn after_hours_article_can_be_deferred_to_next_signal_time() {
        let article = RawArticle {
            vendor_id: "a-after-close".into(),
            source: "fixture_macro".into(),
            source_kind: SourceKind::Broad,
            published_at: "2026-07-02T21:15:00Z".parse().unwrap(),
            title: "Fed shock weighs on markets".into(),
            summary: "Negative macro surprise after the close".into(),
            url: "fixture://after-close".into(),
            tickers: vec![],
            themes: vec!["rates".into()],
        };

        assert_eq!(
            article.published_at.to_rfc3339(),
            "2026-07-02T21:15:00+00:00"
        );
    }
}
