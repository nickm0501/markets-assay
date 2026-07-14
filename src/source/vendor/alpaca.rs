use crate::{domain::market::PriceBar, ids::stable_id, source::vendor::parse_utc};
use anyhow::{Context, Result, bail};
use chrono::Duration;
use serde::Deserialize;
use std::collections::{BTreeMap, BTreeSet};

/// Alpaca's multi-symbol bars payload (`/v2/stocks/bars`).
#[derive(Debug, Deserialize)]
struct AlpacaPayload {
    #[serde(default)]
    bars: BTreeMap<String, Vec<AlpacaBar>>,
}

#[derive(Debug, Deserialize)]
struct AlpacaBar {
    /// Bar START time, RFC 3339.
    #[serde(default)]
    t: String,
    #[serde(default)]
    o: f64,
    #[serde(default)]
    h: f64,
    #[serde(default)]
    l: f64,
    #[serde(default)]
    c: f64,
    #[serde(default)]
    v: u64,
}

/// Parses bars, deriving `end_time` from the configured interval.
///
/// **This deliberately makes no assumption about bar alignment.** The NYSE
/// regular session opens at 09:30 ET, which is not an hour boundary, so an
/// hourly series may be session-aligned (09:30, 10:30, …) or clock-aligned
/// (09:00, 10:00, … with a partial first bar). That is open question S1-A, and
/// it cannot be answered from documentation — only from a real payload. A bar
/// whose timestamp will not parse is a hard error rather than a guess: unlike a
/// news article, a price bar with no time cannot be quarantined and reasoned
/// about later, because every return measurement depends on it.
pub fn parse_alpaca(json: &str, price_interval_minutes: i64) -> Result<Vec<PriceBar>> {
    let payload: AlpacaPayload =
        serde_json::from_str(json).context("failed to parse Alpaca bars payload")?;
    let mut bars = Vec::new();
    for (symbol, rows) in payload.bars {
        for row in rows {
            let Some(start_time) = parse_utc(&row.t) else {
                bail!(
                    "Alpaca bar for {symbol} has an unparseable timestamp {:?}; \
                     a price bar cannot be quarantined the way an article can, \
                     because every return measurement depends on its time",
                    row.t
                );
            };
            let end_time = start_time + Duration::minutes(price_interval_minutes);
            bars.push(PriceBar {
                bar_id: stable_id("bar", &(&symbol, start_time, end_time))?,
                symbol: symbol.clone(),
                start_time,
                end_time,
                open: row.o,
                high: row.h,
                low: row.l,
                close: row.c,
                volume: row.v,
            });
        }
    }
    bars.sort_by(|a, b| (a.start_time, &a.symbol).cmp(&(b.start_time, &b.symbol)));
    Ok(bars)
}

/// The distinct minute-of-hour values across every bar — the evidence for open
/// question S1-A. `{30}` means session-aligned (09:30, 10:30, …); `{0}` means
/// clock-aligned. Anything else means the grid is irregular, which would make
/// `build_observations`' contiguous-coverage rule (design.md Decision 3) drop
/// horizons silently, so it must be surfaced, not inferred.
pub fn observed_minute_offsets(bars: &[PriceBar]) -> BTreeSet<u32> {
    use chrono::Timelike;
    bars.iter().map(|bar| bar.start_time.minute()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn alpaca_payload_parses_into_price_bars() {
        let json = r#"{"bars":{"SPY":[
            {"t":"2025-03-04T14:30:00Z","o":100.0,"h":101.0,"l":99.5,"c":100.5,"v":1000}
        ]}}"#;

        let bars = parse_alpaca(json, 60).unwrap();

        assert_eq!(bars.len(), 1);
        assert_eq!(bars[0].symbol, "SPY");
        assert_eq!(bars[0].start_time.to_rfc3339(), "2025-03-04T14:30:00+00:00");
        assert_eq!(bars[0].end_time.to_rfc3339(), "2025-03-04T15:30:00+00:00");
        assert_eq!(bars[0].open, 100.0);
        assert_eq!(bars[0].close, 100.5);
    }

    #[test]
    fn an_unparseable_bar_timestamp_is_a_hard_error_not_a_guess() {
        let json = r#"{"bars":{"SPY":[{"t":"nonsense","o":1.0,"h":1.0,"l":1.0,"c":1.0,"v":1}]}}"#;

        let error = parse_alpaca(json, 60).unwrap_err().to_string();

        assert!(error.contains("unparseable timestamp"), "got: {error}");
    }

    #[test]
    fn observed_minute_offsets_expose_whether_bars_are_session_or_clock_aligned() {
        // The S1-A investigation, mechanized. Session-aligned NYSE hourly bars
        // sit at :30; clock-aligned ones sit at :00.
        let session_aligned = r#"{"bars":{"SPY":[
            {"t":"2025-03-04T14:30:00Z","o":1.0,"h":1.0,"l":1.0,"c":1.0,"v":1},
            {"t":"2025-03-04T15:30:00Z","o":1.0,"h":1.0,"l":1.0,"c":1.0,"v":1}
        ]}}"#;

        let offsets = observed_minute_offsets(&parse_alpaca(session_aligned, 60).unwrap());

        assert_eq!(offsets, BTreeSet::from([30]));
    }
}
