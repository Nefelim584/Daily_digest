//! End-to-end pipeline orchestrator wiring all stages together (Phase 33).
//!
//! Coordinates collection -> cleaning -> time-window filtering -> exact dedup ->
//! semantic dedup -> priority classification -> extractive summarization ->
//! topic clustering & 2D visualization projection -> sorting -> formatting -> atomic export.

use std::collections::HashSet;
use std::path::PathBuf;

use anyhow::Result;
use chrono::{DateTime, Utc};
use reqwest::Url;

use crate::classifier::PriorityClassifier;
use crate::collectors;
use crate::config::AppConfig;
use crate::export::DigestExporter;
use crate::filters::filter_by_time_window;
use crate::formatters::{self, ClusterPoint};
use crate::ml;
use crate::models::NewsItem;
use crate::summarization::ExtractiveSummarizer;

/// Summary of pipeline execution results.
#[derive(Debug, Clone)]
pub struct PipelineResult {
    /// Number of raw items collected across all sources before deduplication.
    pub total_collected: usize,
    /// Number of unique items included in the final digest.
    pub total_unique: usize,
    /// Number of topic clusters generated (if clustering was enabled).
    pub clusters_found: usize,
    /// Time window hours used for filtering.
    pub time_window_hours: i64,
    /// Destination path of the exported digest file.
    pub output_path: PathBuf,
    /// The processed and formatted items.
    pub items: Vec<NewsItem>,
}

/// Normalize URL for exact deduplication.
///
/// Strips marketing/tracking query parameters (e.g. `utm_*`, `ref`, `source`, `fbclid`),
/// lowercases scheme and domain, removes fragments and trailing slashes.
pub fn normalize_url(raw: &str) -> String {
    let trimmed = raw.trim();
    if let Ok(mut parsed) = Url::parse(trimmed) {
        let _ = parsed.set_scheme(&parsed.scheme().to_lowercase());
        if let Some(host) = parsed.host_str() {
            let host_lower = host.to_lowercase();
            let _ = parsed.set_host(Some(&host_lower));
        }

        let path = parsed.path().trim_end_matches('/').to_string();
        let clean_path = if path.is_empty() { "/" } else { &path };
        parsed.set_path(clean_path);

        // Filter out common tracking query parameters
        let tracking_prefixes = ["utm_", "ref", "source", "fbclid", "gclid", "mc_"];
        let filtered_query: Vec<(String, String)> = parsed
            .query_pairs()
            .filter(|(k, _)| {
                let k_lower = k.to_lowercase();
                !tracking_prefixes
                    .iter()
                    .any(|prefix| k_lower.starts_with(prefix))
            })
            .map(|(k, v)| (k.into_owned(), v.into_owned()))
            .collect();

        if filtered_query.is_empty() {
            parsed.set_query(None);
        } else {
            parsed.query_pairs_mut().clear().extend_pairs(&filtered_query);
        }

        parsed.set_fragment(None);
        parsed.to_string()
    } else {
        trimmed.to_string()
    }
}

/// Exact deduplication by normalized canonical URL and normalized title.
///
/// Keeps the first occurrence of each unique item.
pub fn exact_dedup(items: Vec<NewsItem>) -> Vec<NewsItem> {
    let mut seen_urls = HashSet::new();
    let mut seen_titles = HashSet::new();
    let mut unique = Vec::new();

    for item in items {
        let norm_u = normalize_url(&item.url);
        let norm_t: String = item
            .title
            .to_lowercase()
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ");

        if seen_urls.contains(&norm_u) {
            continue;
        }
        if !norm_t.is_empty() && seen_titles.contains(&norm_t) {
            continue;
        }

        seen_urls.insert(norm_u);
        if !norm_t.is_empty() {
            seen_titles.insert(norm_t);
        }
        unique.push(item);
    }

    unique
}

/// End-to-end pipeline orchestrator for Daily Digest.
pub struct DigestPipeline {
    config: AppConfig,
}

impl DigestPipeline {
    /// Initialize pipeline with application configuration.
    pub fn new(config: AppConfig) -> Self {
        Self { config }
    }

    /// Access the underlying application configuration.
    pub fn config(&self) -> &AppConfig {
        &self.config
    }

    /// Execute the complete daily digest generation pipeline.
    pub async fn run(
        &self,
        hours: Option<i64>,
        output_format: Option<&str>,
        output_dir: Option<PathBuf>,
        enable_summarization: Option<bool>,
        now: Option<DateTime<Utc>>,
    ) -> Result<PipelineResult> {
        let active_hours = hours.unwrap_or(self.config.time_window.default_hours as i64);
        let active_format = output_format
            .map(|f| f.to_string())
            .unwrap_or_else(|| self.config.output.normalized_format());
        let active_dir = output_dir.unwrap_or_else(|| self.config.output.directory.clone());
        let should_summarize = enable_summarization.unwrap_or(self.config.summarization.enabled);
        let current_now = now.unwrap_or_else(Utc::now);

        tracing::info!(
            "Starting Daily Digest pipeline: window={}h, format={}, summarization={}, output_dir={}",
            active_hours,
            active_format,
            should_summarize,
            active_dir.display(),
        );

        // 1. Collection: run collectors concurrently across all enabled sources
        let collectors = collectors::create_collectors(&self.config);
        tracing::info!("Running {} collectors concurrently", collectors.len());

        let mut handles = Vec::new();
        for collector in collectors {
            let h = active_hours;
            let n = Some(current_now);
            handles.push(tokio::spawn(async move {
                collector.collect(h, n).await
            }));
        }

        let mut all_raw_items = Vec::new();
        for handle in handles {
            match handle.await {
                Ok(items) => all_raw_items.extend(items),
                Err(err) => tracing::error!("Collector task encountered join error: {}", err),
            }
        }

        let total_collected = all_raw_items.len();
        tracing::info!("Collected {} total raw items", total_collected);

        // 2. Time-window filtering
        let filtered = filter_by_time_window(all_raw_items, active_hours, Some(current_now), false);

        // 3. Exact deduplication (normalized URL & title)
        let exact_unique = exact_dedup(filtered);
        tracing::info!(
            "Exact deduplication: {} unique items remaining",
            exact_unique.len()
        );

        // 4. Semantic deduplication (TF-IDF cosine similarity)
        let deduped = if self.config.clustering.enabled && self.config.clustering.dedup_threshold > 0.0 {
            let before = exact_unique.len();
            let items = ml::semantic_dedup(exact_unique, self.config.clustering.dedup_threshold);
            let removed = before.saturating_sub(items.len());
            if removed > 0 {
                tracing::info!(
                    "Semantic deduplication (threshold={:.2}) removed {} near-duplicates",
                    self.config.clustering.dedup_threshold,
                    removed
                );
            }
            items
        } else {
            exact_unique
        };

        // 5. Priority classification
        let classifier = PriorityClassifier::new(&self.config.priorities);
        let classified = classifier.classify_items(deduped);

        // 6. Extractive summarization (TextRank)
        let processed = if should_summarize {
            tracing::info!(
                "Running TextRank extractive summarization on {} items",
                classified.len()
            );
            let summarizer = ExtractiveSummarizer::new(self.config.summarization.clone());
            summarizer.summarize_items(classified)
        } else {
            classified
        };

        // 7. Sorting by priority score descending, then published_at descending
        let mut sorted_items = classifier.sort_items_by_priority(processed);

        // 8. Topic clustering & 2D visualization projection
        let mut clusters_found = 0;
        let mut chart_svg = None;

        if self.config.clustering.enabled && !sorted_items.is_empty() {
            let (k, coords) =
                ml::cluster_and_project(&mut sorted_items, self.config.clustering.num_clusters);
            clusters_found = k;
            tracing::info!("Grouped items into {} topic clusters", clusters_found);

            if active_format.eq_ignore_ascii_case("html") && sorted_items.len() >= 2 {
                let points: Vec<ClusterPoint> = sorted_items
                    .iter()
                    .zip(coords.iter())
                    .map(|(item, &(x, y))| ClusterPoint {
                        x,
                        y,
                        cluster: item.cluster_id.unwrap_or(0),
                        label: item.title.clone(),
                    })
                    .collect();
                chart_svg = formatters::render_cluster_chart_svg(&points);
            }
        }

        // 9. Atomic formatting and export
        let mut out_cfg = self.config.output.clone();
        out_cfg.format = active_format.clone();
        out_cfg.directory = active_dir;

        let exporter = DigestExporter::new(out_cfg, true);
        let output_path = exporter.export_digest(
            &sorted_items,
            active_hours,
            &active_format,
            current_now,
            chart_svg.as_deref(),
        )?;

        tracing::info!(
            "Digest successfully generated: {} items saved to {}",
            sorted_items.len(),
            output_path.display()
        );

        Ok(PipelineResult {
            total_collected,
            total_unique: sorted_items.len(),
            clusters_found,
            time_window_hours: active_hours,
            output_path,
            items: sorted_items,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normalize_url_strips_tracking() {
        let u1 = "https://example.com/post?utm_source=twitter&utm_medium=social&ref=123";
        assert_eq!(normalize_url(u1), "https://example.com/post");

        let u2 = "https://example.com/post/?id=42&utm_campaign=spring";
        assert_eq!(normalize_url(u2), "https://example.com/post?id=42");

        let u3 = "HTTPS://EXAMPLE.COM/Page/";
        assert_eq!(normalize_url(u3), "https://example.com/Page");
    }

    #[test]
    fn test_exact_dedup_removes_duplicates() {
        let now = Utc::now();
        let item1 = NewsItem::new(
            "New Breakthrough Model Released",
            "https://example.com/item?utm_source=feed",
            now,
            "Source A",
        );
        let item2 = NewsItem::new(
            "New Breakthrough Model Released",
            "https://example.com/item?utm_source=twitter",
            now,
            "Source B",
        );
        let item3 = NewsItem::new(
            "Different Paper on Reasoning",
            "https://example.com/different",
            now,
            "Source C",
        );

        let items = vec![item1, item2, item3];
        let deduped = exact_dedup(items);

        assert_eq!(deduped.len(), 2);
        assert_eq!(deduped[0].source, "Source A");
        assert_eq!(deduped[1].title, "Different Paper on Reasoning");
    }

    #[tokio::test]
    async fn test_pipeline_run_with_empty_sources() {
        let temp_dir = tempfile::tempdir().unwrap();
        let mut cfg = AppConfig::default();
        cfg.output.directory = temp_dir.path().to_path_buf();
        cfg.sources.clear(); // no sources configured

        let pipeline = DigestPipeline::new(cfg);
        let result = pipeline
            .run(None, Some("markdown"), None, None, None)
            .await
            .unwrap();

        assert_eq!(result.total_collected, 0);
        assert_eq!(result.total_unique, 0);
        assert!(result.output_path.exists());
    }
}
