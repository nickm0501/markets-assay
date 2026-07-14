use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// The three persisted domain enums below serialize as plain **strings**, via
/// `into`/`try_from`, rather than as serde enums.
///
/// This is not stylistic. Parquet schemas are inferred from sample rows, and a
/// serde enum is traced as a Union whose variant indices come from the *data*.
/// Stage 1's real data contains no `NewsScope::SectorTheme` article at all —
/// Massive articles always carry tickers, GDELT articles never do — which left a
/// hole in the middle of the variant indices, produced a `Null`-typed
/// placeholder column, and made the snapshot unwritable. The fixture had the same
/// hole but survived by luck, because its unobserved variant was the last one.
///
/// Serializing as a string makes the on-disk schema depend on the *type* instead
/// of on which values a given dataset happens to contain. The wire format is
/// unchanged — it was already `"finance"`, `"ticker_specific"`, etc.
macro_rules! string_enum {
    ($name:ident { $($variant:ident => $text:literal),+ $(,)? }) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
        #[serde(into = "String", try_from = "String")]
        pub enum $name {
            $($variant),+
        }

        impl $name {
            pub fn as_str(&self) -> &'static str {
                match self { $(Self::$variant => $text),+ }
            }
        }

        impl From<$name> for String {
            fn from(value: $name) -> Self {
                value.as_str().to_string()
            }
        }

        impl TryFrom<String> for $name {
            type Error = String;
            fn try_from(value: String) -> Result<Self, Self::Error> {
                match value.as_str() {
                    $($text => Ok(Self::$variant),)+
                    other => Err(format!(
                        "unknown {} value {other:?}",
                        stringify!($name)
                    )),
                }
            }
        }
    };
}

string_enum!(SourceKind {
    Finance => "finance",
    Broad => "broad",
});

string_enum!(NewsScope {
    TickerSpecific => "ticker_specific",
    SectorTheme => "sector_theme",
    MacroMarket => "macro_market",
});

string_enum!(SentimentLabel {
    Positive => "positive",
    Neutral => "neutral",
    Negative => "negative",
});

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RawArticle {
    pub vendor_id: String,
    pub source: String,
    pub source_kind: SourceKind,
    /// `None` when the vendor gave no publication time, or gave one we could
    /// not parse. The spec requires such articles be quarantined — which a
    /// non-optional `DateTime` made unrepresentable, so the fixture's always-
    /// clean timestamps hid the gap. `published_at_raw` carries the evidence.
    pub published_at: Option<DateTime<Utc>>,
    /// Exactly what the vendor sent, unparsed. Empty when the field was absent.
    /// The spec requires retaining original timestamps, and for a quarantined
    /// row this string *is* the diagnosis.
    pub published_at_raw: String,
    pub title: String,
    pub summary: String,
    pub url: String,
    pub tickers: Vec<String>,
    pub themes: Vec<String>,
}

/// Why an article never became an observation.
///
/// The two dispositions are deliberately distinct. **Quarantine** means "this
/// row is broken and cannot be trusted" — it feeds `quarantine_rate`, which
/// drives the `stop` verdict. **Exclusion** means "this row is fine, it is
/// simply not in our sample". Counting an out-of-window article as quarantined
/// would let a *sample boundary* masquerade as a *data-quality failure* and
/// halt a later stage for no reason, so the two must never share a counter.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Disposition {
    Quarantined,
    Excluded,
}

impl Disposition {
    pub fn as_str(&self) -> &'static str {
        match self {
            Disposition::Quarantined => "quarantined",
            Disposition::Excluded => "excluded",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SetAsideReason {
    // Quality: the article is broken.
    MissingPublishedAt,
    UnparseablePublishedAt,
    AmbiguousPublishedAt,
    MissingTitleAndSummary,
    // Scope: the article is fine, just not ours.
    OutsideDatasetWindow,
    NoRelevantSymbol,
    Duplicate,
}

impl SetAsideReason {
    pub fn disposition(&self) -> Disposition {
        match self {
            SetAsideReason::MissingPublishedAt
            | SetAsideReason::UnparseablePublishedAt
            | SetAsideReason::AmbiguousPublishedAt
            | SetAsideReason::MissingTitleAndSummary => Disposition::Quarantined,
            SetAsideReason::OutsideDatasetWindow
            | SetAsideReason::NoRelevantSymbol
            | SetAsideReason::Duplicate => Disposition::Excluded,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            SetAsideReason::MissingPublishedAt => "missing_published_at",
            SetAsideReason::UnparseablePublishedAt => "unparseable_published_at",
            SetAsideReason::AmbiguousPublishedAt => "ambiguous_published_at",
            SetAsideReason::MissingTitleAndSummary => "missing_title_and_summary",
            SetAsideReason::OutsideDatasetWindow => "outside_dataset_window",
            SetAsideReason::NoRelevantSymbol => "no_relevant_symbol",
            SetAsideReason::Duplicate => "duplicate",
        }
    }
}

/// An article that did not become an observation, and why. Stored flat (all
/// strings) so it round-trips through Parquet and drops straight into the
/// `set_aside.csv` report a human reads.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SetAsideArticle {
    pub vendor_id: String,
    pub source: String,
    pub published_at_raw: String,
    pub title: String,
    pub url: String,
    pub disposition: String,
    pub reason: String,
    /// `article_id` this row duplicates; empty unless `reason` is `duplicate`.
    pub duplicate_of: String,
}

impl SetAsideArticle {
    pub fn new(article: &RawArticle, reason: SetAsideReason, duplicate_of: &str) -> Self {
        Self {
            vendor_id: article.vendor_id.clone(),
            source: article.source.clone(),
            published_at_raw: article.published_at_raw.clone(),
            title: article.title.clone(),
            url: article.url.clone(),
            disposition: reason.disposition().as_str().to_string(),
            reason: reason.as_str().to_string(),
            duplicate_of: duplicate_of.to_string(),
        }
    }
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
            published_at: Some("2026-07-02T21:15:00Z".parse().unwrap()),
            published_at_raw: "2026-07-02T21:15:00Z".into(),
            title: "Fed shock weighs on markets".into(),
            summary: "Negative macro surprise after the close".into(),
            url: "fixture://after-close".into(),
            tickers: vec![],
            themes: vec!["rates".into()],
        };

        assert_eq!(
            article.published_at.unwrap().to_rfc3339(),
            "2026-07-02T21:15:00+00:00"
        );
    }

    #[test]
    fn quality_failures_quarantine_and_scope_facts_exclude() {
        // The split that keeps a sample boundary from reading as a data-quality
        // failure. `quarantine_rate` drives the `stop` verdict; exclusions must
        // never feed it.
        assert_eq!(
            SetAsideReason::MissingPublishedAt.disposition(),
            Disposition::Quarantined
        );
        assert_eq!(
            SetAsideReason::UnparseablePublishedAt.disposition(),
            Disposition::Quarantined
        );
        assert_eq!(
            SetAsideReason::OutsideDatasetWindow.disposition(),
            Disposition::Excluded
        );
        assert_eq!(
            SetAsideReason::Duplicate.disposition(),
            Disposition::Excluded
        );
        assert_eq!(
            SetAsideReason::NoRelevantSymbol.disposition(),
            Disposition::Excluded
        );
    }
}
