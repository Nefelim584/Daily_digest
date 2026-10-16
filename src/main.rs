//! Daily Digest — Rust rewrite entrypoint.
//!
//! Big-bang migration (see todo.md "Rust Migration Epic").
//! Fully local: extractive TextRank summarization, TF-IDF embeddings,
//! built-in web scraper fallback. No model API calls, no API keys.

// Modules are still being filled in phase by phase; silence dead-code noise
// for stubs until the migration completes (Phase 37).
#![allow(dead_code)]

mod classifier;
mod cleaning;
mod collectors;
mod config;
mod export;
mod filters;
mod formatters;
mod ml;
mod models;
mod network;
mod pipeline;
mod search;
mod server;
mod summarization;

use anyhow::Result;

fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    // Phase 24 smoke test: load and summarize the configuration.
    let cfg = config::load_config("config/config.yaml")?;
    tracing::info!(
        "Loaded config: {} sources, {} priority rules, default_hours={}, output_format={}",
        cfg.sources.len(),
        cfg.priorities.len(),
        cfg.time_window.default_hours,
        cfg.output.normalized_format(),
    );

    Ok(())
}
