pub mod alpaca;
pub mod gdelt;
pub mod massive;

/// A vendor timestamp we could not turn into a UTC instant.
///
/// Parsers must never invent a time, drop the row, or default to "now". They
/// hand back `None` plus the vendor's original string, and `normalize.rs`
/// quarantines the article — which is the whole reason `RawArticle.published_at`
/// is an `Option`. Stage 1 exists to find these; hiding one would defeat it.
pub fn parse_utc(raw: &str) -> Option<chrono::DateTime<chrono::Utc>> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }
    chrono::DateTime::parse_from_rfc3339(trimmed)
        .ok()
        .map(|value| value.with_timezone(&chrono::Utc))
}
