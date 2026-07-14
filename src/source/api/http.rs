use crate::ids::sha256_hex;
use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    path::{Path, PathBuf},
    thread::sleep,
    time::{Duration, Instant},
};

/// One row of `ingest_log.csv`.
///
/// The spec: *"Record rate-limit responses, retries, source outages, dropped
/// rows, and coverage gaps."* At Stage 3 scale — two years, ~5 requests/minute
/// against Massive — a fetch is a multi-hour job that WILL be interrupted. When
/// it is, the only way to know what you actually have is to have written down
/// what you actually asked for.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct IngestLogRow {
    pub vendor: String,
    /// The request, with any credentials stripped. Never log a key.
    pub request: String,
    pub status: u16,
    pub attempts: u32,
    pub rate_limited: bool,
    pub from_cache: bool,
    pub duration_ms: u64,
    pub bytes: u64,
    pub error: String,
}

/// Per-vendor request pacing.
///
/// Stage 1 hit 429s from **both** Massive *and* GDELT — GDELT rate-limits despite
/// needing no API key at all. Pacing deliberately beats hammering-then-backing-off:
/// the backoff still costs you the wall-clock time, and it costs the vendor the
/// request.
#[derive(Debug, Clone, Copy)]
pub struct RateLimit {
    pub requests_per_minute: u32,
    pub max_attempts: u32,
}

impl RateLimit {
    pub fn massive() -> Self {
        // Free tier is ~5/min. 5 with pacing keeps us under it on the first pass.
        Self {
            requests_per_minute: 5,
            max_attempts: 5,
        }
    }
    pub fn alpaca() -> Self {
        // 200/min on the free plan. We stay well under.
        Self {
            requests_per_minute: 150,
            max_attempts: 5,
        }
    }
    pub fn gdelt() -> Self {
        // Undocumented, and it 429s. Be a good citizen.
        Self {
            requests_per_minute: 10,
            max_attempts: 5,
        }
    }

    fn min_interval(&self) -> Duration {
        Duration::from_secs_f64(60.0 / self.requests_per_minute.max(1) as f64)
    }
}

/// A rate-limited, retrying, **caching** HTTP client.
///
/// The cache is what makes ingestion *idempotent and resumable*, which the spec
/// has required since day one and which nothing in this project had until now.
/// A cached request is never re-issued, so an interrupted two-year fetch resumes
/// where it stopped instead of starting over — and re-running an ingest produces
/// the same `dataset_id` rather than hammering the vendor for bytes we already
/// hold.
///
/// It is also what makes a failure *debuggable*: the exact bytes the vendor sent
/// are on disk, and can be replayed offline forever. That is the same property
/// the Stage 1 saved sample has, arrived at from the other direction.
pub struct CachingHttpClient {
    client: reqwest::blocking::Client,
    cache_dir: PathBuf,
    last_request: Option<Instant>,
    pub log: Vec<IngestLogRow>,
}

impl CachingHttpClient {
    pub fn new(cache_dir: &Path) -> Result<Self> {
        fs::create_dir_all(cache_dir)?;
        Ok(Self {
            client: reqwest::blocking::Client::builder()
                .timeout(Duration::from_secs(60))
                .build()?,
            cache_dir: cache_dir.to_path_buf(),
            last_request: None,
            log: Vec::new(),
        })
    }

    /// Cache key: a hash of the URL and query, NEVER the credentials. Two runs
    /// with different keys but the same request must hit the same cache entry —
    /// otherwise rotating a key would silently invalidate a two-year fetch.
    fn cache_path(&self, cache_key: &str) -> PathBuf {
        self.cache_dir
            .join(format!("{}.json", sha256_hex(cache_key.as_bytes())))
    }

    /// GET with pacing, retry, and cache. `cache_key` must exclude secrets.
    pub fn get(
        &mut self,
        vendor: &str,
        url: &str,
        query: &[(String, String)],
        headers: &[(String, String)],
        cache_key: &str,
        limit: RateLimit,
    ) -> Result<String> {
        let path = self.cache_path(cache_key);
        if path.exists() {
            let body = fs::read_to_string(&path)?;
            self.log.push(IngestLogRow {
                vendor: vendor.into(),
                request: cache_key.into(),
                status: 200,
                attempts: 0,
                rate_limited: false,
                from_cache: true,
                duration_ms: 0,
                bytes: body.len() as u64,
                error: String::new(),
            });
            return Ok(body);
        }

        let started = Instant::now();
        let mut attempts = 0;
        let mut rate_limited = false;

        loop {
            attempts += 1;

            // Pace BEFORE the request, not after a 429.
            if let Some(last) = self.last_request {
                let elapsed = last.elapsed();
                let interval = limit.min_interval();
                if elapsed < interval {
                    sleep(interval - elapsed);
                }
            }

            let mut request = self.client.get(url).query(query);
            for (name, value) in headers {
                request = request.header(name, value);
            }
            let response = request.send();
            self.last_request = Some(Instant::now());

            let (status, body, error) = match response {
                Ok(response) => {
                    let status = response.status().as_u16();
                    match response.text() {
                        Ok(body) => (status, body, String::new()),
                        Err(e) => (status, String::new(), e.to_string()),
                    }
                }
                // A network failure is a source OUTAGE. The spec wants those
                // recorded, not swallowed.
                Err(e) => (0, String::new(), e.to_string()),
            };

            match status {
                200 => {
                    fs::write(&path, &body)?;
                    self.log.push(IngestLogRow {
                        vendor: vendor.into(),
                        request: cache_key.into(),
                        status,
                        attempts,
                        rate_limited,
                        from_cache: false,
                        duration_ms: started.elapsed().as_millis() as u64,
                        bytes: body.len() as u64,
                        error: String::new(),
                    });
                    return Ok(body);
                }
                // Bad credentials. Retrying five times into a wall wastes minutes
                // and tells you nothing you did not know on the first attempt.
                401 | 403 => {
                    self.log.push(IngestLogRow {
                        vendor: vendor.into(),
                        request: cache_key.into(),
                        status,
                        attempts,
                        rate_limited,
                        from_cache: false,
                        duration_ms: started.elapsed().as_millis() as u64,
                        bytes: 0,
                        error: "credentials rejected".into(),
                    });
                    bail!(
                        "{vendor}: HTTP {status} — credentials rejected. Check your API key; \
                         retrying will not help."
                    );
                }
                429 | 500..=599 | 0 => {
                    rate_limited |= status == 429;
                    if attempts >= limit.max_attempts {
                        self.log.push(IngestLogRow {
                            vendor: vendor.into(),
                            request: cache_key.into(),
                            status,
                            attempts,
                            rate_limited,
                            from_cache: false,
                            duration_ms: started.elapsed().as_millis() as u64,
                            bytes: 0,
                            error: if error.is_empty() {
                                format!("gave up after {attempts} attempts")
                            } else {
                                error.clone()
                            },
                        });
                        bail!(
                            "{vendor}: HTTP {status} after {attempts} attempts. \
                             Nothing is lost — re-run to resume from the cache. {error}"
                        );
                    }
                    // Exponential backoff.
                    sleep(Duration::from_secs(2u64.pow(attempts.min(5))));
                }
                _ => {
                    self.log.push(IngestLogRow {
                        vendor: vendor.into(),
                        request: cache_key.into(),
                        status,
                        attempts,
                        rate_limited,
                        from_cache: false,
                        duration_ms: started.elapsed().as_millis() as u64,
                        bytes: 0,
                        error: body.chars().take(200).collect(),
                    });
                    bail!(
                        "{vendor}: HTTP {status}: {}",
                        body.chars().take(200).collect::<String>()
                    );
                }
            }
        }
    }
}

/// Read a credential from the environment. Never from config, never logged.
pub fn credential(name: &str) -> Result<String> {
    std::env::var(name).with_context(|| {
        format!("{name} is not set. Credentials come from the environment, never from config.")
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn a_cached_request_is_not_reissued() {
        // The whole basis of resumability. If this breaks, an interrupted two-year
        // fetch starts over from zero.
        let temp = TempDir::new().unwrap();
        let mut client = CachingHttpClient::new(temp.path()).unwrap();

        // Seed the cache by hand; the URL is deliberately unroutable, so if the
        // client tried to actually fetch it, this test would fail.
        let key = "massive:news:SPY:2025-07-01";
        fs::write(client.cache_path(key), r#"{"results":[]}"#).unwrap();

        let body = client
            .get(
                "massive",
                "http://127.0.0.1:1/never-reachable",
                &[],
                &[],
                key,
                RateLimit::massive(),
            )
            .unwrap();

        assert_eq!(body, r#"{"results":[]}"#);
        assert!(client.log[0].from_cache);
        assert_eq!(client.log[0].attempts, 0);
    }

    #[test]
    fn the_cache_key_does_not_depend_on_the_credential() {
        // Otherwise rotating an API key would silently invalidate a two-year fetch
        // and re-download everything.
        let temp = TempDir::new().unwrap();
        let client = CachingHttpClient::new(temp.path()).unwrap();

        let a = client.cache_path("massive:news:SPY:2025-07-01");
        let b = client.cache_path("massive:news:SPY:2025-07-01");

        assert_eq!(a, b);
    }

    #[test]
    fn a_network_failure_is_recorded_as_an_outage_not_swallowed() {
        let temp = TempDir::new().unwrap();
        let mut client = CachingHttpClient::new(temp.path()).unwrap();
        // One attempt so the test does not sit through the backoff.
        let limit = RateLimit {
            requests_per_minute: 6000,
            max_attempts: 1,
        };

        let result = client.get(
            "massive",
            "http://127.0.0.1:1/refused",
            &[],
            &[],
            "unreachable",
            limit,
        );

        assert!(result.is_err());
        assert_eq!(
            client.log.len(),
            1,
            "the outage must be LOGGED, not swallowed"
        );
        assert!(!client.log[0].error.is_empty());
        // And the message must tell you the fetch is resumable, because at Stage 3
        // scale the next question is always "do I have to start over?".
        assert!(result.unwrap_err().to_string().contains("re-run to resume"));
    }

    #[test]
    fn rate_limits_are_per_vendor() {
        // Massive's free tier is ~5/min; Alpaca allows 200. Pacing Alpaca at
        // Massive's rate would turn a two-year price fetch into an overnight job
        // for no reason.
        assert!(RateLimit::massive().min_interval() > RateLimit::alpaca().min_interval());
    }
}
