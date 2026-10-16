//! Atomic digest export writer (`.tmp` -> rename) with orphan cleanup
//! (Phase 31).
//!
//! Ported from the Python `DigestExporter`. Writes to a hidden temporary file
//! in the destination directory and atomically renames it into place, so a
//! consumer never reads a half-written digest.

use std::io;
use std::path::PathBuf;

use chrono::{DateTime, Utc};

use crate::config::OutputConfig;
use crate::formatters;
use crate::models::NewsItem;

/// Handles writing formatted digests to disk with templated filenames.
pub struct DigestExporter {
    config: OutputConfig,
}

impl DigestExporter {
    /// Create an exporter, optionally cleaning orphan `.tmp` files up front.
    pub fn new(config: OutputConfig, clean_orphans_on_init: bool) -> Self {
        let exporter = Self { config };
        if clean_orphans_on_init {
            exporter.cleanup_temporary_files();
        }
        exporter
    }

    /// Remove orphan temporary files (`*.tmp`) left by interrupted runs.
    pub fn cleanup_temporary_files(&self) -> usize {
        let mut removed = 0;
        let Ok(entries) = std::fs::read_dir(&self.config.directory) else {
            return 0;
        };

        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_file() {
                continue;
            }
            if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                if name.ends_with(".tmp") && std::fs::remove_file(&path).is_ok() {
                    removed += 1;
                    tracing::info!("Cleaned up orphan temporary file: {}", path.display());
                }
            }
        }

        removed
    }

    /// Generate a filename from the configured pattern and timestamp.
    pub fn generate_filename(&self, format_name: &str, generated_at: DateTime<Utc>) -> String {
        let date_str = generated_at.format("%Y-%m-%d_%H%M").to_string();
        let ext = extension_for(format_name);
        let name = self
            .config
            .filename_pattern
            .replace("{date}", &date_str)
            .replace("{ext}", ext);

        if name.is_empty() {
            format!("digest_{}.{}", date_str, ext)
        } else {
            name
        }
    }

    /// Save content to disk atomically (temp file + rename).
    pub fn save(
        &self,
        content: &str,
        format_name: &str,
        generated_at: DateTime<Utc>,
    ) -> io::Result<PathBuf> {
        let filename = self.generate_filename(format_name, generated_at);
        let dir = &self.config.directory;
        std::fs::create_dir_all(dir)?;

        let target = dir.join(&filename);
        let tmp = dir.join(format!(".{}.tmp", filename));

        tracing::info!(
            "Writing digest atomically via temp file {} -> {}",
            tmp.display(),
            target.display()
        );

        if let Err(err) = std::fs::write(&tmp, content) {
            let _ = std::fs::remove_file(&tmp);
            return Err(err);
        }
        if let Err(err) = std::fs::rename(&tmp, &target) {
            let _ = std::fs::remove_file(&tmp);
            return Err(err);
        }

        tracing::info!("Digest successfully written to {}", target.display());
        Ok(target)
    }

    /// Format and save items in one step.
    pub fn export_digest(
        &self,
        items: &[NewsItem],
        time_window_hours: i64,
        format_name: &str,
        generated_at: DateTime<Utc>,
        chart_svg: Option<&str>,
    ) -> io::Result<PathBuf> {
        let fmt = format_name.trim().to_lowercase();
        let content = match fmt.as_str() {
            "markdown" | "md" => formatters::format_markdown(items, time_window_hours, generated_at),
            "html" => formatters::format_html(items, time_window_hours, generated_at, chart_svg),
            _ => formatters::format_text(items, time_window_hours, generated_at),
        };
        self.save(&content, &fmt, generated_at)
    }
}

/// File extension for a given output format.
fn extension_for(format_name: &str) -> &'static str {
    match format_name.trim().to_lowercase().as_str() {
        "markdown" | "md" => "md",
        "html" => "html",
        _ => "txt",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn config(dir: PathBuf) -> OutputConfig {
        OutputConfig {
            format: "markdown".to_string(),
            directory: dir,
            filename_pattern: "digest_{date}.{ext}".to_string(),
        }
    }

    #[test]
    fn generates_expected_filename() {
        let exporter = DigestExporter::new(config(PathBuf::from("data/digests")), false);
        let dt = Utc.with_ymd_and_hms(2025, 1, 2, 7, 30, 0).unwrap();
        assert_eq!(exporter.generate_filename("markdown", dt), "digest_2025-01-02_0730.md");
        assert_eq!(exporter.generate_filename("text", dt), "digest_2025-01-02_0730.txt");
        assert_eq!(exporter.generate_filename("html", dt), "digest_2025-01-02_0730.html");
    }

    #[test]
    fn atomic_save_writes_file_and_cleans_tmp() {
        let dir = tempfile::tempdir().unwrap();
        let exporter = DigestExporter::new(config(dir.path().to_path_buf()), false);
        let dt = Utc.with_ymd_and_hms(2025, 1, 2, 7, 30, 0).unwrap();

        let path = exporter.save("hello digest", "markdown", dt).unwrap();
        assert!(path.exists());
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "hello digest");

        // No leftover temp files.
        let tmp_count = std::fs::read_dir(dir.path())
            .unwrap()
            .flatten()
            .filter(|e| e.file_name().to_string_lossy().ends_with(".tmp"))
            .count();
        assert_eq!(tmp_count, 0);
    }

    #[test]
    fn cleanup_removes_orphan_tmp() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join(".digest.md.tmp"), "partial").unwrap();
        let exporter = DigestExporter::new(config(dir.path().to_path_buf()), false);
        assert_eq!(exporter.cleanup_temporary_files(), 1);
    }
}
