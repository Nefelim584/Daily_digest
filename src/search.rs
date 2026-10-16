//! Built-in web scraper fallback (reqwest + scraper) for failed sources (Phase 32).
//!
//! Replaces the former Gemini/Mistral web-search grounding entirely.
//! When an RSS feed or RSSHub route fails (404, 403, 429, timeout, or empty/malformed XML),
//! this module provides direct HTML scraping fallbacks:
//! 1. Blog/RSS scraper: Derives or fetches HTML landing/blog pages and extracts
//!    recent article links, headlines, dates, and snippets.
//! 2. X/Twitter scraper: Scrapes public syndication timelines or mirrors for user
//!    profiles, preserving profile URL and embedded external links.

use std::collections::HashSet;
use std::time::Duration;

use chrono::{DateTime, Utc};
use reqwest::Url;
use scraper::{Html, Selector};

use crate::cleaning::sanitize_and_truncate;
use crate::config::{CleaningConfig, SearchFallbackConfig};
use crate::filters::{filter_by_time_window, parse_datetime_str};
use crate::models::NewsItem;
use crate::network::HttpFetcher;

/// Web scraper fallback for failed news sources.
#[derive(Clone)]
pub struct WebScraperFallback {
    fetcher: HttpFetcher,
    cleaning: CleaningConfig,
    enabled: bool,
    pacing_seconds: f64,
}

impl WebScraperFallback {
    /// Create a new scraper fallback client from configuration.
    pub fn new(config: &SearchFallbackConfig, cleaning: &CleaningConfig) -> Self {
        let fetcher = HttpFetcher::new(15, 2, 1.5, None, 0)
            .unwrap_or_else(|_| HttpFetcher::with_defaults().expect("Default HTTP fetcher"));
        Self {
            fetcher,
            cleaning: cleaning.clone(),
            enabled: config.enabled,
            pacing_seconds: config.pacing_seconds,
        }
    }

    /// Construct with a specific HTTP fetcher (useful for testing or custom timeouts).
    pub fn with_fetcher(
        fetcher: HttpFetcher,
        config: &SearchFallbackConfig,
        cleaning: &CleaningConfig,
    ) -> Self {
        Self {
            fetcher,
            cleaning: cleaning.clone(),
            enabled: config.enabled,
            pacing_seconds: config.pacing_seconds,
        }
    }

    /// Whether the scraper fallback is enabled.
    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    /// Derive possible HTML landing page URLs from an RSS feed URL.
    pub fn derive_html_urls(feed_url: &str) -> Vec<String> {
        let mut urls = Vec::new();
        let trimmed = feed_url.trim();
        if trimmed.is_empty() {
            return urls;
        }

        // 1. If it has typical RSS suffixes (.xml, /feed, /feed/, /rss, /rss.xml), try stripping them.
        if let Ok(parsed) = Url::parse(trimmed) {
            let path = parsed.path();
            for suffix in &[
                "/rss.xml",
                "/feed.xml",
                "/index.xml",
                "/atom.xml",
                "/feed/",
                "/feed",
                "/rss/",
                "/rss",
            ] {
                if path.ends_with(suffix) {
                    let mut stripped_url = parsed.clone();
                    let new_path = path.strip_suffix(suffix).unwrap_or("");
                    let base_path = if new_path.is_empty() { "/" } else { new_path };

                    // Candidate with trailing slash (e.g. "/news/")
                    let slash_path = if base_path.ends_with('/') {
                        base_path.to_string()
                    } else {
                        format!("{}/", base_path)
                    };
                    stripped_url.set_path(&slash_path);
                    let s_slash = stripped_url.to_string();
                    if s_slash != trimmed && !urls.contains(&s_slash) {
                        urls.push(s_slash);
                    }

                    // Candidate without trailing slash (e.g. "/news")
                    if base_path != "/" {
                        let no_slash_path = base_path.trim_end_matches('/');
                        stripped_url.set_path(no_slash_path);
                        let s_no_slash = stripped_url.to_string();
                        if s_no_slash != trimmed && !urls.contains(&s_no_slash) {
                            urls.push(s_no_slash);
                        }
                    }
                }
            }
        }

        // 2. Add the original URL itself.
        if !urls.contains(&trimmed.to_string()) {
            urls.push(trimmed.to_string());
        }

        urls
    }

    /// Perform a direct web scrape fallback for an official blog or website source.
    pub async fn scrape_blog_fallback(
        &self,
        source_name: &str,
        source_url: &str,
        hours: i64,
        now: Option<DateTime<Utc>>,
    ) -> Vec<NewsItem> {
        if !self.enabled {
            return Vec::new();
        }

        if self.pacing_seconds > 0.0 {
            tokio::time::sleep(Duration::from_secs_f64(self.pacing_seconds)).await;
        }

        let candidates = Self::derive_html_urls(source_url);
        for target_url in candidates {
            tracing::info!(
                "Scraping blog fallback for '{}' at {}",
                source_name,
                target_url
            );

            match self.fetcher.fetch_text(&target_url, &[]).await {
                Ok(html) => {
                    let items = self.parse_blog_html(
                        &html,
                        &target_url,
                        source_name,
                        hours,
                        now,
                    );
                    if !items.is_empty() {
                        tracing::info!(
                            "Blog scraper fallback for '{}' retrieved {} items from {}",
                            source_name,
                            items.len(),
                            target_url
                        );
                        return items;
                    }
                }
                Err(err) => {
                    tracing::debug!(
                        "Could not fetch HTML candidate {} for '{}': {}",
                        target_url,
                        source_name,
                        err
                    );
                }
            }
        }

        tracing::warn!(
            "Web scraper fallback for '{}' found no articles from {}",
            source_name,
            source_url
        );
        Vec::new()
    }

    /// Parse blog HTML content into news items.
    pub fn parse_blog_html(
        &self,
        html: &str,
        base_url: &str,
        source_name: &str,
        hours: i64,
        now: Option<DateTime<Utc>>,
    ) -> Vec<NewsItem> {
        let doc = Html::parse_document(html);
        let parsed_base = match Url::parse(base_url) {
            Ok(u) => u,
            Err(_) => return Vec::new(),
        };

        let mut items = Vec::new();
        let mut seen_urls = HashSet::new();

        // 1. Try structured article/post selectors
        let container_selectors = [
            "article",
            "div.post",
            "div.article",
            "div.news-item",
            "div.blog-post",
            "li.post",
            "li.article",
            "div.card",
            "section.post",
        ];

        let title_sel = Selector::parse("h1, h2, h3, h4, [class*='title'], [class*='heading'], strong")
            .expect("Valid title selector");
        let link_sel = Selector::parse("a[href]").expect("Valid link selector");
        let time_sel = Selector::parse("time, [class*='date'], [class*='time'], [class*='published']")
            .expect("Valid time selector");
        let p_sel = Selector::parse("p, [class*='summary'], [class*='excerpt'], [class*='description']")
            .expect("Valid snippet selector");

        for sel_str in container_selectors {
            if let Ok(container_sel) = Selector::parse(sel_str) {
                for container in doc.select(&container_sel) {
                    // Extract link
                    let mut href_opt = None;
                    if container.value().name() == "a" {
                        href_opt = container.value().attr("href");
                    }
                    if href_opt.is_none() {
                        href_opt = container.select(&link_sel).next().and_then(|a| a.value().attr("href"));
                    }

                    let Some(href) = href_opt else { continue; };
                    let Ok(resolved_url) = parsed_base.join(href) else { continue; };
                    let full_url = resolved_url.to_string();

                    // URL validation
                    if !full_url.starts_with("http://") && !full_url.starts_with("https://") {
                        continue;
                    }
                    if full_url == base_url || (full_url.contains('#') && full_url.starts_with(base_url)) {
                        continue;
                    }
                    if seen_urls.contains(&full_url) {
                        continue;
                    }

                    // Extract title
                    let mut title = container
                        .select(&title_sel)
                        .next()
                        .map(|el| el.text().map(|s| s.trim()).filter(|s| !s.is_empty()).collect::<Vec<_>>().join(" "))
                        .unwrap_or_default();

                    if title.is_empty() {
                        // Fallback to link text
                        title = container
                            .select(&link_sel)
                            .next()
                            .map(|el| el.text().map(|s| s.trim()).filter(|s| !s.is_empty()).collect::<Vec<_>>().join(" "))
                            .unwrap_or_default();
                    }

                    let title = title.replace('\n', " ").trim().to_string();
                    if title.len() < 5
                        || title.eq_ignore_ascii_case("read more")
                        || title.eq_ignore_ascii_case("learn more")
                    {
                        continue;
                    }

                    // Extract date
                    let mut date_opt = None;
                    if let Some(time_el) = container.select(&time_sel).next() {
                        if let Some(dt_attr) = time_el.value().attr("datetime") {
                            date_opt = parse_datetime_str(dt_attr);
                        }
                        if date_opt.is_none() {
                            let text_date = time_el.text().collect::<String>();
                            date_opt = parse_datetime_str(&text_date);
                        }
                    }
                    let published_at = date_opt.unwrap_or_else(|| now.unwrap_or_else(Utc::now));

                    // Extract snippet
                    let mut raw_content = container
                        .select(&p_sel)
                        .map(|p| p.text().map(|s| s.trim()).filter(|s| !s.is_empty()).collect::<Vec<_>>().join(" "))
                        .filter(|s| !s.is_empty())
                        .collect::<Vec<_>>()
                        .join(" ");

                    if raw_content.is_empty() {
                        raw_content = container
                            .text()
                            .map(|s| s.trim())
                            .filter(|s| !s.is_empty())
                            .collect::<Vec<_>>()
                            .join(" ");
                    }

                    let clean = sanitize_and_truncate(Some(&raw_content), &self.cleaning);
                    seen_urls.insert(full_url.clone());

                    let mut item = NewsItem::new(
                        &title,
                        &full_url,
                        published_at,
                        format!("{} (Web Scraper)", source_name),
                    );
                    item.raw_content = Some(raw_content);
                    item.clean_snippet = Some(clean);
                    item.summary = None; // Summarization reserved for local TextRank
                    items.push(item);
                }
            }

            if !items.is_empty() {
                break;
            }
        }

        // 2. Fallback: if no structured articles found, scan for prominent content links in main / body
        if items.is_empty() {
            if let Ok(fallback_link_sel) = Selector::parse("main a[href], body a[href]") {
                for a in doc.select(&fallback_link_sel) {
                    let Some(href) = a.value().attr("href") else { continue; };
                    let Ok(resolved_url) = parsed_base.join(href) else { continue; };
                    let full_url = resolved_url.to_string();

                    if !full_url.starts_with("http://") && !full_url.starts_with("https://") {
                        continue;
                    }
                    if full_url == base_url || seen_urls.contains(&full_url) {
                        continue;
                    }
                    // Skip common navigational links
                    let path_lower = resolved_url.path().to_lowercase();
                    if path_lower.contains("login")
                        || path_lower.contains("signup")
                        || path_lower.contains("privacy")
                        || path_lower.contains("terms")
                        || path_lower.contains("cookie")
                        || path_lower.contains("contact")
                        || path_lower == "/"
                    {
                        continue;
                    }

                    let link_text = a
                        .text()
                        .map(|s| s.trim())
                        .filter(|s| !s.is_empty())
                        .collect::<Vec<_>>()
                        .join(" ");

                    if link_text.len() < 15 {
                        continue;
                    }

                    seen_urls.insert(full_url.clone());
                    let published_at = now.unwrap_or_else(Utc::now);
                    let clean = sanitize_and_truncate(Some(&link_text), &self.cleaning);

                    let mut item = NewsItem::new(
                        &link_text,
                        &full_url,
                        published_at,
                        format!("{} (Web Scraper)", source_name),
                    );
                    item.raw_content = Some(link_text);
                    item.clean_snippet = Some(clean);
                    item.summary = None;
                    items.push(item);

                    if items.len() >= 20 {
                        break;
                    }
                }
            }
        }

        filter_by_time_window(items, hours, now, true)
    }

    /// Perform a scraper fallback for an X/Twitter account.
    ///
    /// Requirements:
    /// - Always link to user's profile URL (do NOT link to individual status/tweet IDs).
    /// - Extract tweet text, preserving any external links in content.
    /// - Mark summary as None for subsequent TextRank summarization.
    pub async fn scrape_twitter_fallback(
        &self,
        username: &str,
        source_label: &str,
        hours: i64,
        now: Option<DateTime<Utc>>,
    ) -> Vec<NewsItem> {
        if !self.enabled {
            return Vec::new();
        }

        let clean_user = username.trim_start_matches('@').trim();
        let profile_url = format!("https://x.com/{}", clean_user);

        if self.pacing_seconds > 0.0 {
            tokio::time::sleep(Duration::from_secs_f64(self.pacing_seconds)).await;
        }

        // Candidate endpoints for Twitter/X syndication or public mirrors:
        let candidates = [
            format!(
                "https://syndication.twitter.com/srv/timeline-profile/screen-name/{}",
                clean_user
            ),
            format!("https://xcancel.com/{}", clean_user),
            format!("https://nitter.privacydev.net/{}", clean_user),
        ];

        for target_url in candidates {
            tracing::info!(
                "Scraping Twitter fallback for @{} from {}",
                clean_user,
                target_url
            );

            match self.fetcher.fetch_text(&target_url, &[]).await {
                Ok(html) => {
                    let items = self.parse_twitter_html(
                        &html,
                        clean_user,
                        &profile_url,
                        source_label,
                        hours,
                        now,
                    );
                    if !items.is_empty() {
                        tracing::info!(
                            "Twitter scraper for @{} retrieved {} posts from {}",
                            clean_user,
                            items.len(),
                            target_url
                        );
                        return items;
                    }
                }
                Err(err) => {
                    tracing::debug!(
                        "Could not fetch Twitter fallback {} for @{}: {}",
                        target_url,
                        clean_user,
                        err
                    );
                }
            }
        }

        tracing::warn!(
            "Could not scrape Twitter timeline for @{} across public mirrors",
            clean_user
        );
        Vec::new()
    }

    /// Parse Twitter syndication or mirror HTML content into news items.
    pub fn parse_twitter_html(
        &self,
        html: &str,
        clean_user: &str,
        profile_url: &str,
        source_label: &str,
        hours: i64,
        now: Option<DateTime<Utc>>,
    ) -> Vec<NewsItem> {
        let doc = Html::parse_document(html);
        let mut items = Vec::new();

        // Selectors covering Twitter Syndication and Nitter mirrors
        let tweet_selectors = [
            "[data-testid='tweet']",
            "div.timeline-Tweet",
            "div.timeline-item",
            "article",
            "div.tweet-body",
        ];

        let text_sel = Selector::parse(
            "[data-testid='tweetText'], p.timeline-Tweet-text, div.tweet-content, p, [class*='text']",
        )
        .expect("Valid tweet text selector");
        let time_sel = Selector::parse("time[datetime], time, [class*='date'], a[title]")
            .expect("Valid tweet time selector");
        let link_sel = Selector::parse("a[href]").expect("Valid link selector");

        for sel_str in tweet_selectors {
            if let Ok(container_sel) = Selector::parse(sel_str) {
                for tweet_el in doc.select(&container_sel) {
                    // Extract text
                    let tweet_text = tweet_el
                        .select(&text_sel)
                        .next()
                        .map(|el| {
                            el.text()
                                .map(|s| s.trim())
                                .filter(|s| !s.is_empty())
                                .collect::<Vec<_>>()
                                .join(" ")
                        })
                        .unwrap_or_default();

                    let tweet_text = tweet_text.trim();
                    if tweet_text.len() < 5 {
                        continue;
                    }

                    // Preserve external URLs mentioned in the tweet
                    let mut external_links = Vec::new();
                    for a in tweet_el.select(&link_sel) {
                        if let Some(href) = a.value().attr("href") {
                            let h = href.trim();
                            if (h.starts_with("http://") || h.starts_with("https://"))
                                && !h.contains("twitter.com/")
                                && !h.contains("x.com/")
                                && !h.contains("pic.twitter.com")
                                && !tweet_text.contains(h)
                            {
                                external_links.push(h.to_string());
                            }
                        }
                    }
                    external_links.dedup();

                    let full_content = if external_links.is_empty() {
                        tweet_text.to_string()
                    } else {
                        format!("{} (Links: {})", tweet_text, external_links.join(", "))
                    };

                    // Extract date
                    let mut date_opt = None;
                    if let Some(t_el) = tweet_el.select(&time_sel).next() {
                        if let Some(dt) = t_el.value().attr("datetime") {
                            date_opt = parse_datetime_str(dt);
                        }
                        if date_opt.is_none() {
                            if let Some(title_dt) = t_el.value().attr("title") {
                                date_opt = parse_datetime_str(title_dt);
                            }
                        }
                        if date_opt.is_none() {
                            let text = t_el.text().collect::<String>();
                            date_opt = parse_datetime_str(&text);
                        }
                    }
                    let published_at = date_opt.unwrap_or_else(|| now.unwrap_or_else(Utc::now));

                    // Title format: "@{clean_user}: [preview]"
                    let preview: String = tweet_text.chars().take(80).collect();
                    let title = format!("@{}: {}", clean_user, preview.trim());

                    let clean = sanitize_and_truncate(Some(&full_content), &self.cleaning);

                    // Requirement: enforce profile URL, do NOT link to individual status IDs
                    let mut item = NewsItem::new(
                        &title,
                        profile_url,
                        published_at,
                        format!("{} (Web Scraper)", source_label),
                    );
                    item.raw_content = Some(full_content);
                    item.clean_snippet = Some(clean);
                    item.summary = None;
                    items.push(item);
                }
            }

            if !items.is_empty() {
                break;
            }
        }

        filter_by_time_window(items, hours, now, true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn derives_html_urls_from_rss_feed() {
        let derived = WebScraperFallback::derive_html_urls("https://openai.com/news/rss.xml");
        assert!(derived.contains(&"https://openai.com/news/".to_string()));
        assert!(derived.contains(&"https://openai.com/news/rss.xml".to_string()));

        let derived_feed = WebScraperFallback::derive_html_urls("https://www.anthropic.com/feed.xml");
        assert!(derived_feed.contains(&"https://www.anthropic.com/".to_string()));
    }

    #[test]
    fn parses_blog_html_with_articles() {
        let html = r#"
        <!DOCTYPE html>
        <html>
        <body>
            <article>
                <h2><a href="/research/new-model">Announcing New Breakthrough Model</a></h2>
                <time datetime="2026-10-16T10:00:00Z">Oct 16, 2026</time>
                <p>Today we are introducing a new breakthrough foundation model with SOTA reasoning performance.</p>
            </article>
        </body>
        </html>
        "#;

        let scraper = WebScraperFallback::new(
            &SearchFallbackConfig::default(),
            &CleaningConfig::default(),
        );

        let items = scraper.parse_blog_html(
            html,
            "https://example.com/blog/",
            "Example AI",
            48,
            None,
        );

        assert_eq!(items.len(), 1);
        let item = &items[0];
        assert_eq!(item.title, "Announcing New Breakthrough Model");
        assert_eq!(item.url, "https://example.com/research/new-model");
        assert_eq!(item.source, "Example AI (Web Scraper)");
        assert!(item.summary.is_none());
        assert!(item.clean_snippet.as_ref().unwrap().contains("Today we are introducing"));
    }

    #[test]
    fn parses_twitter_html_enforcing_profile_url_and_preserving_links() {
        let html = r#"
        <div class="timeline-Tweet">
            <p class="timeline-Tweet-text">
                Excited to open source our new benchmark! Check out the code:
                <a href="https://github.com/example/benchmark">github.com/example/benchmark</a>
            </p>
            <time datetime="2026-10-16T09:00:00Z"></time>
        </div>
        "#;

        let scraper = WebScraperFallback::new(
            &SearchFallbackConfig::default(),
            &CleaningConfig::default(),
        );

        let items = scraper.parse_twitter_html(
            html,
            "karpathy",
            "https://x.com/karpathy",
            "Researchers",
            48,
            None,
        );

        assert_eq!(items.len(), 1);
        let item = &items[0];
        assert_eq!(item.url, "https://x.com/karpathy");
        assert!(item.title.starts_with("@karpathy: Excited to open source"));
        assert!(item.clean_snippet.as_ref().unwrap().contains("https://github.com/example/benchmark"));
        assert!(item.summary.is_none());
    }

    #[test]
    fn respects_disabled_config() {
        let mut cfg = SearchFallbackConfig::default();
        cfg.enabled = false;
        let scraper = WebScraperFallback::new(&cfg, &CleaningConfig::default());
        assert!(!scraper.is_enabled());
    }
}
