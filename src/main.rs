//! Daily Digest — Rust rewrite entrypoint.
//!
//! Big-bang migration (see todo.md "Rust Migration Epic").
//! Fully local: extractive TextRank summarization, TF-IDF embeddings,
//! built-in web scraper fallback. No model API calls, no API keys.

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

fn main() {
    tracing_subscriber::fmt().with_env_filter("info").init();
    tracing::info!("daily-digest (Rust) bootstrap — module skeleton ready");
}
