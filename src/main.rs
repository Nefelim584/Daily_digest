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
mod cli;
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

#[tokio::main]
async fn main() {
    let exit_code = cli::run().await;
    std::process::exit(exit_code);
}
