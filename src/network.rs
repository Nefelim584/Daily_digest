//! Resilient async HTTP fetcher: timeouts, retry/backoff, 429 handling,
//! and an optional circuit breaker (Phase 25).
//!
//! Ported from the Python `HTTPFetcher`. Retries transient errors
//! (429/500/502/503/504 and network/timeout failures) with exponential
//! backoff, honoring a numeric `Retry-After` header on 429 responses.

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

use reqwest::{header, Client, Response, StatusCode};
use serde::de::DeserializeOwned;
use thiserror::Error;

const DEFAULT_USER_AGENT: &str =
    "Mozilla/5.0 (compatible; DailyDigest/0.1.0; +https://github.com/daily-digest)";

/// Errors raised when an HTTP fetch operation fails.
#[derive(Debug, Error)]
pub enum FetchError {
    #[error("HTTP request to {url} failed with status {status}: {body}")]
    Status {
        url: String,
        status: u16,
        body: String,
    },

    #[error("failed to fetch {url} after {attempts} attempts: {source}")]
    Transport {
        url: String,
        attempts: usize,
        #[source]
        source: reqwest::Error,
    },

    #[error("failed to fetch {url} after {attempts} attempts (no response)")]
    Exhausted { url: String, attempts: usize },

    #[error("circuit breaker open; skipping request to {url}")]
    CircuitOpen { url: String },

    #[error("failed to parse JSON from {url}: {source}")]
    Json {
        url: String,
        #[source]
        source: reqwest::Error,
    },

    #[error("failed to build HTTP client: {0}")]
    Build(#[source] reqwest::Error),
}

impl FetchError {
    /// The HTTP status code associated with this error, if any.
    pub fn status_code(&self) -> Option<u16> {
        match self {
            FetchError::Status { status, .. } => Some(*status),
            _ => None,
        }
    }
}

/// HTTP client wrapper with retry logic, exponential backoff, and rate-limit handling.
#[derive(Clone)]
pub struct HttpFetcher {
    client: Client,
    max_retries: usize,
    backoff_factor: f64,
    /// Consecutive failures threshold before the circuit opens (0 = disabled).
    circuit_breaker_threshold: usize,
    consecutive_failures: Arc<AtomicUsize>,
    circuit_open: Arc<AtomicBool>,
}

impl HttpFetcher {
    /// Create a fetcher with explicit settings.
    ///
    /// `circuit_breaker_threshold` of 0 disables the circuit breaker (default
    /// behaviour, matching the Python fetcher which had none).
    pub fn new(
        timeout_seconds: u64,
        max_retries: usize,
        backoff_factor: f64,
        user_agent: Option<&str>,
        circuit_breaker_threshold: usize,
    ) -> Result<Self, FetchError> {
        let mut headers = header::HeaderMap::new();
        headers.insert(
            header::USER_AGENT,
            header::HeaderValue::from_str(user_agent.unwrap_or(DEFAULT_USER_AGENT))
                .unwrap_or_else(|_| header::HeaderValue::from_static(DEFAULT_USER_AGENT)),
        );
        headers.insert(header::ACCEPT, header::HeaderValue::from_static("*/*"));

        let client = Client::builder()
            .timeout(Duration::from_secs(timeout_seconds))
            .default_headers(headers)
            .build()
            .map_err(FetchError::Build)?;

        Ok(Self {
            client,
            max_retries,
            backoff_factor,
            circuit_breaker_threshold,
            consecutive_failures: Arc::new(AtomicUsize::new(0)),
            circuit_open: Arc::new(AtomicBool::new(false)),
        })
    }

    /// Create a fetcher with sensible defaults (20s timeout, 3 retries, no circuit breaker).
    pub fn with_defaults() -> Result<Self, FetchError> {
        Self::new(20, 3, 1.5, None, 0)
    }

    /// Whether an HTTP status code indicates a transient, retryable error.
    pub fn should_retry(status: StatusCode) -> bool {
        matches!(status.as_u16(), 429 | 500 | 502 | 503 | 504)
    }

    /// Wait time before a retry, respecting a numeric `Retry-After` header when present.
    fn sleep_duration(&self, attempt: usize, retry_after: Option<f64>) -> Duration {
        if let Some(ra) = retry_after {
            return Duration::from_secs_f64(ra.max(1.0));
        }
        let secs = (self.backoff_factor * 2f64.powi(attempt as i32)).max(0.0);
        Duration::from_secs_f64(secs)
    }

    fn record_success(&self) {
        self.consecutive_failures.store(0, Ordering::Relaxed);
        self.circuit_open.store(false, Ordering::Relaxed);
    }

    fn record_failure(&self) {
        let failures = self.consecutive_failures.fetch_add(1, Ordering::Relaxed) + 1;
        if self.circuit_breaker_threshold > 0 && failures >= self.circuit_breaker_threshold {
            self.circuit_open.store(true, Ordering::Relaxed);
            tracing::warn!(
                "HTTP fetcher circuit breaker opened after {} consecutive failures",
                failures
            );
        }
    }

    /// Fetch an HTTP response with automatic retry and rate-limit handling.
    pub async fn fetch_response(
        &self,
        url: &str,
        query: &[(&str, &str)],
    ) -> Result<Response, FetchError> {
        if self.circuit_open.load(Ordering::Relaxed) {
            return Err(FetchError::CircuitOpen {
                url: url.to_string(),
            });
        }

        let mut last_transport: Option<reqwest::Error> = None;

        for attempt in 0..=self.max_retries {
            let request = self.client.get(url).query(query);

            match request.send().await {
                Ok(response) => {
                    let status = response.status();

                    if status.is_success() {
                        self.record_success();
                        return Ok(response);
                    }

                    if Self::should_retry(status) && attempt < self.max_retries {
                        let retry_after = parse_retry_after(&response);
                        let delay = self.sleep_duration(attempt, retry_after);
                        tracing::warn!(
                            "HTTP {} fetching {} (attempt {}/{}). Retrying in {:.1}s...",
                            status.as_u16(),
                            url,
                            attempt + 1,
                            self.max_retries,
                            delay.as_secs_f64(),
                        );
                        tokio::time::sleep(delay).await;
                        continue;
                    }

                    let status_code = status.as_u16();
                    let body = response.text().await.unwrap_or_default();
                    let body: String = body.chars().take(200).collect();
                    self.record_failure();
                    return Err(FetchError::Status {
                        url: url.to_string(),
                        status: status_code,
                        body,
                    });
                }
                Err(err) => {
                    if attempt < self.max_retries {
                        let delay = self.sleep_duration(attempt, None);
                        tracing::warn!(
                            "Network/timeout error fetching {} (attempt {}/{}): {}. Retrying in {:.1}s...",
                            url,
                            attempt + 1,
                            self.max_retries,
                            err,
                            delay.as_secs_f64(),
                        );
                        last_transport = Some(err);
                        tokio::time::sleep(delay).await;
                        continue;
                    }
                    last_transport = Some(err);
                    break;
                }
            }
        }

        self.record_failure();
        let attempts = self.max_retries + 1;
        match last_transport {
            Some(source) => Err(FetchError::Transport {
                url: url.to_string(),
                attempts,
                source,
            }),
            None => Err(FetchError::Exhausted {
                url: url.to_string(),
                attempts,
            }),
        }
    }

    /// Fetch raw response text from the given URL.
    pub async fn fetch_text(&self, url: &str, query: &[(&str, &str)]) -> Result<String, FetchError> {
        let response = self.fetch_response(url, query).await?;
        response.text().await.map_err(|source| FetchError::Transport {
            url: url.to_string(),
            attempts: 1,
            source,
        })
    }

    /// Fetch and deserialize JSON from the given URL.
    pub async fn fetch_json<T: DeserializeOwned>(
        &self,
        url: &str,
        query: &[(&str, &str)],
    ) -> Result<T, FetchError> {
        let response = self.fetch_response(url, query).await?;
        response.json::<T>().await.map_err(|source| FetchError::Json {
            url: url.to_string(),
            source,
        })
    }
}

/// Parse a numeric `Retry-After` header into seconds, if present and valid.
fn parse_retry_after(response: &Response) -> Option<f64> {
    response
        .headers()
        .get(header::RETRY_AFTER)
        .and_then(|value| value.to_str().ok())
        .and_then(|raw| raw.trim().parse::<f64>().ok())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retryable_statuses_are_detected() {
        for code in [429u16, 500, 502, 503, 504] {
            assert!(HttpFetcher::should_retry(
                StatusCode::from_u16(code).unwrap()
            ));
        }
        for code in [200u16, 301, 400, 401, 404] {
            assert!(!HttpFetcher::should_retry(
                StatusCode::from_u16(code).unwrap()
            ));
        }
    }

    #[test]
    fn backoff_grows_exponentially() {
        let fetcher = HttpFetcher::new(20, 3, 1.5, None, 0).unwrap();
        let d0 = fetcher.sleep_duration(0, None).as_secs_f64();
        let d1 = fetcher.sleep_duration(1, None).as_secs_f64();
        let d2 = fetcher.sleep_duration(2, None).as_secs_f64();
        assert!((d0 - 1.5).abs() < 1e-9);
        assert!((d1 - 3.0).abs() < 1e-9);
        assert!((d2 - 6.0).abs() < 1e-9);
    }

    #[test]
    fn retry_after_header_takes_precedence() {
        let fetcher = HttpFetcher::new(20, 3, 1.5, None, 0).unwrap();
        let d = fetcher.sleep_duration(0, Some(10.0)).as_secs_f64();
        assert!((d - 10.0).abs() < 1e-9);
        // Floor of 1.0 is enforced.
        let floored = fetcher.sleep_duration(0, Some(0.2)).as_secs_f64();
        assert!((floored - 1.0).abs() < 1e-9);
    }
}
