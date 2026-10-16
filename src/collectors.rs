//! Source collectors: the `Collector` trait, a factory, and adapters for
//! arXiv, Hugging Face Daily Papers, RSS, and RSSHub (Phase 26).
//!
//! Ported from the Python `collectors` package. `feed-rs` replaces
//! `feedparser` and already parses entry dates into `DateTime<Utc>`.
//! Web-search fallback on failure uses the built-in scraper (Phase 32).

use std::sync::Arc;

use async_trait::async_trait;
use chrono::{DateTime, Utc};

use crate::cleaning::sanitize_and_truncate;
use crate::config::{AppConfig, CleaningConfig, SourceConfig};
use crate::filters::{filter_by_time_window, parse_datetime_str};
use crate::models::NewsItem;
use crate::network::HttpFetcher;
use crate::search::WebScraperFallback;

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
    fallback: Option<Arc<WebScraperFallback>>,
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
                if let Some(fallback) = &self.fallback {
                    tracing::info!("Triggering web scraper fallback for '{}'", self.name);
                    return fallback.scrape_blog_fallback(&self.name, &self.url, hours, now).await;
                }
                return Vec::new();
            }
        };

        let items = feed_to_items(&text, &self.name, &self.cleaning);
        if items.is_empty() {
            if let Some(fallback) = &self.fallback {
                tracing::info!(
                    "Feed for '{}' produced 0 items; attempting web scraper fallback",
                    self.name
                );
                let fallback_items = fallback
                    .scrape_blog_fallback(&self.name, &self.url, hours, now)
                    .await;
                if !fallback_items.is_empty() {
                    return fallback_items;
                }
            }
        }
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
    fallback: Option<Arc<WebScraperFallback>>,
}

impl RsshubCollector {
    async fn trigger_fallback(
        &self,
        fallback: &WebScraperFallback,
        route: &str,
        source_label: &str,
        url: &str,
        hours: i64,
        now: Option<DateTime<Utc>>,
    ) -> Vec<NewsItem> {
        let route_clean = route.trim_start_matches('/');
        let is_twitter = route.contains("/twitter/user/") || route_clean.starts_with("twitter/user/");
        let route_label = route.trim_matches('/').rsplit('/').next().unwrap_or("");

        if is_twitter {
            tracing::info!("Triggering Twitter web scraper fallback for @{}", route_label);
            fallback
                .scrape_twitter_fallback(route_label, source_label, hours, now)
                .await
        } else {
            tracing::info!("Triggering web scraper fallback for route {}", route);
            fallback
                .scrape_blog_fallback(source_label, url, hours, now)
                .await
        }
    }

    async fn collect_route(
        &self,
        route: &str,
        hours: i64,
        now: Option<DateTime<Utc>>,
    ) -> Vec<NewsItem> {
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

        let text = match result {
            Ok(text) => text,
            Err(err) => {
                tracing::warn!("Could not fetch RSSHub route {}: {}", url, err);
                if let Some(fallback) = &self.fallback {
                    return self
                        .trigger_fallback(fallback, route, &source_label, &url, hours, now)
                        .await;
                }
                return Vec::new();
            }
        };

        let items = feed_to_items(&text, &source_label, &self.cleaning);
        if items.is_empty() {
            if let Some(fallback) = &self.fallback {
                let fallback_items = self
                    .trigger_fallback(fallback, route, &source_label, &url, hours, now)
                    .await;
                if !fallback_items.is_empty() {
                    return fallback_items;
                }
            }
        }
        items
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
            all_items.extend(self.collect_route(route, hours, now).await);
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
fn create_collector(
    source: &SourceConfig,
    config: &AppConfig,
    fallback: Option<Arc<WebScraperFallback>>,
) -> Option<Box<dyn Collector>> {
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
                fallback,
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
                fallback,
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
    let fallback = if config.search.enabled {
        Some(Arc::new(WebScraperFallback::new(
            &config.search,
            &config.cleaning,
        )))
    } else {
        None
    };

    let mut collectors: Vec<Box<dyn Collector>> = Vec::new();
    for source in &config.sources {
        if let Some(collector) = create_collector(source, config, fallback.clone()) {
            collectors.push(collector);
        }
    }
    collectors
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_item_sanitizes_and_truncates() {
        let cleaning = CleaningConfig {
            max_characters: 50,
            strip_html_tags: true,
            strip_images: true,
            strip_links: true,
            strip_scripts: true,
        };
        let raw = "<p>This is a <b>long paragraph</b> that needs to be cleaned and truncated properly.</p>";
        let item = build_item(
            "Test Source",
            "Headline",
            "https://example.com/item",
            Utc::now(),
            raw,
            &cleaning,
        );

        assert_eq!(item.title, "Headline");
        assert_eq!(item.url, "https://example.com/item");
        assert_eq!(item.source, "Test Source");
        let snippet = item.clean_snippet.unwrap();
        assert!(!snippet.contains("<p>"));
        assert!(!snippet.contains("<b>"));
        assert!(snippet.len() <= 50);
        assert!(snippet.ends_with("..."));
    }

    #[test]
    fn feed_to_items_parses_rss_xml() {
        let rss_xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<rss version="2.0">
  <channel>
    <title>AI News</title>
    <link>https://example.com</link>
    <description>Latest AI News</description>
    <item>
      <title>GPT-5 Announcement</title>
      <link>https://example.com/gpt-5</link>
      <description><![CDATA[<p>We are excited to announce our newest model.</p>]]></description>
      <pubDate>Wed, 15 Oct 2026 12:00:00 GMT</pubDate>
    </item>
  </channel>
</rss>"#;

        let cleaning = CleaningConfig::default();
        let items = feed_to_items(rss_xml, "OpenAI Blog", &cleaning);
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].title, "GPT-5 Announcement");
        assert_eq!(items[0].url, "https://example.com/gpt-5");
        assert_eq!(items[0].source, "OpenAI Blog");
        assert!(items[0].clean_snippet.as_ref().unwrap().contains("We are excited"));
    }

    #[test]
    fn arxiv_query_builder_formats_categories() {
        let fetcher = HttpFetcher::new(20, 1, 1.0, None, 0).unwrap();
        let collector = ArxivCollector {
            name: "arXiv".to_string(),
            categories: vec!["cs.AI".to_string(), "cs.LG".to_string()],
            max_results: 10,
            endpoint: ArxivCollector::DEFAULT_ENDPOINT.to_string(),
            fetcher,
            cleaning: CleaningConfig::default(),
        };

        assert_eq!(collector.build_search_query(), "cat:cs.AI OR cat:cs.LG");

        let default_collector = ArxivCollector {
            name: "arXiv Default".to_string(),
            categories: Vec::new(),
            max_results: 10,
            endpoint: ArxivCollector::DEFAULT_ENDPOINT.to_string(),
            fetcher: HttpFetcher::new(20, 1, 1.0, None, 0).unwrap(),
            cleaning: CleaningConfig::default(),
        };
        assert_eq!(default_collector.build_search_query(), "cat:cs.AI");
    }

    #[test]
    fn create_collectors_respects_enabled_flag() {
        let mut config = AppConfig::default();
        config.sources = vec![
            SourceConfig {
                name: "Enabled RSS".to_string(),
                source_type: "rss".to_string(),
                enabled: true,
                url: Some("https://example.com/feed.xml".to_string()),
                endpoint: None,
                routes: Vec::new(),
                categories: Vec::new(),
                max_results: 10,
            },
            SourceConfig {
                name: "Disabled RSS".to_string(),
                source_type: "rss".to_string(),
                enabled: false,
                url: Some("https://example.com/disabled.xml".to_string()),
                endpoint: None,
                routes: Vec::new(),
                categories: Vec::new(),
                max_results: 10,
            },
        ];

        let collectors = create_collectors(&config);
        assert_eq!(collectors.len(), 1);
        assert_eq!(collectors[0].name(), "Enabled RSS");
    }
}
