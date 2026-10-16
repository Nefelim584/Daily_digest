//! HTML sanitization and text truncation utilities (Phase 27).
//!
//! Ported from the Python regex-based `HTMLCleaner` and `truncator`.
//! Regexes are compiled once via `OnceLock`. The Rust `regex` crate has no
//! backreferences, so paired-tag patterns are spelled out per tag.

use std::sync::OnceLock;

use regex::{Captures, Regex};

use crate::config::CleaningConfig;

struct Patterns {
    script_style: Regex,
    image: Regex,
    media: Regex,
    bare_media_url: Regex,
    link: Regex,
    html_tag: Regex,
    whitespace: Regex,
    punct_whitespace: Regex,
    multiple_newlines: Regex,
}

fn patterns() -> &'static Patterns {
    static PATTERNS: OnceLock<Patterns> = OnceLock::new();
    PATTERNS.get_or_init(|| Patterns {
        script_style: Regex::new(
            r"(?is)<script\b[^>]*>.*?</script>|<style\b[^>]*>.*?</style>|<noscript\b[^>]*>.*?</noscript>",
        )
        .unwrap(),
        image: Regex::new(
            r"(?is)<img\b[^>]*>|<picture\b[^>]*>.*?</picture>|<figure\b[^>]*>.*?</figure>|!\[.*?\]\(.*?\)",
        )
        .unwrap(),
        media: Regex::new(
            r"(?is)<video\b[^>]*>.*?</video>|<audio\b[^>]*>.*?</audio>|<source\b[^>]*>.*?</source>|<video\b[^>]*>|<audio\b[^>]*>|<source\b[^>]*>",
        )
        .unwrap(),
        bare_media_url: Regex::new(
            r"(?i)https?://\S+?\.(?:mp4|webm|m4v|ogg|ogv|mov|avi|wmv|mp3|wav|flac|png|jpg|jpeg|gif|webp|svg)(?:\?\S*)?",
        )
        .unwrap(),
        link: Regex::new(r"(?is)<a\b[^>]*>(.*?)</a>|\[([^\]]+)\]\([^\)]+\)").unwrap(),
        html_tag: Regex::new(r"(?s)<[^>]+>").unwrap(),
        whitespace: Regex::new(r"[ \t\r\x0b\x0c]+").unwrap(),
        punct_whitespace: Regex::new(r"\s+([.,!?;:)\]])").unwrap(),
        multiple_newlines: Regex::new(r"\n{3,}").unwrap(),
    })
}

/// Clean and sanitize input text/HTML according to configuration.
pub fn clean_html(raw: &str, config: &CleaningConfig) -> String {
    if raw.is_empty() {
        return String::new();
    }

    let p = patterns();
    let mut text = raw.to_string();

    // 1. Strip script/style/noscript blocks.
    if config.strip_scripts {
        text = p.script_style.replace_all(&text, " ").into_owned();
    }

    // 2. Strip images and media.
    if config.strip_images {
        text = p.image.replace_all(&text, " ").into_owned();
        text = p.media.replace_all(&text, " ").into_owned();
        text = p.bare_media_url.replace_all(&text, " ").into_owned();
    }

    // 3. Strip links, keeping inner anchor/markdown text.
    if config.strip_links {
        text = p
            .link
            .replace_all(&text, |caps: &Captures| {
                caps.get(1)
                    .or_else(|| caps.get(2))
                    .map(|m| m.as_str().to_string())
                    .unwrap_or_default()
            })
            .into_owned();
    }

    // 4. Strip remaining HTML tags.
    if config.strip_html_tags {
        text = p.html_tag.replace_all(&text, " ").into_owned();
    }

    // 5. Decode HTML entities (&amp;, &quot;, &#39;, ...).
    text = html_escape::decode_html_entities(&text).into_owned();

    // 6. Normalize whitespace.
    text = p.whitespace.replace_all(&text, " ").into_owned();
    text = p.punct_whitespace.replace_all(&text, "$1").into_owned();
    let joined = text
        .split('\n')
        .map(|line| line.trim())
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join("\n");
    text = p.multiple_newlines.replace_all(&joined, "\n\n").into_owned();

    text.trim().to_string()
}

/// Truncate text to at most `max_characters`, preserving word boundaries when sensible.
pub fn truncate_text(text: &str, max_characters: usize, suffix: &str, break_on_word: bool) -> String {
    let text = text.trim();
    let chars: Vec<char> = text.chars().collect();

    if chars.len() <= max_characters {
        return text.to_string();
    }

    let suffix_len = suffix.chars().count();
    if max_characters <= suffix_len {
        return suffix.chars().take(max_characters).collect();
    }

    let target = max_characters - suffix_len;
    let cut: &[char] = &chars[..target];

    if break_on_word {
        if let Some(pos) = cut.iter().rposition(|&c| c == ' ') {
            if pos >= (target as f64 * 0.7) as usize {
                let s: String = cut[..pos].iter().collect();
                return format!("{}{}", s.trim_end(), suffix);
            }
        }
    }

    let s: String = cut.iter().collect();
    format!("{}{}", s.trim_end(), suffix)
}

/// Clean HTML and truncate the result according to `CleaningConfig`.
pub fn sanitize_and_truncate(raw: Option<&str>, config: &CleaningConfig) -> String {
    let cleaned = clean_html(raw.unwrap_or(""), config);
    truncate_text(&cleaned, config.max_characters, "...", true)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg() -> CleaningConfig {
        CleaningConfig::default()
    }

    #[test]
    fn strips_tags_and_decodes_entities() {
        let out = clean_html("<p>Hello <b>world</b> &amp; AI</p>", &cfg());
        assert_eq!(out, "Hello world & AI");
    }

    #[test]
    fn keeps_anchor_text() {
        let out = clean_html(r#"See <a href="http://x">the paper</a> now"#, &cfg());
        assert_eq!(out, "See the paper now");
    }

    #[test]
    fn strips_scripts() {
        let out = clean_html("<script>evil()</script>Safe", &cfg());
        assert_eq!(out, "Safe");
    }

    #[test]
    fn truncates_on_word_boundary() {
        let out = truncate_text("the quick brown fox jumps", 12, "...", true);
        assert!(out.ends_with("..."));
        assert!(out.len() <= 12);
    }
}
