use crate::{
    domain::article::{RawArticle, SourceKind},
    source::vendor::parse_utc,
};
use anyhow::{Context, Result};
use serde::Deserialize;

/// The Massive Stocks Basic news payload (`GET /v2/reference/news`).
///
/// Every field except the vendor id is optional here on purpose. A parser that
/// requires a field will explode on the one row that lacks it, and the whole
/// point of Stage 1 is to *survive* real data and report what was wrong with
/// it, not to refuse to load.
#[derive(Debug, Deserialize)]
struct MassivePayload {
    #[serde(default)]
    results: Vec<MassiveArticle>,
}

#[derive(Debug, Deserialize)]
struct MassiveArticle {
    #[serde(default)]
    id: String,
    #[serde(default)]
    publisher: Option<MassivePublisher>,
    #[serde(default)]
    title: String,
    #[serde(default)]
    description: String,
    #[serde(default)]
    article_url: String,
    /// Absent or malformed in real payloads. Kept as a string so the original
    /// survives into the quarantine report.
    #[serde(default)]
    published_utc: String,
    #[serde(default)]
    tickers: Vec<String>,
    #[serde(default)]
    keywords: Vec<String>,
    #[serde(default)]
    insights: Vec<MassiveInsight>,
}

#[derive(Debug, Deserialize)]
struct MassivePublisher {
    #[serde(default)]
    name: String,
}

/// Massive's per-ticker sentiment, with the model's reasoning. Free, 100%
/// coverage. A BENCHMARK, not a signal — see `RawArticle::vendor_sentiment`.
#[derive(Debug, Deserialize)]
struct MassiveInsight {
    #[serde(default)]
    sentiment: String,
}

fn insight_score(label: &str) -> Option<f64> {
    match label {
        "positive" => Some(1.0),
        "neutral" => Some(0.0),
        "negative" => Some(-1.0),
        // Real payloads contain values like "neutral/positive". Unknown labels are
        // skipped rather than guessed at — a guessed benchmark is not a benchmark.
        _ => None,
    }
}

pub fn parse_massive(json: &str) -> Result<Vec<RawArticle>> {
    let payload: MassivePayload =
        serde_json::from_str(json).context("failed to parse Massive news payload")?;
    Ok(payload
        .results
        .into_iter()
        .map(|article| {
            let source = article
                .publisher
                .map(|publisher| publisher.name)
                .filter(|name| !name.trim().is_empty())
                .unwrap_or_else(|| "massive_unknown_publisher".to_string());
            let scores: Vec<f64> = article
                .insights
                .iter()
                .filter_map(|insight| insight_score(&insight.sentiment))
                .collect();
            let vendor_sentiment = if scores.is_empty() {
                None
            } else {
                Some(scores.iter().sum::<f64>() / scores.len() as f64)
            };
            RawArticle {
                vendor_id: article.id,
                source,
                source_kind: SourceKind::Finance,
                published_at: parse_utc(&article.published_utc),
                published_at_raw: article.published_utc,
                title: article.title,
                summary: article.description,
                url: article.article_url,
                tickers: article.tickers,
                themes: article.keywords,
                vendor_sentiment,
            }
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn massive_payload_parses_into_raw_articles_with_utc_timestamps() {
        let json = r#"{"results":[{
            "id":"abc123",
            "publisher":{"name":"Reuters"},
            "title":"SPY climbs on strong data",
            "description":"Broad gains.",
            "article_url":"https://example.com/a",
            "published_utc":"2025-03-04T14:35:00Z",
            "tickers":["SPY"],
            "keywords":["markets"]
        }]}"#;

        let articles = parse_massive(json).unwrap();

        assert_eq!(articles.len(), 1);
        assert_eq!(articles[0].vendor_id, "abc123");
        assert_eq!(articles[0].source, "Reuters");
        assert_eq!(
            articles[0].published_at.unwrap().to_rfc3339(),
            "2025-03-04T14:35:00+00:00"
        );
        assert_eq!(articles[0].tickers, vec!["SPY"]);
    }

    #[test]
    fn a_massive_article_with_no_timestamp_parses_with_none_rather_than_failing() {
        // This is the path that makes quarantine possible. If the parser
        // errored or defaulted here, the bad row would never reach the
        // quarantine report — and finding bad rows IS Stage 1.
        let json = r#"{"results":[{"id":"no-time","title":"T","published_utc":""}]}"#;

        let articles = parse_massive(json).unwrap();

        assert_eq!(articles.len(), 1);
        assert!(articles[0].published_at.is_none());
        assert_eq!(articles[0].published_at_raw, "");
    }

    #[test]
    fn a_massive_article_with_a_malformed_timestamp_keeps_the_original_string() {
        let json = r#"{"results":[{"id":"bad","title":"T","published_utc":"04/03/2025 2:35pm"}]}"#;

        let articles = parse_massive(json).unwrap();

        assert!(articles[0].published_at.is_none());
        assert_eq!(articles[0].published_at_raw, "04/03/2025 2:35pm");
    }

    #[test]
    fn vendor_sentiment_is_parsed_and_averaged_across_the_tickers_it_tagged() {
        let json = r#"{"results":[{
            "id":"x","title":"T","published_utc":"2025-03-04T14:35:00Z",
            "insights":[
                {"ticker":"AAPL","sentiment":"positive"},
                {"ticker":"MSFT","sentiment":"neutral"},
                {"ticker":"XYZ","sentiment":"neutral/positive"}
            ]}]}"#;

        let articles = parse_massive(json).unwrap();

        // positive(+1) and neutral(0) average to +0.5. The malformed
        // "neutral/positive" is SKIPPED, not guessed at.
        assert_eq!(articles[0].vendor_sentiment, Some(0.5));
    }

    #[test]
    fn an_article_with_no_insights_has_no_vendor_sentiment_rather_than_zero() {
        // A silent zero would be indistinguishable from "the vendor read this and
        // thought it neutral".
        let json = r#"{"results":[{"id":"x","title":"T","published_utc":"2025-03-04T14:35:00Z"}]}"#;

        assert_eq!(parse_massive(json).unwrap()[0].vendor_sentiment, None);
    }

    #[test]
    fn an_empty_results_array_is_not_an_error() {
        assert!(parse_massive(r#"{"results":[]}"#).unwrap().is_empty());
    }
}
