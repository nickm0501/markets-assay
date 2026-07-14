use crate::domain::{
    observation::NewsSignalObservation,
    run::{BucketReturnRow, CoverageRow},
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

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
    pub recommendation: String,
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
pub fn analyze_observations(observations: &[NewsSignalObservation]) -> Vec<AnalysisSummary> {
    configuration_groups(observations)
        .into_iter()
        .map(|(key, group)| {
            let rows = bucket_rows_for_group(&key, &group, 3);
            let low = rows
                .iter()
                .find(|row| row.bucket == "low")
                .map(|row| row.mean_future_return)
                .unwrap_or(0.0);
            let high = rows
                .iter()
                .find(|row| row.bucket == "high")
                .map(|row| row.mean_future_return)
                .unwrap_or(0.0);
            let observed = high - low;
            let shuffled = shuffled_spread(&group);
            let recommendation = if observed > shuffled && observed > 0.0 {
                "continue".to_string()
            } else {
                "revise".to_string()
            };
            AnalysisSummary {
                news_window_minutes: key.0,
                measurement_horizon_minutes: key.1,
                source_set: key.2,
                observation_count: group.len() as u32,
                observed_top_minus_bottom: observed,
                shuffled_top_minus_bottom: shuffled,
                pearson_correlation: pearson(&group),
                recommendation,
            }
        })
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
        config::Stage0Config, fixture::generate_fixture, normalize::normalize_articles,
        observations::build_observations,
    };

    fn observations() -> Vec<crate::domain::observation::NewsSignalObservation> {
        let config = Stage0Config::load("configs/stage0_fixture.json").unwrap();
        let fixture = generate_fixture(&config).unwrap();
        let articles = normalize_articles(&config, &fixture.raw_articles).unwrap();
        build_observations(&config, "ds_test", &articles, &fixture.price_bars).unwrap()
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
        let summaries = analyze_observations(&observations());
        let configuration_count = configuration_groups(&observations()).len();

        assert_eq!(summaries.len(), configuration_count);
        assert!(configuration_count > 1);
    }

    #[test]
    fn shuffled_baseline_is_worse_than_observed_spread_for_the_finance_only_configuration() {
        let summaries = analyze_observations(&observations());
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
}
