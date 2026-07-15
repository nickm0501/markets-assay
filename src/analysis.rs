use crate::{
    backtest::is_degenerate,
    config::VerdictThresholds,
    domain::{
        observation::NewsSignalObservation,
        run::{BucketReturnRow, CoverageRow},
    },
    verdict::{DataQualityMetrics, SignalMetrics, verdict},
};
use anyhow::{Result, anyhow};
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
    /// 95th percentile of the null distribution. `continue` requires beating the
    /// MEAN (the spec's literal bar); this column tells a reader whether the
    /// result would also survive a real significance test.
    pub shuffled_p95: f64,
    /// Empirical p-value from the BLOCK-permutation null: the share of null draws
    /// whose spread meets or beats the observed one (add-one corrected). The null
    /// permutes whole time-blocks, not individual observations, so it is valid on
    /// the overlapping/fanned-out observations real data produces — where an
    /// i.i.d. shuffle fires ~30% of the time on pure noise.
    pub null_p_value: f64,
    /// Whether this configuration's p-value survives Benjamini-Hochberg FDR
    /// control across the whole configuration matrix. A `continue` requires this:
    /// gating each of ~48 configurations at p < 0.05 independently expects ~2.4
    /// false continues on pure noise.
    pub significant_after_fdr: bool,
    /// False when the held-out split had fewer than 3 observations and the spread
    /// was judged on the FULL group instead — an in-sample measurement a reader
    /// must be able to see and discount.
    pub judged_on_holdout: bool,
    pub pearson_correlation: f64,
    pub quarantine_rate: f64,
    pub articles_per_signal: f64,
    pub source_set_coverage: f64,
    pub lexicon_hit_rate: f64,
    pub degenerate: bool,
    pub vendor_agreement: f64,
    pub sentiment_net_return: f64,
    pub best_baseline_net_return: f64,
    pub best_baseline_name: String,
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
    /// Spearman rho between our local score and the VENDOR's own sentiment.
    /// A diagnostic, never a signal (design.md Decision 21). It answers the one
    /// question we otherwise could not: "is our scorer any good?"
    pub vendor_agreement: f64,
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
    pub seed: u64,
    pub development_fraction: f64,
    pub thresholds: VerdictThresholds,
    /// Per configuration: (sentiment net return, best baseline net return, its
    /// name). Computed by running all five strategies through the same backtest
    /// engine BEFORE analysis, so the verdict can apply the spec's baseline gate.
    pub strategy_nets: BTreeMap<ConfigurationKey, (f64, f64, String)>,
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
) -> Result<Vec<AnalysisSummary>> {
    // First pass: everything per configuration EXCEPT the family-wide FDR
    // decision, which cannot be made until every configuration's p-value is known.
    let mut summaries: Vec<AnalysisSummary> = Vec::new();
    for (key, group) in configuration_groups(observations) {
        // THE SPREAD IS MEASURED ON THE HOLDOUT.
        //
        // The spec's success gate is explicit: "the HELD-OUT top-minus-bottom
        // sentiment return spread is positive". Measuring it across the whole
        // sample means measuring it partly on the data the thresholds were
        // fitted to — marking your own homework.
        //
        // Both observed and null share the same bucketing, so the verdict
        // isolates the sentiment ORDERING rather than a difference in how the
        // two spreads carve up the group.
        let (development, holdout) =
            crate::backtest::split_chronologically(&group, context.development_fraction);
        let judged_on_holdout = holdout.len() >= 3;
        let judged: &[&NewsSignalObservation] = if judged_on_holdout { &holdout } else { &group };
        let observed = top_minus_bottom(&sentiment_return_pairs(judged));
        let (shuffled, shuffled_p95, null_p_value) =
            shuffled_spread(judged, observed, context.seed);

        // DEGENERACY IS JUDGED ON THE DEVELOPMENT SPLIT — the same data, and the
        // same quantiles, the backtest uses. Judging it on whole-group quantiles
        // (as this once did) let the two disagree: analysis could call a
        // configuration degenerate while the backtest happily traded it, or the
        // reverse. Fall back to the whole group only when there is no development
        // split at all (development_fraction == 0), where there is nothing else to
        // use.
        let degeneracy_sentiments: Vec<f64> = if development.is_empty() {
            group.iter().map(|row| row.mean_sentiment).collect()
        } else {
            development.iter().map(|row| row.mean_sentiment).collect()
        };
        let degenerate = is_degenerate(
            &degeneracy_sentiments,
            quantile_of(&degeneracy_sentiments, context.long_quantile),
            quantile_of(&degeneracy_sentiments, context.short_quantile),
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
            .filter_map(|article_id| context.article_sources.get(article_id).map(String::as_str))
            .collect();
        let source_set_coverage = (contributing.len() as f64 / expected).min(1.0);

        let quality = DataQualityMetrics {
            quarantine_rate: context.quarantine_rate,
            articles_per_signal,
            source_set_coverage,
            lexicon_hit_rate: context.lexicon_hit_rate,
            degenerate,
        };
        // A MISSING BASELINE COMPARISON IS A HARD ERROR, not a default.
        //
        // Defaulting to (0.0, NEG_INFINITY) made every configuration trivially
        // "beat" its baseline, silently DISABLING the baseline gate the moment a
        // key-construction bug made the lookup miss. `strategy_comparison` produces
        // an entry for every configuration with holdout results, so a miss means a
        // real defect — fail loudly instead of reporting an unearned `continue`.
        let (sentiment_net, best_baseline_net, best_baseline_name) =
            context.strategy_nets.get(&key).cloned().ok_or_else(|| {
                anyhow!(
                    "no backtest strategy comparison for configuration {key:?}; the baseline gate \
                     cannot be applied, and defaulting it open would silently disable the check"
                )
            })?;
        let signal = SignalMetrics {
            observed_top_minus_bottom: observed,
            shuffled_top_minus_bottom: shuffled,
            shuffled_p95,
            observation_count: judged.len() as u32,
            sentiment_net_return: sentiment_net,
            best_baseline_net_return: best_baseline_net,
            best_baseline_name,
        };
        let decision = verdict(&quality, &signal, &context.thresholds);
        let reason = if judged_on_holdout {
            decision.reason
        } else {
            format!(
                "[SMALL HOLDOUT: spread judged on the full group, not a held-out split] {}",
                decision.reason
            )
        };

        summaries.push(AnalysisSummary {
            news_window_minutes: key.0,
            measurement_horizon_minutes: key.1,
            source_set: key.2,
            observation_count: group.len() as u32,
            observed_top_minus_bottom: observed,
            shuffled_top_minus_bottom: shuffled,
            shuffled_p95,
            null_p_value,
            significant_after_fdr: false,
            judged_on_holdout,
            pearson_correlation: pearson(&group),
            quarantine_rate: quality.quarantine_rate,
            articles_per_signal: quality.articles_per_signal,
            source_set_coverage: quality.source_set_coverage,
            lexicon_hit_rate: quality.lexicon_hit_rate,
            degenerate,
            vendor_agreement: context.vendor_agreement,
            sentiment_net_return: signal.sentiment_net_return,
            best_baseline_net_return: signal.best_baseline_net_return,
            best_baseline_name: signal.best_baseline_name.clone(),
            recommendation: decision.recommendation,
            reason,
        });
    }

    // Second pass: Benjamini-Hochberg across the whole matrix, which can only run
    // once every p-value is in hand.
    apply_benjamini_hochberg(&mut summaries, FDR_Q);
    Ok(summaries)
}

/// The false-discovery rate the configuration matrix is controlled at. 0.05 keeps
/// the family-wide expectation of false `continue`s in line with the per-test bar
/// a single configuration faces.
const FDR_Q: f64 = 0.05;

/// Benjamini-Hochberg across the configuration matrix.
///
/// Each configuration is one hypothesis test. Gating each at p < 0.05
/// independently means a ~48-configuration matrix expects ~2.4 false `continue`s
/// on pure noise even with a valid null. BH controls the false-DISCOVERY rate
/// across the family: sort the p-values ascending, find the largest rank `k` whose
/// p-value clears `(k/m)·q`, and only tests at or below that p-value are
/// significant. A `continue` that clears its own null but not the family-wide
/// correction is downgraded to `revise` — across a matrix this size it is as
/// likely to be luck as signal.
fn apply_benjamini_hochberg(summaries: &mut [AnalysisSummary], q: f64) {
    let m = summaries.len();
    if m == 0 {
        return;
    }
    let mut ranked: Vec<(usize, f64)> = summaries
        .iter()
        .enumerate()
        .map(|(i, summary)| (i, summary.null_p_value))
        .collect();
    ranked.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap());
    let mut cutoff_rank = 0usize;
    for (rank, (_, p)) in ranked.iter().enumerate() {
        let k = rank + 1;
        if *p <= (k as f64 / m as f64) * q {
            cutoff_rank = k;
        }
    }
    let threshold = if cutoff_rank == 0 {
        f64::NEG_INFINITY
    } else {
        ranked[cutoff_rank - 1].1
    };
    for summary in summaries.iter_mut() {
        summary.significant_after_fdr = summary.null_p_value <= threshold;
        if summary.recommendation == "continue" && !summary.significant_after_fdr {
            summary.reason = format!(
                "downgraded from continue: p-value {:.4} is not significant after Benjamini-Hochberg \
                 FDR control across {} configurations (q = {:.2}). Prior reason: {}",
                summary.null_p_value, m, q, summary.reason
            );
            summary.recommendation = "revise".to_string();
        }
    }
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

/// How many permutations to build the null distribution from.
///
/// One permutation is not a null distribution, it is one sample from it — and a
/// noisy one. The bar that `continue` must clear should not itself be a coin flip.
const NULL_PERMUTATIONS: usize = 200;

/// The null hypothesis: sentiment scores paired with the WRONG returns — but
/// permuted in BLOCKS, because this pipeline's observations are not exchangeable.
///
/// **The i.i.d. shuffle this replaced was invalid on real data.** An i.i.d.
/// Fisher-Yates shuffle assumes each observation is independent. This pipeline's
/// own construction guarantees they are not: `normalize.rs` fans one macro article
/// out to every macro symbol; a news window several bars wide puts the same
/// article in consecutive observations; and a horizon several bars wide makes
/// adjacent `future_return`s share most of their hours. Under that clustering the
/// *observed* spread has fat tails an i.i.d. null cannot see, so its p95 sits far
/// too low — an adversarial probe fired the significance gate ~30% of the time on
/// pure noise built with exactly this structure.
///
/// A block permutation fixes it. Observations are ordered by `signal_time` and
/// partitioned into blocks spanning `news_window + horizon` minutes — the exact
/// reach over which one article touches adjacent observations and adjacent returns
/// overlap. Same-time symbols land in the same block, which absorbs the macro
/// fan-out. Each null draw permutes whole BLOCKS: within-block structure is
/// preserved, only the alignment of sentiment-blocks to return-positions is
/// randomized. The null then carries the same autocorrelation the real data does,
/// so p95 reflects the TRUE number of independent observations rather than the
/// inflated count of overlapping ones.
///
/// Returns the mean null spread, its 95th percentile (the significance bar), and
/// an add-one-corrected empirical p-value (share of null draws ≥ observed).
fn shuffled_spread(group: &[&NewsSignalObservation], observed: f64, seed: u64) -> (f64, f64, f64) {
    if group.len() < 3 {
        // Not a baseline — an admission that we cannot compute one. The verdict's
        // `min_observations` gate exists so this value is never actually compared
        // against anything. p = 1.0: nothing here is significant.
        return (0.0, 0.0, 1.0);
    }
    // Order by time so blocks are contiguous, and same-`signal_time` observations
    // (the macro fan-out across symbols) share a block.
    let mut ordered: Vec<&NewsSignalObservation> = group.to_vec();
    ordered.sort_by(|a, b| {
        (a.signal_time, a.observation_id.as_str()).cmp(&(b.signal_time, b.observation_id.as_str()))
    });
    let returns: Vec<f64> = ordered.iter().map(|row| row.future_return).collect();

    // All observations in a configuration share one window and one horizon, so the
    // block length is a property of the configuration.
    let block_len_minutes =
        (ordered[0].news_window_minutes + ordered[0].measurement_horizon_minutes).max(1);
    let start = ordered[0].signal_time;
    let mut blocks: Vec<Vec<f64>> = Vec::new();
    let mut current_bin: i64 = i64::MIN;
    for row in &ordered {
        let bin = (row.signal_time - start).num_minutes() / block_len_minutes;
        if bin != current_bin {
            blocks.push(Vec::new());
            current_bin = bin;
        }
        blocks.last_mut().unwrap().push(row.mean_sentiment);
    }

    let mut spreads = Vec::with_capacity(NULL_PERMUTATIONS);
    let mut at_or_above_observed = 0usize;
    for permutation in 0..NULL_PERMUTATIONS {
        // Fisher-Yates over BLOCK ORDER, not over individual observations.
        let mut order: Vec<usize> = (0..blocks.len()).collect();
        let mut state = seed
            .wrapping_mul(0x9E37_79B9_7F4A_7C15)
            .wrapping_add(permutation as u64);
        for i in (1..order.len()).rev() {
            state = state.wrapping_add(0x9E37_79B9_7F4A_7C15);
            let mut z = state;
            z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
            z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
            z ^= z >> 31;
            let j = (z % (i as u64 + 1)) as usize;
            order.swap(i, j);
        }
        let permuted: Vec<f64> = order
            .iter()
            .flat_map(|&block| blocks[block].iter().copied())
            .collect();
        let pairs: Vec<(f64, f64)> = permuted
            .iter()
            .copied()
            .zip(returns.iter().copied())
            .collect();
        let spread = top_minus_bottom(&pairs);
        if spread >= observed {
            at_or_above_observed += 1;
        }
        spreads.push(spread);
    }

    let mean_spread = spreads.iter().sum::<f64>() / spreads.len() as f64;
    spreads.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let p95 = spreads[((spreads.len() as f64 * 0.95) as usize).min(spreads.len() - 1)];
    // Add-one correction: the observed statistic is itself one draw from the null,
    // so a permutation p-value can never be exactly zero.
    let p_value = (at_or_above_observed as f64 + 1.0) / (NULL_PERMUTATIONS as f64 + 1.0);
    (mean_spread, p95, p_value)
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
        backtest::{BacktestParams, run_backtests_by_configuration, strategy_comparison},
        config::PipelineConfig,
        domain::observation::NewsSignalObservation,
        normalize::normalize_articles,
        observations::build_observations,
        source::fixture::generate_fixture,
    };

    /// A context with clean data, so these tests exercise the SIGNAL gates.
    /// The data-quality gates get their own dedicated tests in `verdict.rs`.
    ///
    /// `strategy_nets` is built from a real backtest over the same observations,
    /// because a missing entry is now a hard error (defaulting the baseline gate
    /// open would silently disable it).
    fn healthy_context(observations: &[NewsSignalObservation]) -> AnalysisContext {
        // 0.0 = no development split, so the spread is measured over the whole
        // group. These tests are about the SPREAD MATH, not the split; the split
        // has its own test (tests/no_threshold_lookahead.rs).
        let development_fraction = 0.0;
        let results = run_backtests_by_configuration(
            "analysis_test",
            observations,
            BacktestParams {
                long_quantile: 0.8,
                short_quantile: 0.2,
                cost_bps: 0.0,
                max_modal_share: 1.0,
                seed: 42,
                development_fraction,
            },
        );
        AnalysisContext {
            quarantine_rate: 0.0,
            lexicon_hit_rate: 1.0,
            vendor_agreement: 0.5,
            expected_sources: BTreeMap::from([
                ("finance_only".to_string(), 1),
                ("broad_news".to_string(), 1),
                ("finance_plus_broad".to_string(), 1),
            ]),
            article_sources: BTreeMap::new(),
            strategy_nets: strategy_comparison(&results),
            seed: 42,
            development_fraction,
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
        let observations = observations();
        let summaries =
            analyze_observations(&observations, &healthy_context(&observations)).unwrap();
        let configuration_count = configuration_groups(&observations).len();

        assert_eq!(summaries.len(), configuration_count);
        assert!(configuration_count > 1);
    }

    #[test]
    fn shuffled_baseline_is_worse_than_observed_spread_for_the_finance_only_configuration() {
        let observations = observations();
        let summaries =
            analyze_observations(&observations, &healthy_context(&observations)).unwrap();
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

        let summaries = analyze_observations(&group, &healthy_context(&group)).unwrap();

        assert_eq!(summaries.len(), 1);
        assert_eq!(summaries[0].observation_count, 7);
        assert!((summaries[0].observed_top_minus_bottom - 50.0).abs() < 1e-9);
    }
}
