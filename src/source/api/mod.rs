pub mod http;

use crate::{
    config::PipelineConfig,
    domain::{article::RawArticle, market::PriceBar},
    source::{
        NewsSource, PriceSource,
        vendor::{alpaca::parse_alpaca, gdelt::parse_gdelt, massive::parse_massive},
    },
};
use anyhow::{Result, bail};
use http::{CachingHttpClient, IngestLogRow, RateLimit, credential};
use std::{cell::RefCell, path::Path};

/// The live-API source. Stage 2's job; the trait seam means it touches **no
/// research code at all** — `analysis.rs`, `backtest.rs` and `observations.rs`
/// never learn where their data came from. That was the entire point of building
/// the seam in Stage 1.
///
/// It reuses the Stage 1 vendor PARSERS unchanged. Those are already proven
/// against real payloads, and sharing them is what stops the saved-file path and
/// the live path from silently drifting apart — the spec requires fixture,
/// saved-file and API modes be indistinguishable downstream.
pub struct ApiSource {
    client: RefCell<CachingHttpClient>,
}

impl ApiSource {
    pub fn new(cache_dir: &Path) -> Result<Self> {
        Ok(Self {
            client: RefCell::new(CachingHttpClient::new(cache_dir)?),
        })
    }

    pub fn log(&self) -> Vec<IngestLogRow> {
        self.client.borrow().log.clone()
    }
}

impl NewsSource for ApiSource {
    fn vendor_names(&self) -> Vec<String> {
        vec!["massive".into(), "gdelt".into()]
    }

    fn ingest_log(&self) -> Option<Vec<IngestLogRow>> {
        Some(self.log())
    }

    fn fetch_raw_articles(&self, config: &PipelineConfig) -> Result<Vec<RawArticle>> {
        let (Some(start), Some(end)) = (config.dataset_date_start, config.dataset_date_end) else {
            bail!(
                "the api source needs dataset_date_start and dataset_date_end — \
                 an unbounded news fetch is not a dataset, it is a denial of service"
            );
        };

        let mut articles = Vec::new();
        let key = credential("MASSIVE_API_KEY")?;
        let mut client = self.client.borrow_mut();

        for symbol in &config.symbols {
            // PAGINATION IS MANDATORY. Stage 1 learned this the hard way: ignoring
            // Alpaca's cursor silently returned 8 of 12 symbols, and a missing
            // symbol is indistinguishable from a symbol nobody wrote about.
            let mut page = 0;
            let mut cursor: Option<String> = None;
            loop {
                let mut query = vec![
                    ("ticker".to_string(), symbol.clone()),
                    ("published_utc.gte".to_string(), start.to_string()),
                    ("published_utc.lte".to_string(), end.to_string()),
                    ("limit".to_string(), "1000".to_string()),
                    ("order".to_string(), "asc".to_string()),
                ];
                if let Some(token) = &cursor {
                    query.push(("cursor".to_string(), token.clone()));
                }
                let body = client.get(
                    "massive",
                    "https://api.massive.com/v2/reference/news",
                    &query,
                    // The key goes in a header, and the CACHE KEY below omits it —
                    // rotating a key must not invalidate a two-year fetch.
                    &[("Authorization".into(), format!("Bearer {key}"))],
                    &format!("massive:news:{symbol}:{start}:{end}:{page}"),
                    RateLimit::massive(),
                )?;
                articles.extend(parse_massive(&body)?);

                cursor = serde_json::from_str::<serde_json::Value>(&body)
                    .ok()
                    .and_then(|v| v.get("next_url").and_then(|u| u.as_str()).map(String::from))
                    .and_then(|u| {
                        u.split("cursor=")
                            .nth(1)
                            .map(|c| c.split('&').next().unwrap_or(c).to_string())
                    });
                if cursor.is_none() {
                    break;
                }
                page += 1;
            }
        }

        // GDELT needs no key, and rate-limits anyway.
        //
        // CHUNK BY DAY. `maxrecords=250` caps the ENTIRE response, not a page — a
        // single request over a two-year range returns 250 articles total and
        // nothing complains, the exact "missing data looks like no data" failure
        // the Alpaca pagination bug already taught us. One request per day keeps
        // each chunk under the cap and makes any gap visible in the ingest log,
        // the same shape as the Massive per-symbol loop above.
        let mut day = start;
        loop {
            let body = client.get(
                "gdelt",
                "https://api.gdeltproject.org/api/v2/doc/doc",
                &[
                    (
                        "query".to_string(),
                        "(stocks OR \"federal reserve\" OR inflation OR \"interest rates\") sourcelang:english"
                            .to_string(),
                    ),
                    (
                        "startdatetime".to_string(),
                        format!("{}000000", day.format("%Y%m%d")),
                    ),
                    (
                        "enddatetime".to_string(),
                        format!("{}235959", day.format("%Y%m%d")),
                    ),
                    ("mode".to_string(), "ArtList".to_string()),
                    ("maxrecords".to_string(), "250".to_string()),
                    ("format".to_string(), "json".to_string()),
                ],
                &[],
                &format!("gdelt:macro:{day}"),
                RateLimit::gdelt(),
            )?;
            articles.extend(parse_gdelt(&body)?);
            if day >= end {
                break;
            }
            let Some(next) = day.succ_opt() else { break };
            day = next;
        }

        if articles.is_empty() {
            bail!("the api source returned zero articles; refusing to build an empty dataset");
        }
        Ok(articles)
    }
}

/// Alpaca's timeframe units, which are NOT just "minutes".
///
/// It rejects any minute unit above 59 — `60Min` is HTTP 400, "timeframe period
/// number is larger than the allowed maximum of 59". Found by running it. The
/// spec's primary interval is one hour, which must be spelled `1Hour`, and its
/// secondary is 15 minutes, which must be `15Min`.
fn alpaca_timeframe(minutes: i64) -> Result<String> {
    Ok(match minutes {
        1..=59 => format!("{minutes}Min"),
        60 => "1Hour".to_string(),
        120 => "2Hour".to_string(),
        240 => "4Hour".to_string(),
        1440 => "1Day".to_string(),
        other => bail!(
            "alpaca has no timeframe for {other} minutes. It accepts 1-59 Min, \
             N Hour, or 1 Day — not arbitrary minute counts."
        ),
    })
}

impl PriceSource for ApiSource {
    fn vendor_names(&self) -> Vec<String> {
        vec!["alpaca".into()]
    }

    fn fetch_price_bars(&self, config: &PipelineConfig) -> Result<Vec<PriceBar>> {
        let (Some(start), Some(end)) = (config.dataset_date_start, config.dataset_date_end) else {
            bail!("the api source needs dataset_date_start and dataset_date_end");
        };
        let id = credential("APCA_API_KEY_ID")?;
        let secret = credential("APCA_API_SECRET_KEY")?;
        let mut client = self.client.borrow_mut();

        let mut bars = Vec::new();
        let mut page = 0;
        let mut token: Option<String> = None;
        loop {
            let mut query = vec![
                ("symbols".to_string(), config.symbols.join(",")),
                (
                    "timeframe".to_string(),
                    alpaca_timeframe(config.price_interval_minutes)?,
                ),
                ("start".to_string(), format!("{start}T00:00:00Z")),
                ("end".to_string(), format!("{end}T23:59:59Z")),
                ("limit".to_string(), "10000".to_string()),
                ("adjustment".to_string(), "all".to_string()),
                // CAVEAT: `iex` is the free feed — a single venue (~2-3% of
                // consolidated volume), not the SIP tape. Its bars can be thin or
                // absent for less-liquid names and its prices need not match the
                // NBBO, so any Stage 3 result that leans on these fill prices must
                // be read with that in mind. Upgrading to `sip` is a feed change
                // here and nothing else downstream.
                ("feed".to_string(), "iex".to_string()),
            ];
            if let Some(t) = &token {
                query.push(("page_token".to_string(), t.clone()));
            }
            let body = client.get(
                "alpaca",
                "https://data.alpaca.markets/v2/stocks/bars",
                &query,
                &[
                    ("APCA-API-KEY-ID".into(), id.clone()),
                    ("APCA-API-SECRET-KEY".into(), secret.clone()),
                ],
                &format!(
                    "alpaca:bars:{}:{start}:{end}:{page}",
                    config.symbols.join(",")
                ),
                RateLimit::alpaca(),
            )?;
            bars.extend(parse_alpaca(&body, config.price_interval_minutes)?);

            token = serde_json::from_str::<serde_json::Value>(&body)
                .ok()
                .and_then(|v| {
                    v.get("next_page_token")
                        .and_then(|t| t.as_str())
                        .map(String::from)
                });
            if token.is_none() {
                break;
            }
            page += 1;
        }

        // Every requested symbol must come back. A silently missing symbol looks
        // exactly like a symbol with no data — Stage 1's Alpaca fetch lost SPY,
        // QQQ, NVDA and TSLA this way and nothing complained.
        let returned: std::collections::BTreeSet<&str> =
            bars.iter().map(|bar| bar.symbol.as_str()).collect();
        let missing: Vec<&String> = config
            .symbols
            .iter()
            .filter(|symbol| !returned.contains(symbol.as_str()))
            .collect();
        if !missing.is_empty() {
            bail!(
                "alpaca returned no bars for {missing:?}. Refusing to build a partial dataset — \
                 a missing symbol is indistinguishable from a symbol with no data."
            );
        }
        Ok(bars)
    }
}
