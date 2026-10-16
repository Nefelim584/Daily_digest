//! Command-line interface definition and execution handler (Phase 34).
//!
//! Provides clap-based CLI options matching the Python entrypoint:
//! `--hours`, `--config`, `--format`, `--output`, `--no-summary`, `--verbose`, `--serve`.
//! Ensures clean exit codes (0 on success, != 0 on error).

use std::path::PathBuf;

use clap::Parser;

/// Daily AI & Tech News Digest — single static binary, fully local.
#[derive(Parser, Debug, Clone)]
#[command(
    name = "daily-digest",
    about = "Autonomous AI & tech news digest — aggregates, prioritizes, and summarizes research",
    version
)]
pub struct CliArgs {
    /// Path to YAML configuration file
    #[arg(short = 'c', long = "config", default_value = "config/config.yaml")]
    pub config: PathBuf,

    /// Collection time window in hours (overrides config default)
    #[arg(short = 'H', long = "hours")]
    pub hours: Option<i64>,

    /// Digest output format: markdown, text, or html (overrides config default)
    #[arg(short = 'f', long = "format")]
    pub format: Option<String>,

    /// Destination directory for generated digest files (overrides config default)
    #[arg(short = 'o', long = "output")]
    pub output: Option<PathBuf>,

    /// Disable extractive summarization even if enabled in config
    #[arg(long = "no-summary", default_value_t = false)]
    pub no_summary: bool,

    /// Enable verbose debug logging output
    #[arg(short = 'v', long = "verbose", default_value_t = false)]
    pub verbose: bool,

    /// Start asynchronous HTTP API server (axum)
    #[arg(long = "serve", default_value_t = false)]
    pub serve: bool,

    /// Server host interface to listen on (default from config/env, e.g. 0.0.0.0)
    #[arg(long = "host")]
    pub host: Option<String>,

    /// Server port to listen on (default from config/env, e.g. 8000)
    #[arg(short = 'p', long = "port")]
    pub port: Option<u16>,
}

/// Execute CLI pipeline with the provided arguments, returning an exit code.
pub async fn run_with_args(args: CliArgs) -> i32 {
    let filter = if args.verbose { "debug" } else { "info" };
    let _ = tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new(filter)),
        )
        .try_init();

    // 1. Load configuration
    let mut config = match crate::config::load_config(&args.config) {
        Ok(cfg) => cfg,
        Err(err) => {
            eprintln!("Configuration error: {:#}", err);
            return 1;
        }
    };

    // Apply CLI overrides to config
    if let Some(host) = args.host {
        config.server.host = host;
    }
    if let Some(port) = args.port {
        config.server.port = port;
    }

    if args.serve {
        if let Err(err) = crate::server::run_server(config).await {
            eprintln!("Server error: {:#}", err);
            return 1;
        }
        return 0;
    }

    let pipeline = crate::pipeline::DigestPipeline::new(config);
    let enable_summarization = if args.no_summary { Some(false) } else { None };

    match pipeline
        .run(
            args.hours,
            args.format.as_deref(),
            args.output,
            enable_summarization,
            None,
        )
        .await
    {
        Ok(result) => {
            println!("\nDaily Digest Generated Successfully:");
            println!(" - Time window:  {} hours", result.time_window_hours);
            println!(
                " - Total items:  {} retrieved ({} unique)",
                result.total_collected, result.total_unique
            );
            if result.clusters_found > 0 {
                println!(" - Clusters:     {} topic groups", result.clusters_found);
            }
            println!(" - Digest file:  {}\n", result.output_path.display());
            0
        }
        Err(err) => {
            eprintln!("Pipeline execution failed: {:#}", err);
            1
        }
    }
}

/// Parse command-line arguments and run the application.
pub async fn run() -> i32 {
    let args = CliArgs::parse();
    run_with_args(args).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_default_args() {
        let args = CliArgs::parse_from(["daily-digest"]);
        assert_eq!(args.config, PathBuf::from("config/config.yaml"));
        assert_eq!(args.hours, None);
        assert_eq!(args.format, None);
        assert_eq!(args.output, None);
        assert!(!args.no_summary);
        assert!(!args.verbose);
        assert!(!args.serve);
    }

    #[test]
    fn parses_custom_flags() {
        let args = CliArgs::parse_from([
            "daily-digest",
            "-c",
            "custom/path.yaml",
            "-H",
            "48",
            "-f",
            "html",
            "-o",
            "dist/digests",
            "--no-summary",
            "-v",
            "--serve",
            "--host",
            "127.0.0.1",
            "-p",
            "9000",
        ]);
        assert_eq!(args.config, PathBuf::from("custom/path.yaml"));
        assert_eq!(args.hours, Some(48));
        assert_eq!(args.format.as_deref(), Some("html"));
        assert_eq!(args.output, Some(PathBuf::from("dist/digests")));
        assert!(args.no_summary);
        assert!(args.verbose);
        assert!(args.serve);
        assert_eq!(args.host.as_deref(), Some("127.0.0.1"));
        assert_eq!(args.port, Some(9000));
    }
}
