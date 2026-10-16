//! Core data models, including the standardized `NewsItem` (Phase 24).
//!
//! Ported from the Python `pydantic` `NewsItem`. Adds `cluster_id` to support
//! TF-IDF topic clustering (Phase 30).

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Standardized representation of a news item across all sources.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NewsItem {
    /// Headline or paper title.
    pub title: String,
    /// Link to source article or paper.
    pub url: String,
    /// Publication timestamp (UTC).
    pub published_at: DateTime<Utc>,
    /// Source name (e.g. "OpenAI Blog", "arXiv").
    pub source: String,
    /// Original uncleaned content or abstract from source.
    #[serde(default)]
    pub raw_content: Option<String>,
    /// Sanitized and truncated text preview.
    #[serde(default)]
    pub clean_snippet: Option<String>,
    /// Locally generated extractive summary.
    #[serde(default)]
    pub summary: Option<String>,
    /// Computed priority score for ordering (higher is more important).
    #[serde(default = "default_priority_score")]
    pub priority_score: i32,
    /// Name of the matching priority category.
    #[serde(default)]
    pub priority_group: Option<String>,
    /// Assigned topic cluster id (set during TF-IDF clustering).
    #[serde(default)]
    pub cluster_id: Option<usize>,
}

fn default_priority_score() -> i32 {
    10
}

impl NewsItem {
    /// Construct a new `NewsItem` with the required fields and sensible defaults.
    pub fn new(
        title: impl Into<String>,
        url: impl Into<String>,
        published_at: DateTime<Utc>,
        source: impl Into<String>,
    ) -> Self {
        Self {
            title: title.into(),
            url: url.into(),
            published_at,
            source: source.into(),
            raw_content: None,
            clean_snippet: None,
            summary: None,
            priority_score: default_priority_score(),
            priority_group: None,
            cluster_id: None,
        }
    }

    /// Best available text for processing: the clean snippet, else raw content.
    pub fn best_text(&self) -> Option<&str> {
        self.clean_snippet
            .as_deref()
            .filter(|s| !s.is_empty())
            .or(self.raw_content.as_deref().filter(|s| !s.is_empty()))
    }
}
