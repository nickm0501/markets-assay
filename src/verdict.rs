use crate::config::VerdictThresholds;
use serde::{Deserialize, Serialize};

/// What the data itself looks like, independent of whether sentiment predicts
/// anything. These are the inputs the fixture could never produce — synthetic
/// data has no quarantine rate and no vocabulary gap to react to — which is why
/// Stage 0 could only implement `continue`/`revise` (design.md Decision 6), and
/// why Stage 1 is the first place `stop`/`expand data`/`expand sources` can
/// actually be tested rather than merely written.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct DataQualityMetrics {
    /// Share of articles QUARANTINED — broken timestamps, missing text. Scope
    /// exclusions (out-of-window, wrong symbol, duplicate) must NOT be counted
    /// here: an article outside our date range is perfectly sound, and letting
    /// a sample boundary inflate this number would trip `stop` for no reason.
    pub quarantine_rate: f64,
    pub articles_per_signal: f64,
    pub source_set_coverage: f64,
    pub lexicon_hit_rate: f64,
    pub degenerate: bool,
}

/// Whether sentiment appears to predict returns. Only meaningful once the data
/// is known to be usable.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct SignalMetrics {
    pub observed_top_minus_bottom: f64,
    pub shuffled_top_minus_bottom: f64,
    /// How many observations the spread was computed from. A spread from two
    /// rows is not a weak result — it is not a result.
    pub observation_count: u32,
}

/// A verdict, plus the specific measurement that produced it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Verdict {
    pub recommendation: String,
    /// The metric that decided it, and its value. A verdict a human cannot
    /// audit is ceremony — if a Stage 3 run says `stop`, the very next question
    /// will be "because of what?", and the report must already answer it.
    pub reason: String,
}

/// The spec's five decision values, in the order that matters.
///
/// **Data-quality gates run BEFORE signal gates, and that ordering is
/// load-bearing.** A top-minus-bottom spread computed over articles whose
/// timestamps are unreliable, or whose sentiment scores are all tied at zero, is
/// not a weak signal result — it is not a signal result at all. Check the other
/// way round and a Stage 3 run could report `continue` on the strength of a
/// spread it had no business computing, which is precisely the false positive
/// this whole pipeline exists to avoid.
pub fn verdict(
    quality: &DataQualityMetrics,
    signal: &SignalMetrics,
    thresholds: &VerdictThresholds,
) -> Verdict {
    let decide = |recommendation: &str, reason: String| Verdict {
        recommendation: recommendation.to_string(),
        reason,
    };

    if quality.quarantine_rate > thresholds.max_quarantine_rate {
        return decide(
            "stop",
            format!(
                "quarantine_rate {:.3} exceeds max {:.3}: publication times are too unreliable to trust any result",
                quality.quarantine_rate, thresholds.max_quarantine_rate
            ),
        );
    }
    if quality.degenerate {
        return decide(
            "expand sources",
            "sentiment distribution is degenerate: the scores are too tied to separate a top from a bottom, so there is nothing to test".to_string(),
        );
    }
    if quality.lexicon_hit_rate < thresholds.min_lexicon_hit_rate {
        return decide(
            "expand sources",
            format!(
                "lexicon_hit_rate {:.3} is below min {:.3}: the scorer is not reading most of the news it is given",
                quality.lexicon_hit_rate, thresholds.min_lexicon_hit_rate
            ),
        );
    }
    if quality.source_set_coverage < thresholds.min_source_coverage {
        return decide(
            "expand sources",
            format!(
                "source_set_coverage {:.3} is below min {:.3}: fewer sources are contributing than the configuration claims",
                quality.source_set_coverage, thresholds.min_source_coverage
            ),
        );
    }
    if quality.articles_per_signal < thresholds.min_articles_per_signal {
        return decide(
            "expand data",
            format!(
                "articles_per_signal {:.3} is below min {:.3}: too little news per signal to aggregate",
                quality.articles_per_signal, thresholds.min_articles_per_signal
            ),
        );
    }

    // A configuration with too few observations cannot be judged at all.
    //
    // This is not pedantry. `shuffled_spread` returns 0.0 BY FIAT for any group
    // smaller than 3 — it does not compute a baseline, it invents one. Comparing
    // a real spread against a fabricated zero and declaring victory is how a
    // 2-observation configuration ended up reporting `continue`.
    if signal.observation_count < thresholds.min_observations {
        return decide(
            "expand data",
            format!(
                "only {} observations, below the minimum {} needed to judge a configuration at all",
                signal.observation_count, thresholds.min_observations
            ),
        );
    }

    // Only now is a signal result meaningful.
    //
    // The margin is NOT decoration. Comparing `observed > shuffled` directly let
    // the verdict be decided by floating-point noise: two spreads that are equal
    // to fifteen decimal places still compare as `>` because they were summed in
    // a different order, and three fixture configurations reported `continue` on
    // the strength of a 1e-18 difference. An edge you can only see past the 15th
    // decimal place is not an edge, it is rounding.
    let margin = signal.observed_top_minus_bottom - signal.shuffled_top_minus_bottom;
    if margin > thresholds.min_spread_margin
        && signal.observed_top_minus_bottom > thresholds.min_spread_margin
    {
        return decide(
            "continue",
            format!(
                "observed spread {:.8} beats the shuffled baseline {:.8} by {:.8} (> min margin {:.8}) and is positive",
                signal.observed_top_minus_bottom,
                signal.shuffled_top_minus_bottom,
                margin,
                thresholds.min_spread_margin
            ),
        );
    }
    decide(
        "revise",
        format!(
            "observed spread {:.8} does not beat the shuffled baseline {:.8} by a meaningful margin (got {:.8}, need > {:.8})",
            signal.observed_top_minus_bottom,
            signal.shuffled_top_minus_bottom,
            margin,
            thresholds.min_spread_margin
        ),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn healthy_quality() -> DataQualityMetrics {
        DataQualityMetrics {
            quarantine_rate: 0.0,
            articles_per_signal: 3.0,
            source_set_coverage: 1.0,
            lexicon_hit_rate: 0.9,
            degenerate: false,
        }
    }

    fn positive_signal() -> SignalMetrics {
        SignalMetrics {
            observed_top_minus_bottom: 0.01,
            shuffled_top_minus_bottom: 0.001,
            observation_count: 100,
        }
    }

    #[test]
    fn a_spread_that_beats_the_baseline_only_by_floating_point_noise_is_not_a_continue() {
        // THE FALSE-POSITIVE GENERATOR, pinned.
        //
        // Found on the fixture 2026-07-14: three configurations reported
        // `continue` because `observed > shuffled` was TRUE for two numbers that
        // are equal to fifteen decimal places — they differed only by the order
        // the floats were summed in. An edge visible only past the 15th decimal
        // place is rounding, not signal.
        let noise = SignalMetrics {
            observed_top_minus_bottom: 0.0032000000000000004,
            shuffled_top_minus_bottom: 0.0032,
            observation_count: 100,
        };
        assert!(
            noise.observed_top_minus_bottom > noise.shuffled_top_minus_bottom,
            "precondition: the naive comparison really does say 'greater'"
        );

        let verdict = verdict(&healthy_quality(), &noise, &VerdictThresholds::default());

        assert_eq!(verdict.recommendation, "revise");
    }

    #[test]
    fn an_exactly_zero_spread_is_never_a_continue() {
        // The fixture's finance_only/60 configuration reported:
        //   "observed spread 0.00000000 beats the shuffled baseline 0.00000000
        //    and is positive"
        // ...which is nonsense on its face, and was true only in float terms.
        let zero = SignalMetrics {
            observed_top_minus_bottom: 0.0,
            shuffled_top_minus_bottom: 0.0,
            observation_count: 100,
        };

        assert_eq!(
            verdict(&healthy_quality(), &zero, &VerdictThresholds::default()).recommendation,
            "revise"
        );
    }

    #[test]
    fn a_configuration_with_too_few_observations_cannot_be_judged() {
        // `shuffled_spread` returns 0.0 BY FIAT below 3 observations — it invents
        // a baseline rather than computing one. A 2-observation configuration
        // "beating" that fabricated zero is not a finding, and it reported
        // `continue` on the fixture until this gate existed.
        let tiny = SignalMetrics {
            observed_top_minus_bottom: 0.05,
            shuffled_top_minus_bottom: 0.0,
            observation_count: 2,
        };

        let verdict = verdict(&healthy_quality(), &tiny, &VerdictThresholds::default());

        assert_eq!(verdict.recommendation, "expand data");
        assert!(
            verdict.reason.contains("only 2 observations"),
            "got: {}",
            verdict.reason
        );
    }

    #[test]
    fn a_high_quarantine_rate_yields_stop() {
        let quality = DataQualityMetrics {
            quarantine_rate: 0.5,
            ..healthy_quality()
        };

        let verdict = verdict(&quality, &positive_signal(), &VerdictThresholds::default());

        assert_eq!(verdict.recommendation, "stop");
        assert!(verdict.reason.contains("quarantine_rate"));
    }

    #[test]
    fn a_high_exclusion_rate_does_not_yield_stop() {
        // The counter-test for the quarantine/exclusion split. A sample full of
        // out-of-window articles is a SCOPE fact, not a data-quality failure.
        // Exclusions never enter DataQualityMetrics at all — that is the whole
        // point — so a run can discard thousands of them and still `continue`.
        let quality = healthy_quality();

        let verdict = verdict(&quality, &positive_signal(), &VerdictThresholds::default());

        assert_eq!(verdict.recommendation, "continue");
    }

    #[test]
    fn a_degenerate_sentiment_distribution_yields_expand_sources() {
        let quality = DataQualityMetrics {
            degenerate: true,
            ..healthy_quality()
        };

        let verdict = verdict(&quality, &positive_signal(), &VerdictThresholds::default());

        assert_eq!(verdict.recommendation, "expand sources");
        assert!(verdict.reason.contains("degenerate"));
    }

    #[test]
    fn a_lexicon_that_misses_most_headlines_yields_expand_sources() {
        let quality = DataQualityMetrics {
            lexicon_hit_rate: 0.02,
            ..healthy_quality()
        };

        let verdict = verdict(&quality, &positive_signal(), &VerdictThresholds::default());

        assert_eq!(verdict.recommendation, "expand sources");
        assert!(verdict.reason.contains("lexicon_hit_rate"));
    }

    #[test]
    fn too_few_articles_per_signal_yields_expand_data() {
        let quality = DataQualityMetrics {
            articles_per_signal: 0.2,
            ..healthy_quality()
        };

        let verdict = verdict(&quality, &positive_signal(), &VerdictThresholds::default());

        assert_eq!(verdict.recommendation, "expand data");
        assert!(verdict.reason.contains("articles_per_signal"));
    }

    #[test]
    fn data_quality_gates_are_evaluated_before_signal_gates() {
        // THE test that stops Stage 3 reporting a false positive.
        //
        // Here the signal looks great — the observed spread beats the shuffled
        // baseline and is positive — but the data is unusable. A pipeline that
        // checked signal first would proudly say `continue` on the strength of
        // a number it had no business computing.
        let unusable_data = DataQualityMetrics {
            quarantine_rate: 0.9,
            degenerate: true,
            lexicon_hit_rate: 0.0,
            ..healthy_quality()
        };
        let excellent_signal = SignalMetrics {
            observed_top_minus_bottom: 0.25,
            shuffled_top_minus_bottom: 0.0001,
            observation_count: 100,
        };

        let verdict = verdict(
            &unusable_data,
            &excellent_signal,
            &VerdictThresholds::default(),
        );

        assert_ne!(verdict.recommendation, "continue");
        assert_eq!(verdict.recommendation, "stop");
    }

    #[test]
    fn a_positive_spread_over_usable_data_yields_continue() {
        let verdict = verdict(
            &healthy_quality(),
            &positive_signal(),
            &VerdictThresholds::default(),
        );

        assert_eq!(verdict.recommendation, "continue");
    }

    #[test]
    fn a_spread_that_loses_to_the_shuffled_baseline_yields_revise() {
        let weak = SignalMetrics {
            observed_top_minus_bottom: 0.0001,
            shuffled_top_minus_bottom: 0.01,
            observation_count: 100,
        };

        let verdict = verdict(&healthy_quality(), &weak, &VerdictThresholds::default());

        assert_eq!(verdict.recommendation, "revise");
    }

    #[test]
    fn every_verdict_value_in_the_spec_is_reachable() {
        // The spec's Decision Demo ends on one of exactly five values. Stage 0
        // could only ever emit two of them. All five must now be reachable, or
        // the vocabulary is decoration.
        let thresholds = VerdictThresholds::default();
        let reached: std::collections::BTreeSet<String> = [
            verdict(
                &DataQualityMetrics {
                    quarantine_rate: 0.9,
                    ..healthy_quality()
                },
                &positive_signal(),
                &thresholds,
            ),
            verdict(
                &DataQualityMetrics {
                    degenerate: true,
                    ..healthy_quality()
                },
                &positive_signal(),
                &thresholds,
            ),
            verdict(
                &DataQualityMetrics {
                    articles_per_signal: 0.1,
                    ..healthy_quality()
                },
                &positive_signal(),
                &thresholds,
            ),
            verdict(&healthy_quality(), &positive_signal(), &thresholds),
            verdict(
                &healthy_quality(),
                &SignalMetrics {
                    observed_top_minus_bottom: -0.01,
                    shuffled_top_minus_bottom: 0.01,
                    observation_count: 100,
                },
                &thresholds,
            ),
        ]
        .into_iter()
        .map(|verdict| verdict.recommendation)
        .collect();

        assert_eq!(
            reached,
            [
                "continue",
                "expand data",
                "expand sources",
                "revise",
                "stop"
            ]
            .iter()
            .map(|value| value.to_string())
            .collect()
        );
    }

    #[test]
    fn every_non_continue_verdict_names_the_metric_that_tripped_it() {
        let verdicts = [
            verdict(
                &DataQualityMetrics {
                    quarantine_rate: 0.9,
                    ..healthy_quality()
                },
                &positive_signal(),
                &VerdictThresholds::default(),
            ),
            verdict(
                &DataQualityMetrics {
                    articles_per_signal: 0.1,
                    ..healthy_quality()
                },
                &positive_signal(),
                &VerdictThresholds::default(),
            ),
        ];

        for verdict in verdicts {
            assert_ne!(verdict.recommendation, "continue");
            assert!(
                !verdict.reason.is_empty(),
                "an unauditable verdict is ceremony"
            );
        }
    }
}
