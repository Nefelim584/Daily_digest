//! Application configuration structs and YAML loader (Phase 24).
//!
//! Ported from the Python `pydantic` models. All Mistral/Gemini/API-key
//! fields are intentionally dropped — summarization is local (TextRank) and
//! the web-search fallback is a built-in scraper. Unknown YAML keys (e.g. the
//! legacy `mistral:` / `gemini:` blocks still present in `config/config.yaml`)
//! are ignored by serde, so the existing config keeps loading during migration.

use std::path::PathBuf;

use anyhow::{Context, Result};
use serde::Deserialize;

/// Network server settings.
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct ServerConfig {
    pub host: String,
    pub port: u16,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            host: "0.0.0.0".to_string(),
            port: 8000,
        }
    }
}

/// Collection time-window settings.
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct TimeWindowConfig {
    pub default_hours: u32,
}

impl Default for TimeWindowConfig {
    fn default() -> Self {
        Self { default_hours: 24 }
    }
}

/// Digest output settings.
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct OutputConfig {
    pub format: String,
    pub directory: PathBuf,
    pub filename_pattern: String,
}

impl Default for OutputConfig {
    fn default() -> Self {
        Self {
            format: "markdown".to_string(),
            directory: PathBuf::from("data/digests"),
            filename_pattern: "digest_{date}.{ext}".to_string(),
        }
    }
}

impl OutputConfig {
    /// Normalize the configured format into one of: `markdown`, `text`, `html`.
    pub fn normalized_format(&self) -> String {
        match self.format.trim().to_lowercase().as_str() {
            "markdown" | "md" => "markdown".to_string(),
            "text" | "txt" => "text".to_string(),
            "html" => "html".to_string(),
            _ => "markdown".to_string(),
        }
    }
}

/// Regex-based HTML cleaning and truncation settings.
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct CleaningConfig {
    pub max_characters: usize,
    pub strip_html_tags: bool,
    pub strip_images: bool,
    pub strip_links: bool,
    pub strip_scripts: bool,
}

impl Default for CleaningConfig {
    fn default() -> Self {
        Self {
            max_characters: 1500,
            strip_html_tags: true,
            strip_images: true,
            strip_links: true,
            strip_scripts: true,
        }
    }
}

/// RSSHub integration settings.
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct RsshubConfig {
    pub base_url: String,
    pub timeout_seconds: u64,
    pub access_key: String,
}

impl Default for RsshubConfig {
    fn default() -> Self {
        Self {
            base_url: "https://rsshub.app".to_string(),
            timeout_seconds: 15,
            access_key: String::new(),
        }
    }
}

/// Local extractive summarization (TextRank) settings — replaces Mistral.
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct SummarizationConfig {
    pub enabled: bool,
    /// Number of top sentences to keep as the summary (2-3 bullet points).
    pub max_sentences: usize,
}

impl Default for SummarizationConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            max_sentences: 3,
        }
    }
}

/// TF-IDF semantic deduplication and topic clustering settings.
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct ClusteringConfig {
    pub enabled: bool,
    /// Cosine-similarity threshold above which two items are considered duplicates.
    pub dedup_threshold: f64,
    /// Number of topic clusters (0 = auto / heuristic based on item count).
    pub num_clusters: usize,
}

impl Default for ClusteringConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            dedup_threshold: 0.80,
            num_clusters: 0,
        }
    }
}

/// Built-in web-scraper fallback settings — replaces Gemini/Mistral web search.
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct SearchFallbackConfig {
    pub enabled: bool,
    /// Pacing delay (seconds) between scraper requests.
    pub pacing_seconds: f64,
}

impl Default for SearchFallbackConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            pacing_seconds: 1.0,
        }
    }
}

/// Regex-based priority rule for category assignment.
#[derive(Debug, Clone, Deserialize)]
pub struct PriorityRule {
    pub name: String,
    pub priority: i32,
    pub regex: String,
}

/// Configuration for a single news source.
#[derive(Debug, Clone, Deserialize)]
pub struct SourceConfig {
    pub name: String,
    /// Collector adapter type: `arxiv_api`, `huggingface_papers_api`, `rss`, `rsshub`.
    #[serde(rename = "type")]
    pub source_type: String,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default)]
    pub endpoint: Option<String>,
    #[serde(default)]
    pub routes: Vec<String>,
    #[serde(default)]
    pub categories: Vec<String>,
    #[serde(default = "default_max_results")]
    pub max_results: usize,
}

fn default_true() -> bool {
    true
}

fn default_max_results() -> usize {
    50
}

/// Root application configuration.
#[derive(Debug, Clone, Deserialize, Default)]
#[serde(default)]
pub struct AppConfig {
    pub server: ServerConfig,
    pub time_window: TimeWindowConfig,
    pub output: OutputConfig,
    pub cleaning: CleaningConfig,
    pub rsshub: RsshubConfig,
    pub summarization: SummarizationConfig,
    pub clustering: ClusteringConfig,
    pub search: SearchFallbackConfig,
    pub priorities: Vec<PriorityRule>,
    pub sources: Vec<SourceConfig>,
}

/// Load, validate, and enrich application configuration from a YAML file.
///
/// Applies 12-Factor environment overrides after parsing, mirroring the
/// behaviour of the Python loader (minus the removed API-key resolution).
pub fn load_config(config_path: impl Into<PathBuf>) -> Result<AppConfig> {
    let path = config_path.into();

    let raw = std::fs::read_to_string(&path)
        .with_context(|| format!("Configuration file not found at: {}", path.display()))?;

    let mut config: AppConfig = serde_yaml::from_str(&raw)
        .with_context(|| format!("Failed to parse/validate YAML at {}", path.display()))?;

    // Load environment variables from a local .env file if present (best-effort).
    let _ = dotenvy::dotenv();

    apply_env_overrides(&mut config);
    Ok(config)
}

/// Apply 12-Factor environment-variable overrides onto a parsed config.
fn apply_env_overrides(config: &mut AppConfig) {
    use std::env;

    if let Some(dir) = first_env(&["DIGEST_OUTPUT_DIR", "BOT_DIGESTS_PATH"]) {
        config.output.directory = PathBuf::from(dir.trim());
    }

    if let Ok(fmt) = env::var("DIGEST_OUTPUT_FORMAT") {
        config.output.format = fmt.trim().to_string();
    }

    if let Some(host) = first_env(&["DIGEST_HOST", "HOST"]) {
        config.server.host = host.trim().to_string();
    }

    if let Some(port) = first_env(&["DIGEST_PORT", "PORT"]) {
        if let Ok(p) = port.trim().parse::<u16>() {
            config.server.port = p;
        }
    }

    if let Some(hours) = first_env(&["DIGEST_DEFAULT_HOURS", "DEFAULT_HOURS"]) {
        if let Ok(h) = hours.trim().parse::<u32>() {
            config.time_window.default_hours = h;
        }
    }

    if let Ok(url) = env::var("RSSHUB_BASE_URL") {
        config.rsshub.base_url = url.trim().trim_end_matches('/').to_string();
    }
}

/// Return the first non-empty value among the given environment variable names.
fn first_env(names: &[&str]) -> Option<String> {
    for name in names {
        if let Ok(val) = std::env::var(name) {
            if !val.trim().is_empty() {
                return Some(val);
            }
        }
    }
    None
}
