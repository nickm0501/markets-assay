use crate::{
    calendar::{is_trading_day, regular_open},
    config::PipelineConfig,
    domain::{
        article::{RawArticle, SourceKind},
        market::PriceBar,
    },
    ids::stable_id,
    source::{NewsSource, PriceSource},
};
use anyhow::Result;
use chrono::{Duration, TimeZone, Utc};

#[derive(Debug, Clone, PartialEq)]
pub struct FixtureData {
    pub raw_articles: Vec<RawArticle>,
    pub price_bars: Vec<PriceBar>,
}

/// Stage 0's made-up data, now reachable through the same trait the saved-file
/// reader and (later) the live-API source implement. The generation logic below
/// is unchanged from when it was hardwired into `pipeline.rs`.
pub struct FixtureSource;

impl NewsSource for FixtureSource {
    fn vendor_names(&self) -> Vec<String> {
        vec!["fixture".to_string()]
    }

    fn fetch_raw_articles(&self, config: &PipelineConfig) -> Result<Vec<RawArticle>> {
        Ok(generate_fixture(config)?.raw_articles)
    }
}

impl PriceSource for FixtureSource {
    fn vendor_names(&self) -> Vec<String> {
        vec!["fixture".to_string()]
    }

    fn fetch_price_bars(&self, config: &PipelineConfig) -> Result<Vec<PriceBar>> {
        Ok(generate_fixture(config)?.price_bars)
    }
}

pub fn generate_fixture(config: &PipelineConfig) -> Result<FixtureData> {
    let raw_articles = vec![
        article(
            "massive-1",
            "fixture_finance",
            SourceKind::Finance,
            "2026-06-29T14:05:00Z",
            "SPY breakout on broad earnings strength",
            "Analysts call the move strong and constructive.",
            "fixture://spy-breakout",
            vec!["SPY"],
            vec![],
        ),
        article(
            "massive-1-dup",
            "fixture_wire",
            SourceKind::Finance,
            "2026-06-29T14:07:00Z",
            "SPY breakout on broad earnings strength",
            "Analysts call the move strong and constructive.",
            "fixture://spy-breakout",
            vec!["SPY"],
            vec![],
        ),
        article(
            "massive-2",
            "fixture_finance",
            SourceKind::Finance,
            "2026-06-30T15:05:00Z",
            "QQQ downgrade follows weak chip demand",
            "The note described weak orders and negative guidance.",
            "fixture://qqq-downgrade",
            vec!["QQQ"],
            vec!["technology"],
        ),
        article(
            "gdelt-1",
            "fixture_macro",
            SourceKind::Broad,
            "2026-07-01T13:40:00Z",
            "Rates relief boosts risk appetite",
            "Lower yields are positive for growth stocks and broad indexes.",
            "fixture://rates-relief",
            vec![],
            vec!["rates"],
        ),
        article(
            "gdelt-2",
            "fixture_macro",
            SourceKind::Broad,
            "2026-07-02T21:15:00Z",
            "Fed shock weighs on markets",
            "A surprise hawkish turn is negative for stocks after the close.",
            "fixture://after-close",
            vec![],
            vec!["rates"],
        ),
        article(
            "gdelt-3",
            "fixture_housing",
            SourceKind::Broad,
            "2026-07-06T14:10:00Z",
            "Housing data remains neutral",
            "Mixed data left investors with a neutral read.",
            "fixture://housing-neutral",
            vec![],
            vec!["housing"],
        ),
    ];
    let price_bars = generate_price_bars(config)?;
    Ok(FixtureData {
        raw_articles,
        price_bars,
    })
}

#[allow(clippy::too_many_arguments)]
fn article(
    vendor_id: &str,
    source: &str,
    source_kind: SourceKind,
    published_at: &str,
    title: &str,
    summary: &str,
    url: &str,
    tickers: Vec<&str>,
    themes: Vec<&str>,
) -> RawArticle {
    RawArticle {
        vendor_id: vendor_id.into(),
        source: source.into(),
        source_kind,
        published_at: Some(published_at.parse().unwrap()),
        published_at_raw: published_at.into(),
        title: title.into(),
        summary: summary.into(),
        url: url.into(),
        tickers: tickers.into_iter().map(String::from).collect(),
        themes: themes.into_iter().map(String::from).collect(),
    }
}

fn generate_price_bars(config: &PipelineConfig) -> Result<Vec<PriceBar>> {
    let mut bars = Vec::new();
    let mut date = Utc
        .with_ymd_and_hms(2026, 6, 29, 0, 0, 0)
        .unwrap()
        .date_naive();
    while date
        <= Utc
            .with_ymd_and_hms(2026, 7, 7, 0, 0, 0)
            .unwrap()
            .date_naive()
    {
        if is_trading_day(date, &config.holidays) {
            for symbol in &config.symbols {
                let mut start = regular_open(date);
                let mut price = if symbol == "SPY" { 500.0 } else { 425.0 };
                while start < crate::calendar::regular_close(date, &config.early_closes) {
                    let ret = fixture_return(symbol, start);
                    let open = price;
                    let close = open * (1.0 + ret);
                    let high = open.max(close) * 1.001;
                    let low = open.min(close) * 0.999;
                    let end_time = start + Duration::minutes(config.price_interval_minutes);
                    let bar_id = stable_id("bar", &(symbol, start, end_time))?;
                    bars.push(PriceBar {
                        bar_id,
                        symbol: symbol.clone(),
                        start_time: start,
                        end_time,
                        open,
                        high,
                        low,
                        close,
                        volume: 1_000_000,
                    });
                    price = close;
                    start = end_time;
                }
            }
        }
        date = date.succ_opt().unwrap();
    }
    Ok(bars)
}

fn fixture_return(symbol: &str, start: chrono::DateTime<Utc>) -> f64 {
    match (symbol, start.to_rfc3339().as_str()) {
        ("SPY", "2026-06-29T15:30:00+00:00") => 0.008,
        ("QQQ", "2026-06-30T16:30:00+00:00") => -0.009,
        ("SPY", "2026-07-01T14:30:00+00:00") => 0.006,
        ("QQQ", "2026-07-01T14:30:00+00:00") => 0.007,
        ("SPY", "2026-07-06T13:30:00+00:00") => -0.007,
        ("QQQ", "2026-07-06T13:30:00+00:00") => -0.008,
        _ => 0.0005,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::PipelineConfig;

    fn config() -> PipelineConfig {
        PipelineConfig::load("configs/stage0_fixture.json").unwrap()
    }

    #[test]
    fn fixture_generation_is_deterministic() {
        let first = generate_fixture(&config()).unwrap();
        let second = generate_fixture(&config()).unwrap();

        assert_eq!(first.raw_articles, second.raw_articles);
        assert_eq!(first.price_bars, second.price_bars);
    }

    #[test]
    fn fixture_contains_positive_negative_macro_theme_duplicate_and_after_hours_cases() {
        let fixture = generate_fixture(&config()).unwrap();

        assert!(
            fixture
                .raw_articles
                .iter()
                .any(|a| a.title.contains("breakout"))
        );
        assert!(
            fixture
                .raw_articles
                .iter()
                .any(|a| a.title.contains("downgrade"))
        );
        assert!(
            fixture
                .raw_articles
                .iter()
                .any(|a| a.themes.contains(&"rates".to_string()))
        );
        assert!(
            fixture
                .raw_articles
                .iter()
                .any(|a| a.themes.contains(&"technology".to_string()))
        );
        assert!(
            fixture
                .raw_articles
                .iter()
                .filter(|a| a.url == "fixture://spy-breakout")
                .count()
                >= 2
        );
        assert!(fixture.raw_articles.iter().any(|a| {
            a.published_at
                .is_some_and(|at| at.to_rfc3339() == "2026-07-02T21:15:00+00:00")
        }));
    }

    #[test]
    fn fixture_prices_include_known_positive_and_negative_future_moves() {
        let fixture = generate_fixture(&config()).unwrap();
        let spy_returns: Vec<f64> = fixture
            .price_bars
            .iter()
            .filter(|bar| bar.symbol == "SPY")
            .map(|bar| bar.return_pct())
            .collect();
        let qqq_returns: Vec<f64> = fixture
            .price_bars
            .iter()
            .filter(|bar| bar.symbol == "QQQ")
            .map(|bar| bar.return_pct())
            .collect();

        assert!(spy_returns.iter().any(|value| *value > 0.006));
        assert!(qqq_returns.iter().any(|value| *value < -0.006));
    }
}
