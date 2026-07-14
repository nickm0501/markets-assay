use crate::domain::{article::NormalizedArticle, observation::NewsSignalObservation};
use anyhow::{Result, bail};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

/// One row of `timestamp_audit.csv` — the table Stage 1 exists to produce.
///
/// The spec names Stage 1 "hand-curated real sample for **timestamp and leakage
/// inspection**". Inspection needs something to inspect. This is it: for every
/// article that survived into the dataset, when the vendor said it was
/// published, when we allowed a signal to use it, and whether that differed.
/// An after-hours deferral, a timezone bug, or a DST error all become visible to
/// the eye here rather than hiding inside an aggregate.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TimestampAuditRow {
    pub vendor_id: String,
    pub source: String,
    pub published_at: String,
    pub available_at: String,
    /// True when `available_at != published_at`, i.e. the article was published
    /// outside the regular session and had to wait for the next signal time.
    pub was_deferred: bool,
    pub deferred_by_minutes: i64,
    /// How many observations this article actually contributed to. Zero means
    /// it entered the dataset and then influenced nothing — worth seeing.
    pub observation_count: u32,
}

pub fn timestamp_audit_rows(
    articles: &[NormalizedArticle],
    observations: &[NewsSignalObservation],
) -> Vec<TimestampAuditRow> {
    let mut uses: BTreeMap<&str, u32> = BTreeMap::new();
    for observation in observations {
        for article_id in &observation.article_ids {
            *uses.entry(article_id.as_str()).or_default() += 1;
        }
    }

    let mut rows: Vec<TimestampAuditRow> = articles
        .iter()
        .map(|article| {
            let deferred_by = article.available_at - article.published_at;
            TimestampAuditRow {
                vendor_id: article.vendor_id.clone(),
                source: article.source.clone(),
                published_at: article.published_at.to_rfc3339(),
                available_at: article.available_at.to_rfc3339(),
                was_deferred: article.available_at != article.published_at,
                deferred_by_minutes: deferred_by.num_minutes(),
                observation_count: uses.get(article.article_id.as_str()).copied().unwrap_or(0),
            }
        })
        .collect();
    rows.sort_by(|a, b| (&a.published_at, &a.vendor_id).cmp(&(&b.published_at, &b.vendor_id)));
    rows
}

/// Hard leakage check, run as part of the pipeline rather than only in tests.
///
/// design.md Decision 4 guarantees `available_at <= entry_time` for every
/// article contributing to an observation — that is what makes `entry_time ==
/// signal_time` safe rather than lookahead. A guarantee nobody checks on real
/// data is a hope. This checks it, and a violation **fails the run**: a
/// leakage bug that still produces a report is worse than no report, because
/// somebody will believe the report.
pub fn assert_no_lookahead(
    articles: &[NormalizedArticle],
    observations: &[NewsSignalObservation],
) -> Result<()> {
    let by_id: BTreeMap<&str, &NormalizedArticle> = articles
        .iter()
        .map(|article| (article.article_id.as_str(), article))
        .collect();

    let mut violations = Vec::new();
    for observation in observations {
        for article_id in &observation.article_ids {
            let Some(article) = by_id.get(article_id.as_str()) else {
                continue;
            };
            if article.available_at > observation.entry_time {
                violations.push(format!(
                    "observation {} (symbol {}, entry_time {}) uses article {} available_at {} — \
                     that is {} minutes AFTER entry",
                    observation.observation_id,
                    observation.symbol,
                    observation.entry_time.to_rfc3339(),
                    article.vendor_id,
                    article.available_at.to_rfc3339(),
                    (article.available_at - observation.entry_time).num_minutes(),
                ));
            }
        }
    }

    if !violations.is_empty() {
        bail!(
            "LOOKAHEAD DETECTED: {} observation(s) use information that did not exist yet. \
             This invalidates every result in the run.\n{}",
            violations.len(),
            violations.join("\n")
        );
    }
    Ok(())
}

/// Articles that entered the dataset but never reached an observation. Not an
/// error — a macro article about symbols with no price bars that day is simply
/// unused — but a number a human should see rather than infer.
pub fn unused_article_count(
    articles: &[NormalizedArticle],
    observations: &[NewsSignalObservation],
) -> usize {
    let used: BTreeSet<&str> = observations
        .iter()
        .flat_map(|observation| observation.article_ids.iter().map(String::as_str))
        .collect();
    articles
        .iter()
        .filter(|article| !used.contains(article.article_id.as_str()))
        .count()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        config::PipelineConfig, normalize::normalize_articles, observations::build_observations,
        source::fixture::generate_fixture,
    };

    fn fixture_data() -> (Vec<NormalizedArticle>, Vec<NewsSignalObservation>) {
        let config = PipelineConfig::load("configs/stage0_fixture.json").unwrap();
        let fixture = generate_fixture(&config).unwrap();
        let articles = normalize_articles(&config, &fixture.raw_articles)
            .unwrap()
            .normalized;
        let observations =
            build_observations(&config, "ds_test", &articles, &fixture.price_bars).unwrap();
        (articles, observations)
    }

    #[test]
    fn no_observation_uses_an_article_available_after_its_entry_time() {
        // design.md Decision 4's guarantee, checked rather than assumed. This is
        // the single most important assertion in Stage 1 — on the real sample it
        // is the difference between a result and a fantasy.
        let (articles, observations) = fixture_data();

        assert!(assert_no_lookahead(&articles, &observations).is_ok());
    }

    #[test]
    fn the_no_lookahead_check_fails_the_run_when_violated() {
        // Inject a violation: make an article's availability postdate the entry
        // it feeds. The pipeline must ERROR, not quietly report.
        let (mut articles, observations) = fixture_data();
        let used = observations
            .iter()
            .find(|observation| !observation.article_ids.is_empty())
            .expect("fixture must produce at least one observation with an article");
        let article = articles
            .iter_mut()
            .find(|article| article.article_id == used.article_ids[0])
            .unwrap();
        article.available_at = used.entry_time + chrono::Duration::minutes(30);

        let error = assert_no_lookahead(&articles, &observations)
            .unwrap_err()
            .to_string();

        assert!(error.contains("LOOKAHEAD DETECTED"), "got: {error}");
        assert!(error.contains("30 minutes AFTER entry"), "got: {error}");
    }

    #[test]
    fn timestamp_audit_marks_the_after_hours_article_as_deferred() {
        // The fixture's `gdelt-2` is published 21:15 UTC — after the close — so
        // it must wait for the next regular session's signal time. That
        // deferral is exactly what a human is looking for in this table.
        let (articles, observations) = fixture_data();

        let rows = timestamp_audit_rows(&articles, &observations);
        let deferred = rows.iter().find(|row| row.vendor_id == "gdelt-2").unwrap();

        assert!(deferred.was_deferred);
        assert!(deferred.deferred_by_minutes > 0);
        let same_day = rows
            .iter()
            .find(|row| row.vendor_id == "massive-1")
            .unwrap();
        assert!(!same_day.was_deferred);
        assert_eq!(same_day.deferred_by_minutes, 0);
    }

    #[test]
    fn the_audit_reports_one_row_per_normalized_article() {
        let (articles, observations) = fixture_data();

        let rows = timestamp_audit_rows(&articles, &observations);

        assert_eq!(rows.len(), articles.len());
    }
}
