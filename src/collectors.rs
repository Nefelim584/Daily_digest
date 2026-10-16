//! Source collectors: the `Collector` trait, a factory, and adapters for
//! arXiv, Hugging Face Daily Papers, RSS, and RSSHub (Phase 26).
//!
//! Ported from the Python `collectors` package. `feed-rs` replaces
//! `feedparser` and already parses entry dates into `DateTime<Utc>`.
//! Web-search fallback on failure is deferred to Phase 32 (built-in scraper).

use async_trait::async_trait;
use chrono::{DateTime, Utc};

use crate::cleaning::sanitize_and_truncate;
use crate::config::{AppConfig, CleaningConfig, SourceConfig};
use crate::filters::{filter_by_time_window, parse_datetime_str};
use crate::models::NewsItem;
use crate::network::HttpFetcher;

/// Common interface for all source collectors.
#[async_trait]
pub trait Collector: Send + Sync {
    /// Human-readable source name.
    fn name(&self) -> &str;

    /// Fetch, sanitize, and return items published within the last `hours`.
    async fn collect(&self, hours: i64, now: Option<DateTime<Utc>>) -> Vec<NewsItem>;
}

/// Build a standardized `NewsItem` with a sanitized snippet.
fn build_item(
    source: &str,
    title: &str,
    url: &str,
    published_at: DateTime<Utc>,
    raw_content: &str,
    cleaning: &CleaningConfig,
) -> NewsItem {
    let clean = sanitize_and_truncate(Some(raw_content), cleaning);
    let mut item = NewsItem::new(
        title.replace('\n', " ").trim(),
        url.trim(),
        published_at,
        source,
    );
    item.raw_content = Some(raw_content.to_string());
    item.clean_snippet = Some(clean);
    item
}

/// Parse an RSS/Atom document into news items (shared by arXiv, RSS, RSSHub).
fn feed_to_items(text: &str, source: &str, cleaning: &CleaningConfig) -> Vec<NewsItem> {
    let feed = match feed_rs::parser::parse(text.as_bytes()) {
        Ok(feed) => feed,
        Err(err) => {
            tracing::warn!("Failed to parse feed for '{}': {}", source, err);
            return Vec::new();
        }
    };

    let mut items = Vec::new();
    for entry in feed.entries {
        let title = entry
            .title
            .map(|t| t.content)
            .unwrap_or_default()
            .replace('\n', " ")
            .trim()
            .to_string();

        let link = entry
            .links
            .first()
            .map(|l| l.href.clone())
            .filter(|h| !h.trim().is_empty())
            .unwrap_or_else(|| entry.id.clone());

        let published = entry.published.or(entry.updated);

        if title.is_empty() || link.trim().is_empty() || published.is_none() {
            continue;
        }

        let raw = entry
            .content
            .and_then(|c| c.body)
            .or_else(|| entry.summary.map(|t| t.content))
            .unwrap_or_default();

        items.push(build_item(
            source,
            &title,
            &link,
            published.expect("published checked above"),
            &raw,
            cleaning,
        ));
    }

    items
}

// ----------------------------------------------------------------------------
// arXiv
// ----------------------------------------------------------------------------

pub struct ArxivCollector {
    name: String,
    categories: Vec<String>,
    max_results: usize,
    endpoint: String,
    fetcher: HttpFetcher,
    cleaning: CleaningConfig,
}

impl ArxivCollector {
    pub const DEFAULT_ENDPOINT: &'static str = "https://export.arxiv.org/api/query";

    fn build_search_query(&self) -> String {
        if self.categories.is_empty() {
            return "cat:cs.AI".to_string();
        }
        self.categories
            .iter()
            .map(|c| format!("cat:{}", c.trim()))
            .collect::<Vec<_>>()
            .join(" OR ")
    }
}

#[async_trait]
impl Collector for ArxivCollector {
    fn name(&self) -> &str {
        &self.name
    }

    async fn collect(&self, hours: i64, now: Option<DateTime<Utc>>) -> Vec<NewsItem> {
        let query = self.build_search_query();
        let max = self.max_results.to_string();
        let params = [
            ("search_query", query.as_str()),
            ("sortBy", "submittedDate"),
            ("sortOrder", "descending"),
            ("max_results", max.as_str()),
        ];

        tracing::info!("Fetching arXiv papers with query: {}", query);
        let text = match self.fetcher.fetch_text(&self.endpoint, &params).await {
            Ok(text) => text,
            Err(err) => {
                tracing::error!("Failed to fetch arXiv papers: {}", err);
                return Vec::new();
            }
        };

        let items = feed_to_items(&text, &self.name, &self.cleaning);
        filter_by_time_window(items, hours, now, true)
    }
}

// ----------------------------------------------------------------------------
// Hugging Face Daily Papers
// ----------------------------------------------------------------------------

pub struct HuggingFacePapersCollector {
    name: String,
    endpoint: String,
    fetcher: HttpFetcher,
    cleaning: CleaningConfig,
}

impl HuggingFacePapersCollector {
    pub const DEFAULT_ENDPOINT: &'static str = "https://huggingface.co/api/daily_papers";
}

#[async_trait]
impl Collector for HuggingFacePapersCollector {
    fn name(&self) -> &str {
        &self.name
    }

    async fn collect(&self, hours: i64, now: Option<DateTime<Utc>>) -> Vec<NewsItem> {
        tracing::info!("Fetching Hugging Face Daily Papers from {}", self.endpoint);
        let data: serde_json::Value = match self.fetcher.fetch_json(&self.endpoint, &[]).await {
            Ok(data) => data,
            Err(err) => {
                tracing::error!("Failed to fetch Hugging Face Daily Papers: {}", err);
                return Vec::new();
            }
        };

        let Some(records) = data.as_array() else {
            tracing::warn!("Unexpected Hugging Face response structure: expected list");
            return Vec::new();
        };

        let mut items = Vec::new();
        for record in records {
            let paper = record.get("paper");

            let title = paper
                .and_then(|p| p.get("title"))
                .and_then(|v| v.as_str())
                .or_else(|| record.get("title").and_then(|v| v.as_str()))
                .unwrap_or("")
                .replace('\n', " ")
                .trim()
                .to_string();

            let id = paper
                .and_then(|p| p.get("id"))
                .and_then(|v| v.as_str())
                .or_else(|| record.get("id").and_then(|v| v.as_str()));
            let url = match id {
                Some(id) => format!("https://huggingface.co/papers/{}", id),
                None => "https://huggingface.co/papers".to_string(),
            };

            let date_str = paper
                .and_then(|p| p.get("submittedOnDailyAt"))
                .and_then(|v| v.as_str())
                .or_else(|| record.get("submittedOnDailyAt").and_then(|v| v.as_str()))
                .or_else(|| record.get("publishedAt").and_then(|v| v.as_str()))
                .or_else(|| {
                    paper
                        .and_then(|p| p.get("publishedAt"))
                        .and_then(|v| v.as_str())
                });
            let published = date_str.and_then(parse_datetime_str);

            if title.is_empty() || published.is_none() {
                continue;
            }

            let summary = paper
                .and_then(|p| p.get("summary"))
                .and_then(|v| v.as_str())
                .or_else(|| record.get("summary").and_then(|v| v.as_str()))
                .unwrap_or("");

            items.push(build_item(
                &self.name,
                &title,
                &url,
                published.expect("published checked above"),
                summary,
                &self.cleaning,
            ));
        }

        filter_by_time_window(items, hours, now, true)
    }
}

// ----------------------------------------------------------------------------
// RSS / Atom
// ----------------------------------------------------------------------------

pub struct RssCollector {
    name: String,
    url: String,
    fetcher: HttpFetcher,
    cleaning: CleaningConfig,
}

#[async_trait]
impl Collector for RssCollector {
    fn name(&self) -> &str {
        &self.name
    }

    async fn collect(&self, hours: i64, now: Option<DateTime<Utc>>) -> Vec<NewsItem> {
        tracing::info!("Fetching RSS feed for '{}' from {}", self.name, self.url);
        let text = match self.fetcher.fetch_text(&self.url, &[]).await {
            Ok(text) => text,
            Err(err) => {
                tracing::warn!("Could not fetch RSS feed for '{}': {}", self.name, err);
                // TODO(Phase 32): trigger built-in scraper fallback here.
                return Vec::new();
            }
        };

        let items = feed_to_items(&text, &self.name, &self.cleaning);
        filter_by_time_window(items, hours, now, true)
    }
}

// ----------------------------------------------------------------------------
// RSSHub
// ----------------------------------------------------------------------------

pub struct RsshubCollector {
    name: String,
    routes: Vec<String>,
    base_url: String,
    access_key: String,
    fetcher: HttpFetcher,
    cleaning: CleaningConfig,
}

impl RsshubCollector {
    async fn collect_route(&self, route: &str) -> Vec<NewsItem> {
        let url = format!("{}/{}", self.base_url, route.trim_start_matches('/'));
        let is_twitter = route.contains("/twitter/user/");
        let route_label = route.trim_matches('/').rsplit('/').next().unwrap_or("");
        let source_label = if is_twitter {
            format!("{} (@{})", self.name, route_label)
        } else {
            self.name.clone()
        };

        tracing::info!("Fetching RSSHub route {}", url);
        let result = if self.access_key.is_empty() {
            self.fetcher.fetch_text(&url, &[]).await
        } else {
            self.fetcher
                .fetch_text(&url, &[("key", self.access_key.as_str())])
                .await
        };

        match result {
            Ok(text) => feed_to_items(&text, &source_label, &self.cleaning),
            Err(err) => {
                tracing::warn!("Could not fetch RSSHub route {}: {}", url, err);
                // TODO(Phase 32): trigger built-in scraper fallback here.
                Vec::new()
            }
        }
    }
}

#[async_trait]
impl Collector for RsshubCollector {
    fn name(&self) -> &str {
        &self.name
    }

    async fn collect(&self, hours: i64, now: Option<DateTime<Utc>>) -> Vec<NewsItem> {
        let mut all_items = Vec::new();
        for route in &self.routes {
            all_items.extend(self.collect_route(route).await);
        }
        filter_by_time_window(all_items, hours, now, true)
    }
}

// ----------------------------------------------------------------------------
// Factory
// ----------------------------------------------------------------------------

/// Create a shared fetcher with the given timeout, logging on failure.
fn make_fetcher(timeout_seconds: u64) -> Option<HttpFetcher> {
    match HttpFetcher::new(timeout_seconds, 3, 1.5, None, 0) {
        Ok(fetcher) => Some(fetcher),
        Err(err) => {
            tracing::error!("Failed to build HTTP fetcher: {}", err);
            None
        }
    }
}

/// Build a single collector from a source configuration.
fn create_collector(source: &SourceConfig, config: &AppConfig) -> Option<Box<dyn Collector>> {
    if !source.enabled {
        return None;
    }

    let cleaning = config.cleaning.clone();
    let collector_type = source.source_type.trim().to_lowercase();

    match collector_type.as_str() {
        "arxiv_api" => {
            let fetcher = make_fetcher(20)?;
            Some(Box::new(ArxivCollector {
                name: source.name.clone(),
                categories: source.categories.clone(),
                max_results: source.max_results,
                endpoint: source
                    .endpoint
                    .clone()
                    .unwrap_or_else(|| ArxivCollector::DEFAULT_ENDPOINT.to_string()),
                fetcher,
                cleaning,
            }))
        }

        "huggingface_papers_api" | "huggingface_daily_papers" => {
            let fetcher = make_fetcher(20)?;
            Some(Box::new(HuggingFacePapersCollector {
                name: source.name.clone(),
                endpoint: source
                    .endpoint
                    .clone()
                    .unwrap_or_else(|| HuggingFacePapersCollector::DEFAULT_ENDPOINT.to_string()),
                fetcher,
                cleaning,
            }))
        }

        "rss" => {
            let url = match &source.url {
                Some(url) if !url.trim().is_empty() => url.clone(),
                _ => {
                    tracing::warn!(
                        "Source '{}' has type 'rss' but no URL specified; skipping.",
                        source.name
                    );
                    return None;
                }
            };
            let fetcher = make_fetcher(20)?;
            Some(Box::new(RssCollector {
                name: source.name.clone(),
                url,
                fetcher,
                cleaning,
            }))
        }

        "rsshub" => {
            let fetcher = make_fetcher(config.rsshub.timeout_seconds)?;
            Some(Box::new(RsshubCollector {
                name: source.name.clone(),
                routes: source.routes.clone(),
                base_url: config.rsshub.base_url.trim_end_matches('/').to_string(),
                access_key: config.rsshub.access_key.clone(),
                fetcher,
                cleaning,
            }))
        }

        other => {
            tracing::warn!(
                "Unknown collector type '{}' for source '{}'",
                other,
                source.name
            );
            None
        }
    }
}

/// Instantiate all enabled collectors defined in the application configuration.
pub fn create_collectors(config: &AppConfig) -> Vec<Box<dyn Collector>> {
    let mut collectors: Vec<Box<dyn Collector>> = Vec::new();
    for source in &config.sources {
        if let Some(collector) = create_collector(source, config) {
            collectors.push(collector);
        }
    }
    collectors
}
