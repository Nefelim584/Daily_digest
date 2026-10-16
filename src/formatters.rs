//! Output formatters: Markdown, plain text, and HTML with a cluster scatter
//! chart (Phase 31).
//!
//! Ported from the Python `MarkdownFormatter` / `PlainTextFormatter`. The HTML
//! formatter is new and embeds a hand-rendered SVG scatter plot of the TF-IDF
//! topic clusters (coordinates computed in `ml::project_2d`).

use chrono::{DateTime, Utc};

use crate::models::NewsItem;

/// A projected 2D point for the cluster scatter chart.
pub struct ClusterPoint {
    pub x: f64,
    pub y: f64,
    pub cluster: usize,
    pub label: String,
}

const CLUSTER_COLORS: &[&str] = &[
    "#4e79a7", "#f28e2b", "#e15759", "#76b7b2", "#59a14f", "#edc948", "#b07aa1", "#ff9da7",
    "#9c755f", "#bab0ac",
];

fn fmt_dt(dt: DateTime<Utc>) -> String {
    dt.format("%Y-%m-%d %H:%M UTC").to_string()
}

fn section_icon(group: &str) -> &'static str {
    match group {
        "Major Releases & Breakthroughs" => "\u{1F680}", // 🚀
        "Research & Benchmarks" => "\u{1F52C}",          // 🔬
        "General News & Updates" => "\u{1F4F0}",         // 📰
        _ => "\u{1F4CC}",                                // 📌
    }
}

/// Group items by priority category, preserving first-seen (priority) order.
fn group_items(items: &[NewsItem]) -> Vec<(String, Vec<&NewsItem>)> {
    let mut groups: Vec<(String, Vec<&NewsItem>)> = Vec::new();
    for item in items {
        let name = item
            .priority_group
            .clone()
            .unwrap_or_else(|| "General News & Updates".to_string());
        if let Some(entry) = groups.iter_mut().find(|(n, _)| n == &name) {
            entry.1.push(item);
        } else {
            groups.push((name, vec![item]));
        }
    }
    groups
}

fn non_empty(opt: &Option<String>) -> Option<&str> {
    opt.as_deref().filter(|s| !s.trim().is_empty())
}

// ----------------------------------------------------------------------------
// Markdown
// ----------------------------------------------------------------------------

/// Render items into GitHub-flavored Markdown.
pub fn format_markdown(items: &[NewsItem], hours: i64, generated_at: DateTime<Utc>) -> String {
    let formatted_date = fmt_dt(generated_at);
    let mut lines: Vec<String> = vec![
        "# Daily AI & Research Digest".to_string(),
        String::new(),
        format!(
            "> **Generated:** {} | **Time Window:** Last {} Hours | **Total Items:** {}",
            formatted_date,
            hours,
            items.len()
        ),
        String::new(),
    ];

    if items.is_empty() {
        lines.push("No articles or papers were found for the selected time window.".to_string());
        lines.push(String::new());
        return lines.join("\n");
    }

    for (group_name, group_items) in group_items(items) {
        lines.push(format!("## {} {}", section_icon(&group_name), group_name));
        lines.push(String::new());

        for (idx, item) in group_items.iter().enumerate() {
            lines.push(format!("### {}. [{}]({})", idx + 1, item.title, item.url));
            lines.push(format!(
                "- **Source:** {} | **Published:** {}",
                item.source,
                fmt_dt(item.published_at)
            ));

            if let Some(summary) = non_empty(&item.summary) {
                lines.push("- **AI Summary:**".to_string());
                for line in summary.trim().split('\n') {
                    let s = line.trim();
                    if s.is_empty() {
                        continue;
                    }
                    if s.starts_with('-') || s.starts_with('*') || s.starts_with('\u{2022}') {
                        lines.push(format!("  {}", s));
                    } else {
                        lines.push(format!("  - {}", s));
                    }
                }
            } else if let Some(snippet) = non_empty(&item.clean_snippet) {
                lines.push("- **Snippet:**".to_string());
                lines.push(format!("  > {}", snippet));
            }

            lines.push(String::new());
        }

        lines.push("---".to_string());
        lines.push(String::new());
    }

    format!("{}\n", lines.join("\n").trim_end())
}

// ----------------------------------------------------------------------------
// Plain text
// ----------------------------------------------------------------------------

/// Render items into structured plain text.
pub fn format_text(items: &[NewsItem], hours: i64, generated_at: DateTime<Utc>) -> String {
    let divider_major = "=".repeat(80);
    let divider_minor = "-".repeat(80);
    let formatted_date = fmt_dt(generated_at);

    let mut lines: Vec<String> = vec![
        divider_major.clone(),
        "DAILY AI & RESEARCH DIGEST".to_string(),
        format!(
            "Generated: {} | Window: Last {} Hours | Total Items: {}",
            formatted_date,
            hours,
            items.len()
        ),
        divider_major.clone(),
        String::new(),
    ];

    if items.is_empty() {
        lines.push("No articles or papers were found for the selected time window.".to_string());
        lines.push(String::new());
        return lines.join("\n");
    }

    for (group_name, group_items) in group_items(items) {
        lines.push(divider_minor.clone());
        lines.push(format!(
            "[{}] ({} items)",
            group_name.to_uppercase(),
            group_items.len()
        ));
        lines.push(divider_minor.clone());
        lines.push(String::new());

        for (idx, item) in group_items.iter().enumerate() {
            lines.push(format!("{}. {}", idx + 1, item.title));
            lines.push(format!("   URL:       {}", item.url));
            lines.push(format!(
                "   Source:    {} | Published: {}",
                item.source,
                fmt_dt(item.published_at)
            ));

            if let Some(summary) = non_empty(&item.summary) {
                lines.push("   Summary:".to_string());
                for line in summary.trim().split('\n') {
                    let s = line.trim();
                    if !s.is_empty() {
                        lines.push(format!("     {}", s));
                    }
                }
            } else if let Some(snippet) = non_empty(&item.clean_snippet) {
                lines.push("   Snippet:".to_string());
                for line in snippet.trim().split('\n') {
                    let s = line.trim();
                    if !s.is_empty() {
                        lines.push(format!("     {}", s));
                    }
                }
            }

            lines.push(String::new());
        }
    }

    format!("{}\n", lines.join("\n").trim_end())
}

// ----------------------------------------------------------------------------
// HTML (+ cluster chart)
// ----------------------------------------------------------------------------

fn esc(s: &str) -> String {
    html_escape::encode_text(s).into_owned()
}

/// Render items into a standalone HTML page, optionally embedding a cluster SVG.
pub fn format_html(
    items: &[NewsItem],
    hours: i64,
    generated_at: DateTime<Utc>,
    chart_svg: Option<&str>,
) -> String {
    let mut html = String::new();
    html.push_str("<!DOCTYPE html>\n<html lang=\"en\">\n<head>\n<meta charset=\"utf-8\">\n");
    html.push_str("<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n");
    html.push_str("<title>Daily AI &amp; Research Digest</title>\n<style>\n");
    html.push_str(
        "body{font-family:-apple-system,Segoe UI,Roboto,Helvetica,Arial,sans-serif;max-width:900px;margin:0 auto;padding:24px;color:#1b1b1b;line-height:1.5}\
         h1{margin-bottom:4px}\
         .meta{color:#666;font-size:14px;margin-bottom:24px}\
         h2{border-bottom:2px solid #eee;padding-bottom:6px;margin-top:32px}\
         .item{margin:16px 0;padding:12px 16px;background:#fafafa;border:1px solid #eee;border-radius:8px}\
         .item a{color:#1a56db;text-decoration:none;font-weight:600}\
         .src{color:#777;font-size:13px;margin:4px 0}\
         .summary{margin:8px 0 0 0}\
         .chart{margin:24px 0;text-align:center}\n",
    );
    html.push_str("</style>\n</head>\n<body>\n");

    html.push_str("<h1>Daily AI &amp; Research Digest</h1>\n");
    html.push_str(&format!(
        "<div class=\"meta\">Generated: {} &middot; Time Window: Last {} Hours &middot; Total Items: {}</div>\n",
        esc(&fmt_dt(generated_at)),
        hours,
        items.len()
    ));

    if let Some(svg) = chart_svg {
        html.push_str("<div class=\"chart\">\n");
        html.push_str(svg);
        html.push_str("\n</div>\n");
    }

    if items.is_empty() {
        html.push_str("<p>No articles or papers were found for the selected time window.</p>\n");
        html.push_str("</body>\n</html>\n");
        return html;
    }

    for (group_name, group_items) in group_items(items) {
        html.push_str(&format!(
            "<h2>{} {}</h2>\n",
            section_icon(&group_name),
            esc(&group_name)
        ));

        for item in group_items {
            html.push_str("<div class=\"item\">\n");
            html.push_str(&format!(
                "<a href=\"{}\">{}</a>\n",
                esc(&item.url),
                esc(&item.title)
            ));
            html.push_str(&format!(
                "<div class=\"src\">{} &middot; {}</div>\n",
                esc(&item.source),
                esc(&fmt_dt(item.published_at))
            ));

            if let Some(summary) = non_empty(&item.summary) {
                html.push_str("<ul class=\"summary\">\n");
                for line in summary.trim().split('\n') {
                    let s = line.trim().trim_start_matches(['-', '*', '\u{2022}']).trim();
                    if !s.is_empty() {
                        html.push_str(&format!("<li>{}</li>\n", esc(s)));
                    }
                }
                html.push_str("</ul>\n");
            } else if let Some(snippet) = non_empty(&item.clean_snippet) {
                html.push_str(&format!("<p class=\"summary\">{}</p>\n", esc(snippet)));
            }

            html.push_str("</div>\n");
        }
    }

    html.push_str("</body>\n</html>\n");
    html
}

/// Render a hand-written SVG scatter plot of topic clusters.
///
/// Returns `None` when there are too few points to be meaningful.
pub fn render_cluster_chart_svg(points: &[ClusterPoint]) -> Option<String> {
    if points.len() < 2 {
        return None;
    }

    let (width, height) = (820.0_f64, 560.0_f64);
    let pad = 50.0_f64;

    let (mut min_x, mut max_x) = (f64::MAX, f64::MIN);
    let (mut min_y, mut max_y) = (f64::MAX, f64::MIN);
    for p in points {
        min_x = min_x.min(p.x);
        max_x = max_x.max(p.x);
        min_y = min_y.min(p.y);
        max_y = max_y.max(p.y);
    }
    // Avoid zero-width/height ranges.
    if (max_x - min_x).abs() < 1e-9 {
        min_x -= 1.0;
        max_x += 1.0;
    }
    if (max_y - min_y).abs() < 1e-9 {
        min_y -= 1.0;
        max_y += 1.0;
    }

    let sx = |x: f64| pad + (x - min_x) / (max_x - min_x) * (width - 2.0 * pad);
    // Invert y so larger values are higher on screen.
    let sy = |y: f64| height - pad - (y - min_y) / (max_y - min_y) * (height - 2.0 * pad);

    let mut svg = String::new();
    svg.push_str(&format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{}\" height=\"{}\" viewBox=\"0 0 {} {}\">\n",
        width, height, width, height
    ));
    svg.push_str(&format!(
        "<rect width=\"{}\" height=\"{}\" fill=\"#ffffff\"/>\n",
        width, height
    ));
    svg.push_str(&format!(
        "<text x=\"{}\" y=\"28\" font-family=\"sans-serif\" font-size=\"18\" font-weight=\"bold\" fill=\"#333\">Topic clusters (TF-IDF projection)</text>\n",
        pad
    ));

    // Points.
    let mut clusters_seen: Vec<usize> = Vec::new();
    for p in points {
        let color = CLUSTER_COLORS[p.cluster % CLUSTER_COLORS.len()];
        if !clusters_seen.contains(&p.cluster) {
            clusters_seen.push(p.cluster);
        }
        svg.push_str(&format!(
            "<circle cx=\"{:.1}\" cy=\"{:.1}\" r=\"6\" fill=\"{}\" fill-opacity=\"0.75\" stroke=\"#333\" stroke-width=\"0.5\"><title>{}</title></circle>\n",
            sx(p.x),
            sy(p.y),
            color,
            esc(&p.label)
        ));
    }

    // Legend.
    clusters_seen.sort_unstable();
    let mut ly = 50.0;
    for cluster in clusters_seen {
        let color = CLUSTER_COLORS[cluster % CLUSTER_COLORS.len()];
        svg.push_str(&format!(
            "<rect x=\"{}\" y=\"{:.1}\" width=\"12\" height=\"12\" fill=\"{}\"/>\n",
            width - 150.0,
            ly,
            color
        ));
        svg.push_str(&format!(
            "<text x=\"{}\" y=\"{:.1}\" font-family=\"sans-serif\" font-size=\"13\" fill=\"#333\">Cluster {}</text>\n",
            width - 132.0,
            ly + 11.0,
            cluster
        ));
        ly += 20.0;
    }

    svg.push_str("</svg>\n");
    Some(svg)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn item(title: &str, group: &str) -> NewsItem {
        let mut it = NewsItem::new(
            title,
            "https://example.com/x",
            Utc.with_ymd_and_hms(2025, 1, 1, 12, 0, 0).unwrap(),
            "OpenAI Blog",
        );
        it.priority_group = Some(group.to_string());
        it.summary = Some("- key point one\n- key point two".to_string());
        it
    }

    #[test]
    fn markdown_contains_header_and_link() {
        let items = vec![item("Big Launch", "Major Releases & Breakthroughs")];
        let out = format_markdown(&items, 24, Utc::now());
        assert!(out.contains("# Daily AI & Research Digest"));
        assert!(out.contains("[Big Launch](https://example.com/x)"));
        assert!(out.contains("key point one"));
    }

    #[test]
    fn text_handles_empty() {
        let out = format_text(&[], 24, Utc::now());
        assert!(out.contains("No articles or papers were found"));
    }

    #[test]
    fn html_escapes_and_embeds_chart() {
        let items = vec![item("A <b>title</b>", "Research & Benchmarks")];
        let out = format_html(&items, 24, Utc::now(), Some("<svg></svg>"));
        assert!(out.contains("&lt;b&gt;title&lt;/b&gt;"));
        assert!(out.contains("<svg></svg>"));
    }

    #[test]
    fn chart_needs_two_points() {
        let one = vec![ClusterPoint {
            x: 0.0,
            y: 0.0,
            cluster: 0,
            label: "a".to_string(),
        }];
        assert!(render_cluster_chart_svg(&one).is_none());

        let two = vec![
            ClusterPoint {
                x: 0.0,
                y: 0.0,
                cluster: 0,
                label: "a".to_string(),
            },
            ClusterPoint {
                x: 1.0,
                y: 1.0,
                cluster: 1,
                label: "b".to_string(),
            },
        ];
        assert!(render_cluster_chart_svg(&two).is_some());
    }
}
