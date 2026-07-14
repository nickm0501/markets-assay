use crate::{
    backtest::is_degenerate,
    config::VerdictThresholds,
    domain::{
        observation::NewsSignalObservation,
        run::{BucketReturnRow, CoverageRow},
    },
    verdict::{DataQualityMetrics, SignalMetrics, verdict},
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

pub type ConfigurationKey = (i64, i64, String);

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AnalysisSummary {
    pub news_window_minutes: i64,
    pub measurement_horizon_minutes: i64,
    pub source_set: String,
    pub observation_count: u32,
    pub observed_top_minus_bottom: f64,
    pub shuffled_top_minus_bottom: f64,
    pub pearson_correlation: f64,
    pub quarantine_rate: f64,
    pub articles_per_signal: f64,
    pub source_set_coverage: f64,
    pub lexicon_hit_rate: f64,
    pub degenerate: bool,
    pub recommendation: String,
    /// The measurement that produced the recommendation. A verdict nobody can
    /// audit is ceremony.
    pub reason: String,
}

/// Dataset-wide facts the observations themselves do not carry, but which the
/// verdict needs. These are exactly the inputs a synthetic fixture cannot
/// produce — which is why Stage 0 could only ever emit `continue`/`revise`.
#[derive(Debug, Clone, Default)]
pub struct AnalysisContext {
    /// Quarantined (broken) articles / total raw articles. Scope exclusions are
    /// deliberately NOT in this number.
    pub quarantine_rate: f64,
    /// Share of articles containing at least one word the sentiment lexicon
    /// knows.
    pub lexicon_hit_rate: f64,
    /// How many distinct sources each `source_set` could draw on, from the
    /// dataset's source catalog. Used to tell "this mixture is thin" from
    /// "this mixture is complete".
    pub expected_sources: BTreeMap<String, usize>,
    /// `article_id` -> publisher. Needed because `source_set_coverage` asks
    /// which sources ACTUALLY contributed to a configuration, and an
    /// observation records only how many sources fed it, not which.
    pub article_sources: BTreeMap<String, String>,
    pub long_quantile: f64,
    pub short_quantile: f64,
    pub max_modal_share: f64,
    pub thresholds: VerdictThresholds,
}

/// `run_id` is defined as one analysis/backtest configuration and result
/// (see design.md Decision 1), so every analysis and backtest function below
/// groups by this key instead of pooling across configurations.
pub fn configuration_groups(
    observations: &[NewsSignalObservation],
) -> BTreeMap<ConfigurationKey, Vec<&NewsSignalObservation>> {
    let mut groups: BTreeMap<ConfigurationKey, Vec<&NewsSignalObservation>> = BTreeMap::new();
    for row in observations {
        groups
            .entry((
                row.news_window_minutes,
                row.measurement_horizon_minutes,
                row.source_set.clone(),
            ))
            .or_default()
            .push(row);
    }
    groups
}

pub fn coverage_rows(observations: &[NewsSignalObservation]) -> Vec<CoverageRow> {
    let mut grouped: BTreeMap<(i64, i64, String, String), (u32, u32)> = BTreeMap::new();
    for row in observations {
        let key = (
            row.news_window_minutes,
            row.measurement_horizon_minutes,
            row.source_set.clone(),
            row.symbol.clone(),
        );
        let entry = grouped.entry(key).or_default();
        entry.0 += 1;
        entry.1 += row.article_count;
    }
    grouped
        .into_iter()
        .map(
            |(
                (news_window_minutes, measurement_horizon_minutes, source_set, symbol),
                (observation_count, article_count),
            )| CoverageRow {
                news_window_minutes,
                measurement_horizon_minutes,
                source_set,
                symbol,
                observation_count,
                article_count,
            },
        )
        .collect()
}

fn bucket_rows_for_group(
    key: &ConfigurationKey,
    group: &[&NewsSignalObservation],
    bucket_count: usize,
) -> Vec<BucketReturnRow> {
    let mut sorted: Vec<&NewsSignalObservation> = group.to_vec();
    sorted.sort_by(|a, b| a.mean_sentiment.partial_cmp(&b.mean_sentiment).unwrap());
    let mut rows = Vec::new();
    for (idx, label) in ["low", "middle", "high"]
        .iter()
        .enumerate()
        .take(bucket_count)
    {
        let start = idx * sorted.len() / bucket_count;
        let end = ((idx + 1) * sorted.len() / bucket_count)
            .max(start + 1)
            .min(sorted.len());
        let slice = &sorted[start..end];
        rows.push(BucketReturnRow {
            news_window_minutes: key.0,
            measurement_horizon_minutes: key.1,
            source_set: key.2.clone(),
            bucket: (*label).into(),
            observation_count: slice.len() as u32,
            mean_sentiment: mean(slice.iter().map(|row| row.mean_sentiment)),
            mean_future_return: mean(slice.iter().map(|row| row.future_return)),
        });
    }
    rows
}

pub fn bucket_return_rows(
    observations: &[NewsSignalObservation],
    bucket_count: usize,
) -> Vec<BucketReturnRow> {
    configuration_groups(observations)
        .iter()
        .flat_map(|(key, group)| bucket_rows_for_group(key, group, bucket_count))
        .collect()
}

/// Returns one summary per (news_window, measurement_horizon, source_set)
/// configuration. Never collapse this back into a single pooled summary —
/// that was the Task 7 defect design.md Decision 1 corrects.
pub fn analyze_observations(
    observations: &[NewsSignalObservation],
    context: &AnalysisContext,
) -> Vec<AnalysisSummary> {
    configuration_groups(observations)
        .into_iter()
        .map(|(key, group)| {
            // Observed and null must share the same bucketing (top_minus_bottom) so the
            // continue/revise verdict isolates the sentiment ordering, not a difference
            // in how the two spreads carve up the group.
            let observed = top_minus_bottom(&sentiment_return_pairs(&group));
            let shuffled = shuffled_spread(&group);

            let sentiments: Vec<f64> = group.iter().map(|row| row.mean_sentiment).collect();
            let degenerate = is_degenerate(
                &sentiments,
                quantile_of(&sentiments, context.long_quantile),
                quantile_of(&sentiments, context.short_quantile),
                context.max_modal_share,
            );
            let articles_per_signal = if group.is_empty() {
                0.0
            } else {
                group
                    .iter()
                    .map(|row| row.article_count as f64)
                    .sum::<f64>()
                    / group.len() as f64
            };
            // How rich is this source mixture actually, against how rich it
            // claims to be? A `finance_plus_broad` set that only ever draws on
            // one publisher is not the mixture it advertises.
            //
            // This counts DISTINCT sources contributing anywhere in the
            // configuration, not sources-per-signal. Those are different
            // questions: a single article per signal is normal in a small
            // sample and says nothing about whether the mixture is thin.
            let expected = context
                .expected_sources
                .get(&key.2)
                .copied()
                .unwrap_or(0)
                .max(1) as f64;
            let contributing: BTreeSet<&str> = group
                .iter()
                .flat_map(|row| row.article_ids.iter())
                .filter_map(|article_id| {
                    context.article_sources.get(article_id).map(String::as_str)
                })
                .collect();
            let source_set_coverage = (contributing.len() as f64 / expected).min(1.0);

            let quality = DataQualityMetrics {
                quarantine_rate: context.quarantine_rate,
                articles_per_signal,
                source_set_coverage,
                lexicon_hit_rate: context.lexicon_hit_rate,
                degenerate,
            };
            let signal = SignalMetrics {
                observed_top_minus_bottom: observed,
                shuffled_top_minus_bottom: shuffled,
                observation_count: group.len() as u32,
            };
            let decision = verdict(&quality, &signal, &context.thresholds);

            AnalysisSummary {
                news_window_minutes: key.0,
                measurement_horizon_minutes: key.1,
                source_set: key.2,
                observation_count: group.len() as u32,
                observed_top_minus_bottom: observed,
                shuffled_top_minus_bottom: shuffled,
                pearson_correlation: pearson(&group),
                quarantine_rate: quality.quarantine_rate,
                articles_per_signal: quality.articles_per_signal,
                source_set_coverage: quality.source_set_coverage,
                lexicon_hit_rate: quality.lexicon_hit_rate,
                degenerate,
                recommendation: decision.recommendation,
                reason: decision.reason,
            }
        })
        .collect()
}

fn quantile_of(values: &[f64], q: f64) -> f64 {
    let mut sorted = values.to_vec();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
    if sorted.is_empty() {
        return 0.0;
    }
    let idx = ((sorted.len() - 1) as f64 * q).round() as usize;
    sorted[idx]
}

fn sentiment_return_pairs(group: &[&NewsSignalObservation]) -> Vec<(f64, f64)> {
    group
        .iter()
        .map(|row| (row.mean_sentiment, row.future_return))
        .collect()
}

fn top_minus_bottom(pairs: &[(f64, f64)]) -> f64 {
    let mut sorted = pairs.to_vec();
    sorted.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
    let bucket_size = (sorted.len() / 3).max(1);
    let high_start = sorted.len().saturating_sub(bucket_size);
    let low = mean(
        sorted[..bucket_size.min(sorted.len())]
            .iter()
            .map(|(_, r)| *r),
    );
    let high = mean(sorted[high_start..].iter().map(|(_, r)| *r));
    high - low
}

fn shuffled_spread(group: &[&NewsSignalObservation]) -> f64 {
    if group.len() < 3 {
        return 0.0;
    }
    let sentiments: Vec<f64> = group.iter().map(|row| row.mean_sentiment).collect();
    let shuffled_pairs: Vec<(f64, f64)> = group
        .iter()
        .enumerate()
        .map(|(idx, row)| (sentiments[(idx + 1) % sentiments.len()], row.future_return))
        .collect();
    top_minus_bottom(&shuffled_pairs)
}

fn pearson(group: &[&NewsSignalObservation]) -> f64 {
    let x_mean = mean(group.iter().map(|row| row.mean_sentiment));
    let y_mean = mean(group.iter().map(|row| row.future_return));
    let numerator: f64 = group
        .iter()
        .map(|row| (row.mean_sentiment - x_mean) * (row.future_return - y_mean))
        .sum();
    let x_var: f64 = group
        .iter()
        .map(|row| (row.mean_sentiment - x_mean).powi(2))
        .sum();
    let y_var: f64 = group
        .iter()
        .map(|row| (row.future_return - y_mean).powi(2))
        .sum();
    if x_var == 0.0 || y_var == 0.0 {
        0.0
    } else {
        numerator / (x_var.sqrt() * y_var.sqrt())
    }
}

fn mean(values: impl Iterator<Item = f64>) -> f64 {
    let mut count = 0.0;
    let mut total = 0.0;
    for value in values {
        count += 1.0;
        total += value;
    }
    if count == 0.0 { 0.0 } else { total / count }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        config::PipelineConfig, domain::observation::NewsSignalObservation,
        normalize::normalize_articles, observations::build_observations,
        source::fixture::generate_fixture,
    };

    /// A context with clean data, so these tests exercise the SIGNAL gates.
    /// The data-quality gates get their own dedicated tests in `verdict.rs`.
    fn healthy_context() -> AnalysisContext {
        AnalysisContext {
            quarantine_rate: 0.0,
            lexicon_hit_rate: 1.0,
            expected_sources: BTreeMap::from([
                ("finance_only".to_string(), 1),
                ("broad_news".to_string(), 1),
                ("finance_plus_broad".to_string(), 1),
            ]),
            article_sources: BTreeMap::new(),
            long_quantile: 0.8,
            short_quantile: 0.2,
            max_modal_share: 1.0,
            thresholds: VerdictThresholds {
                max_quarantine_rate: 1.0,
                min_lexicon_hit_rate: 0.0,
                min_source_coverage: 0.0,
                min_articles_per_signal: 0.0,
                // These tests exercise the SIGNAL gates on tiny synthetic
                // groups; the data-quality and sample-size gates get their own
                // dedicated tests in verdict.rs.
                min_observations: 0,
                min_spread_margin: 0.0,
            },
        }
    }

    fn observations() -> Vec<crate::domain::observation::NewsSignalObservation> {
        let config = PipelineConfig::load("configs/stage0_fixture.json").unwrap();
        let fixture = generate_fixture(&config).unwrap();
        let articles = normalize_articles(&config, &fixture.raw_articles)
            .unwrap()
            .normalized;
        build_observations(&config, "ds_test", &articles, &fixture.price_bars).unwrap()
    }

    fn observation_with_sentiment_and_return(
        mean_sentiment: f64,
        future_return: f64,
    ) -> NewsSignalObservation {
        let now = chrono::Utc::now();
        NewsSignalObservation {
            observation_id: "obs".to_string(),
            dataset_id: "ds".to_string(),
            symbol: "TEST".to_string(),
            signal_time: now,
            news_window_minutes: 60,
            measurement_horizon_minutes: 60,
            price_interval_minutes: 5,
            source_set: "finance_only".to_string(),
            article_count: 1,
            ticker_article_count: 1,
            sector_theme_article_count: 0,
            macro_article_count: 0,
            source_count: 1,
            publisher_count: 1,
            mean_sentiment,
            weighted_sentiment: mean_sentiment,
            extreme_sentiment: mean_sentiment,
            positive_article_count: 0,
            negative_article_count: 0,
            sentiment_dispersion: 0.0,
            prior_return: 0.0,
            prior_volatility: 0.0,
            market_session: "regular".to_string(),
            is_after_hours_signal: false,
            future_return,
            future_volatility: 0.0,
            future_tail_event: false,
            future_max_drawdown: 0.0,
            future_max_runup: 0.0,
            entry_time: now,
            exit_time: now,
            article_ids: vec![],
            price_bar_ids: vec![],
            created_by_run_id: "run".to_string(),
        }
    }

    #[test]
    fn coverage_counts_observations_and_articles_by_window_horizon_source_set_and_symbol() {
        let rows = coverage_rows(&observations());

        assert!(rows.iter().any(|row| row.symbol == "SPY"
            && row.news_window_minutes == 60
            && row.source_set == "finance_only"
            && row.observation_count > 0));
        assert!(rows.iter().any(|row| row.symbol == "QQQ"
            && row.news_window_minutes == 240
            && row.source_set == "broad_news"
            && row.article_count > 0));
    }

    #[test]
    fn sentiment_buckets_show_synthetic_top_minus_bottom_spread_within_a_configuration() {
        let rows = bucket_return_rows(&observations(), 3);
        let configuration_rows: Vec<_> = rows
            .iter()
            .filter(|row| {
                row.news_window_minutes == 60
                    && row.measurement_horizon_minutes == 60
                    && row.source_set == "finance_only"
            })
            .collect();
        let high = configuration_rows
            .iter()
            .find(|row| row.bucket == "high")
            .unwrap();
        let low = configuration_rows
            .iter()
            .find(|row| row.bucket == "low")
            .unwrap();

        assert!(high.mean_future_return > low.mean_future_return);
    }

    #[test]
    fn analyze_observations_returns_one_summary_per_configuration_not_pooled() {
        let summaries = analyze_observations(&observations(), &healthy_context());
        let configuration_count = configuration_groups(&observations()).len();

        assert_eq!(summaries.len(), configuration_count);
        assert!(configuration_count > 1);
    }

    #[test]
    fn shuffled_baseline_is_worse_than_observed_spread_for_the_finance_only_configuration() {
        let summaries = analyze_observations(&observations(), &healthy_context());
        let summary = summaries
            .iter()
            .find(|summary| {
                summary.news_window_minutes == 60
                    && summary.measurement_horizon_minutes == 60
                    && summary.source_set == "finance_only"
            })
            .unwrap();

        assert!(summary.observed_top_minus_bottom > summary.shuffled_top_minus_bottom);
    }

    #[test]
    fn observed_spread_uses_the_same_bucketing_as_the_null_baseline_for_group_sizes_not_divisible_by_three()
     {
        // len = 7 is not divisible by 3, so an index-thirds bucketing (high = last
        // ceil(len/3) = 3 elements) and a fixed len/3 bucketing (high = last 2
        // elements) disagree. Perfectly correlated sentiment/return pairs make the
        // expected top_minus_bottom value unambiguous: sorted sentiment 0..6, high
        // bucket = last 2 elements (sentiment 5, 6 -> returns 50, 60, mean 55), low
        // bucket = first 2 elements (sentiment 0, 1 -> returns 0, 10, mean 5).
        let group: Vec<NewsSignalObservation> = (0..7)
            .map(|i| observation_with_sentiment_and_return(i as f64, i as f64 * 10.0))
            .collect();

        let summaries = analyze_observations(&group, &healthy_context());

        assert_eq!(summaries.len(), 1);
        assert_eq!(summaries[0].observation_count, 7);
        assert!((summaries[0].observed_top_minus_bottom - 50.0).abs() < 1e-9);
    }
}
