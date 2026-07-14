use crate::domain::article::SentimentLabel;

#[derive(Debug, Clone, PartialEq)]
pub struct SentimentResult {
    pub score: f64,
    pub label: SentimentLabel,
}

pub const SENTIMENT_VERSION: &str = "stage0_lexicon_v1";

pub fn score_text(text: &str) -> SentimentResult {
    let positive = [
        "strong",
        "breakout",
        "boosts",
        "relief",
        "positive",
        "constructive",
        "growth",
    ];
    let negative = [
        "weak",
        "downgrade",
        "negative",
        "shock",
        "weighs",
        "surprise",
        "hawkish",
    ];
    let lower = text.to_lowercase();
    let mut score = 0.0;
    for token in lower.split(|c: char| !c.is_ascii_alphabetic()) {
        if positive.contains(&token) {
            score += 1.0;
        }
        if negative.contains(&token) {
            score -= 1.0;
        }
    }
    let bounded = (score / 4.0_f64).clamp(-1.0, 1.0);
    let label = if bounded > 0.05 {
        SentimentLabel::Positive
    } else if bounded < -0.05 {
        SentimentLabel::Negative
    } else {
        SentimentLabel::Neutral
    };
    SentimentResult {
        score: bounded,
        label,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lexicon_scores_positive_negative_and_neutral_text() {
        assert!(score_text("strong breakout boosts risk appetite").score > 0.0);
        assert!(score_text("weak downgrade negative shock").score < 0.0);
        assert_eq!(
            score_text("mixed data remains neutral").label,
            crate::domain::article::SentimentLabel::Neutral
        );
    }

    #[test]
    fn sentiment_is_bounded() {
        let result = score_text("strong strong strong breakout relief positive boosts");
        assert!(result.score <= 1.0);
        assert!(result.score >= -1.0);
    }
}
