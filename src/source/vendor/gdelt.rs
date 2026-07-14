use crate::domain::article::{RawArticle, SourceKind};
use anyhow::{Context, Result};
use chrono::{NaiveDateTime, TimeZone, Utc};
use serde::Deserialize;

/// GDELT DOC 2.0 article list
/// (`api.gdeltproject.org/api/v2/doc/doc?mode=ArtList&format=json`).
///
/// **Deviation from the Stage 1 plan**, which said "GDELT export CSV". The DOC
/// 2.0 JSON endpoint needs no API key, returns exactly the fields we want
/// (title, seen date, domain), and avoids parsing GDELT's very wide event CSV
/// schema. The plan's Implementation Notes anticipated payload shapes differing
/// from its sketches; this is that.
///
/// Two consequences worth knowing, both recorded rather than papered over:
///
/// - GDELT gives **no description**, so `summary` is empty and sentiment for
///   these articles is scored on the headline alone. That halves the text the
///   scorer sees, and it will show up in Task 7's `lexicon_hit_rate`.
/// - GDELT gives **no ticker tags and no themes** in this mode, so every GDELT
///   article is `MacroMarket` scope and picks up the config's `macro_symbols`.
///   That is the correct reading of broad news, but it means one GDELT article
///   fans out to every macro symbol.
#[derive(Debug, Deserialize)]
struct GdeltPayload {
    #[serde(default)]
    articles: Vec<GdeltArticle>,
}

#[derive(Debug, Deserialize)]
struct GdeltArticle {
    #[serde(default)]
    url: String,
    #[serde(default)]
    title: String,
    #[serde(default)]
    domain: String,
    /// GDELT's own format: `YYYYMMDDTHHMMSSZ`, always UTC. Not RFC 3339, so it
    /// needs its own parse — and anything that fails it must land in quarantine
    /// rather than being guessed at.
    #[serde(default)]
    seendate: String,
}

fn parse_seendate(raw: &str) -> Option<chrono::DateTime<Utc>> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }
    NaiveDateTime::parse_from_str(trimmed, "%Y%m%dT%H%M%SZ")
        .ok()
        .map(|naive| Utc.from_utc_datetime(&naive))
}

pub fn parse_gdelt(json: &str) -> Result<Vec<RawArticle>> {
    let payload: GdeltPayload =
        serde_json::from_str(json).context("failed to parse GDELT article list")?;
    Ok(payload
        .articles
        .into_iter()
        .map(|article| RawArticle {
            // GDELT has no stable article id; the URL is its identity.
            vendor_id: article.url.clone(),
            source: if article.domain.trim().is_empty() {
                "gdelt_unknown_domain".to_string()
            } else {
                article.domain
            },
            source_kind: SourceKind::Broad,
            published_at: parse_seendate(&article.seendate),
            published_at_raw: article.seendate,
            title: article.title,
            summary: String::new(),
            url: article.url,
            tickers: vec![],
            themes: vec![],
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gdelt_row_parses_its_yyyymmddhhmmss_timestamp_as_utc() {
        let json = r#"{"articles":[{
            "url":"https://example.com/macro",
            "title":"Fed holds rates steady",
            "domain":"reuters.com",
            "seendate":"20250304T143000Z"
        }]}"#;

        let articles = parse_gdelt(json).unwrap();

        assert_eq!(articles.len(), 1);
        assert_eq!(
            articles[0].published_at.unwrap().to_rfc3339(),
            "2025-03-04T14:30:00+00:00"
        );
        assert_eq!(articles[0].source, "reuters.com");
        assert_eq!(articles[0].source_kind, SourceKind::Broad);
    }

    #[test]
    fn a_gdelt_row_whose_seendate_is_not_gdelt_format_is_left_for_quarantine() {
        // RFC 3339 is *wrong* here — GDELT does not emit it. If this ever
        // parsed, we would be guessing at a format the vendor did not use.
        let json = r#"{"articles":[{"url":"u","title":"T","seendate":"2025-03-04T14:30:00Z"}]}"#;

        let articles = parse_gdelt(json).unwrap();

        assert!(articles[0].published_at.is_none());
        assert_eq!(articles[0].published_at_raw, "2025-03-04T14:30:00Z");
    }

    #[test]
    fn gdelt_articles_carry_no_tickers_or_themes_so_they_read_as_macro_news() {
        let json = r#"{"articles":[{"url":"u","title":"T","seendate":"20250304T143000Z"}]}"#;

        let articles = parse_gdelt(json).unwrap();

        assert!(articles[0].tickers.is_empty());
        assert!(articles[0].themes.is_empty());
        assert!(articles[0].summary.is_empty());
    }
}
