use crate::domain::article::SentimentLabel;
use std::{
    collections::{HashMap, HashSet},
    sync::LazyLock,
};

#[derive(Debug, Clone, PartialEq)]
pub struct SentimentResult {
    pub score: f64,
    pub label: SentimentLabel,
}

/// Bumped from `stage0_lexicon_v1` when the 14-word lexicon was replaced by
/// Loughran-McDonald + VADER (design.md Decision 20, 2026-07-14).
///
/// This string is recorded in every observation-set manifest, and that is the
/// point: an observation set scored by a different scorer **is a different
/// observation set**. Without the bump, a two-year backtest could silently mix
/// rows scored by two different scorers, which would be worthless.
pub const SENTIMENT_VERSION: &str = "stage2_lm_vader_v1";

// The lexicons are EMBEDDED, not read from disk at runtime.
//
// A scorer that loads a file at run time can change between the development year
// and the holdout year without anyone noticing, and the entire experiment rests
// on those two being scored identically. Compiling the words into the binary
// makes that impossible. It is also what lets `SENTIMENT_VERSION` mean something.
static LM_POSITIVE_RAW: &str = include_str!("../data/lexicons/loughran_mcdonald_positive.txt");
static LM_NEGATIVE_RAW: &str = include_str!("../data/lexicons/loughran_mcdonald_negative.txt");
static VADER_RAW: &str = include_str!("../data/lexicons/vader_lexicon.txt");

static LM_POSITIVE: LazyLock<HashSet<&'static str>> = LazyLock::new(|| {
    LM_POSITIVE_RAW
        .lines()
        .map(str::trim)
        .filter(|w| !w.is_empty())
        .collect()
});

static LM_NEGATIVE: LazyLock<HashSet<&'static str>> = LazyLock::new(|| {
    LM_NEGATIVE_RAW
        .lines()
        .map(str::trim)
        .filter(|w| !w.is_empty())
        .collect()
});

/// VADER's lexicon is `word \t mean_valence \t stddev \t raw_ratings`.
static VADER: LazyLock<HashMap<&'static str, f64>> = LazyLock::new(|| {
    VADER_RAW
        .lines()
        .filter_map(|line| {
            let mut parts = line.split('\t');
            let word = parts.next()?;
            let valence: f64 = parts.next()?.parse().ok()?;
            Some((word, valence))
        })
        .collect()
});

/// Words that flip the polarity of what follows. Headlines really do write
/// "not strong" and "fails to beat", and a scorer that reads those as positive
/// is worse than one that reads nothing at all.
const NEGATORS: [&str; 12] = [
    "not", "no", "never", "without", "lacks", "lacking", "fails", "failed", "cannot", "cant",
    "isnt", "wont",
];

fn tokenize(text: &str) -> Vec<String> {
    text.to_lowercase()
        .split(|c: char| !(c.is_ascii_alphabetic() || c == '\''))
        .filter(|token| !token.is_empty())
        .map(str::to_string)
        .collect()
}

/// Loughran-McDonald tone, length-normalized.
///
/// Divided by `sqrt(len)` rather than `len` because lexicon hits grow
/// sub-linearly with text length — dividing by raw length would systematically
/// punish Massive's 46-word descriptions relative to GDELT's 11-word headlines,
/// and the two must be comparable because they land in the same quantile.
fn lm_tone(tokens: &[String]) -> f64 {
    if tokens.is_empty() {
        return 0.0;
    }
    let positive = tokens
        .iter()
        .filter(|token| LM_POSITIVE.contains(token.as_str()))
        .count() as f64;
    let negative = tokens
        .iter()
        .filter(|token| LM_NEGATIVE.contains(token.as_str()))
        .count() as f64;
    if positive + negative == 0.0 {
        return 0.0;
    }
    (positive - negative) / (tokens.len() as f64).sqrt()
}

/// VADER valence sum with negation, normalized to (-1, 1) by VADER's own curve.
fn vader_compound(tokens: &[String]) -> f64 {
    let mut total = 0.0;
    for (index, token) in tokens.iter().enumerate() {
        let Some(valence) = VADER.get(token.as_str()).copied() else {
            continue;
        };
        // Compare the preceding token with apostrophes stripped. The tokenizer
        // PRESERVES apostrophes, so a real "isn't" stays "isn't" and would never
        // match the bare "isnt"/"cant"/"wont" in NEGATORS — the exact contractions
        // headlines actually use would silently fail to negate.
        let negated = index > 0 && {
            let previous = tokens[index - 1].replace('\'', "");
            NEGATORS.contains(&previous.as_str())
        };
        total += if negated { -valence * 0.74 } else { valence };
    }
    if total == 0.0 {
        return 0.0;
    }
    total / (total * total + 15.0).sqrt()
}

/// Does the scorer understand ANY of this text?
///
/// A miss scores exactly 0.0, which is indistinguishable from a genuinely
/// neutral article. Aggregate enough misses and the sentiment distribution
/// collapses into ties, the quantile thresholds stop separating anything, and
/// the backtest degenerates (`backtest::is_degenerate`). So the hit rate is a
/// first-class data-quality signal, not a curiosity: it is the difference
/// between "the news is neutral" and "we cannot read the news".
///
/// It is what caught the 14-word lexicon (`lexicon_hit_rate = 0.2021`), and it
/// stays here to catch the next one.
pub fn has_lexicon_hit(text: &str) -> bool {
    tokenize(text).iter().any(|token| {
        LM_POSITIVE.contains(token.as_str())
            || LM_NEGATIVE.contains(token.as_str())
            || VADER.contains_key(token.as_str())
    })
}

/// Loughran-McDonald + VADER, equally weighted.
///
/// Chosen by measurement, not reputation — see
/// `docs/superpowers/design/correlation-first-pipeline/2026-07-14-sentiment-scorer-bakeoff.md`.
/// They win *together* because they fail in different places: LM was built from
/// 10-K filings and reads Massive's fuller descriptions well but is 66% blind on
/// 11-word headlines; VADER was built for short punchy text and is the reverse.
/// Combined on the real sample: 2% blind on Massive, 27% on GDELT (down from 49%
/// and 94%), 128 distinct values (up from 7).
pub fn score_text(text: &str) -> SentimentResult {
    let tokens = tokenize(text);
    let score = (0.5 * lm_tone(&tokens) + 0.5 * vader_compound(&tokens)).clamp(-1.0, 1.0);
    let label = if score > 0.05 {
        SentimentLabel::Positive
    } else if score < -0.05 {
        SentimentLabel::Negative
    } else {
        SentimentLabel::Neutral
    };
    SentimentResult { score, label }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_lexicons_actually_loaded() {
        assert!(LM_POSITIVE.len() > 300, "got {}", LM_POSITIVE.len());
        assert!(LM_NEGATIVE.len() > 2000, "got {}", LM_NEGATIVE.len());
        assert!(VADER.len() > 7000, "got {}", VADER.len());
    }

    #[test]
    fn scores_positive_negative_and_neutral_text() {
        assert!(score_text("strong breakout boosts risk appetite").score > 0.0);
        assert!(score_text("weak downgrade negative shock").score < 0.0);
        assert_eq!(
            score_text("the meeting is scheduled").label,
            SentimentLabel::Neutral
        );
    }

    #[test]
    fn negation_flips_polarity() {
        // Headlines really do write this, and a scorer that reads "not strong"
        // as positive is worse than one that reads nothing.
        let plain = score_text("earnings were strong");
        let negated = score_text("earnings were not strong");

        assert!(plain.score > 0.0);
        assert!(
            negated.score < plain.score,
            "negation must reduce the score: plain={} negated={}",
            plain.score,
            negated.score
        );
    }

    #[test]
    fn a_contraction_negator_with_an_apostrophe_still_flips_polarity() {
        // The tokenizer keeps apostrophes, so "isn't" tokenizes to "isn't" — which
        // never matched the bare "isnt" in NEGATORS. The contractions headlines
        // actually write ("guidance isn't strong") silently failed to negate.
        let plain = score_text("guidance is strong");
        let negated = score_text("guidance isn't strong");

        assert!(plain.score > 0.0);
        assert!(
            negated.score < plain.score,
            "an apostrophe'd negator must reduce the score: plain={} negated={}",
            plain.score,
            negated.score
        );
    }

    #[test]
    fn the_new_scorer_reads_a_headline_the_old_one_was_blind_to() {
        // A real GDELT headline from the committed sample. The 14-word lexicon
        // scored this exactly 0.0 — indistinguishable from neutral. 94% of GDELT
        // looked like this.
        let real = "CNBC Daily Open : Tariffs led us down a different timeline for interest rates";

        assert!(has_lexicon_hit(real), "the scorer must at least SEE this");
        assert_ne!(score_text(real).score, 0.0);
    }

    #[test]
    fn scores_stay_bounded_in_minus_one_to_one() {
        let extreme_positive = "strong strong excellent excellent perfect outstanding superb";
        let extreme_negative = "catastrophic fraud bankruptcy litigation disaster collapse crisis";

        for text in [extreme_positive, extreme_negative] {
            let score = score_text(text).score;
            assert!((-1.0..=1.0).contains(&score), "{text} -> {score}");
        }
    }

    #[test]
    fn the_scorer_is_deterministic() {
        let text = "Broadcom beats on strong AI demand but guidance disappoints";

        assert_eq!(score_text(text).score, score_text(text).score);
    }

    #[test]
    fn resolution_is_continuous_not_seven_buckets() {
        // The old scorer emitted 7 values in 0.25 steps and therefore could not
        // rank. The strategy is quantile-based; ordering power is the ONLY
        // property it uses.
        let texts = [
            "profits surge on strong demand",
            "modest gains reported",
            "results were mixed",
            "shares slip on weak guidance",
            "company faces fraud litigation and bankruptcy",
        ];
        let scores: Vec<String> = texts
            .iter()
            .map(|t| format!("{:.6}", score_text(t).score))
            .collect();
        let distinct: std::collections::BTreeSet<&String> = scores.iter().collect();

        assert_eq!(
            distinct.len(),
            texts.len(),
            "scores must be distinguishable: {scores:?}"
        );
    }
}
