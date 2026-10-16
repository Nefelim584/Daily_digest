//! Regex-based priority classifier, sorting, and grouping (Phase 28).
//!
//! Ported from the Python `PriorityClassifier`. Rules are sorted by priority
//! (descending) and matched against item titles; the first match wins.

use regex::Regex;

use crate::config::PriorityRule;
use crate::models::NewsItem;

const DEFAULT_GROUP: &str = "General News & Updates";
const DEFAULT_SCORE: i32 = 10;

struct CompiledRule {
    name: String,
    priority: i32,
    regex: Regex,
}

/// Classifies news items into priority tiers using regular expressions.
pub struct PriorityClassifier {
    rules: Vec<CompiledRule>,
}

impl PriorityClassifier {
    /// Build a classifier from configured rules (sorted by priority descending).
    pub fn new(rules: &[PriorityRule]) -> Self {
        let mut sorted = rules.to_vec();
        sorted.sort_by_key(|a| std::cmp::Reverse(a.priority));

        let mut compiled = Vec::new();
        for rule in sorted {
            match Regex::new(&rule.regex) {
                Ok(regex) => compiled.push(CompiledRule {
                    name: rule.name,
                    priority: rule.priority,
                    regex,
                }),
                Err(err) => tracing::error!(
                    "Invalid regex in priority rule '{}' ({}): {}",
                    rule.name,
                    rule.regex,
                    err
                ),
            }
        }

        Self { rules: compiled }
    }

    /// Assign a priority score and group name to a single item (first match wins).
    pub fn classify_item(&self, item: &mut NewsItem) {
        for rule in &self.rules {
            if rule.regex.is_match(&item.title) {
                item.priority_score = rule.priority;
                item.priority_group = Some(rule.name.clone());
                return;
            }
        }
        item.priority_score = DEFAULT_SCORE;
        item.priority_group = Some(DEFAULT_GROUP.to_string());
    }

    /// Classify a collection of items.
    pub fn classify_items(&self, mut items: Vec<NewsItem>) -> Vec<NewsItem> {
        for item in &mut items {
            self.classify_item(item);
        }
        items
    }

    /// Sort items deterministically: priority desc, publication time desc, URL desc.
    pub fn sort_items_by_priority(&self, mut items: Vec<NewsItem>) -> Vec<NewsItem> {
        items.sort_by(|a, b| {
            b.priority_score
                .cmp(&a.priority_score)
                .then(b.published_at.cmp(&a.published_at))
                .then(b.url.cmp(&a.url))
        });
        items
    }

    /// Group classified items by priority category, preserving priority order
    /// and dropping empty groups.
    pub fn group_by_priority(&self, items: Vec<NewsItem>) -> Vec<(String, Vec<NewsItem>)> {
        let mut groups: Vec<(String, Vec<NewsItem>)> = Vec::new();
        for rule in &self.rules {
            if !groups.iter().any(|(name, _)| name == &rule.name) {
                groups.push((rule.name.clone(), Vec::new()));
            }
        }
        if !groups.iter().any(|(name, _)| name == DEFAULT_GROUP) {
            groups.push((DEFAULT_GROUP.to_string(), Vec::new()));
        }

        for item in items {
            let group = item
                .priority_group
                .clone()
                .unwrap_or_else(|| DEFAULT_GROUP.to_string());
            if let Some(entry) = groups.iter_mut().find(|(name, _)| name == &group) {
                entry.1.push(item);
            } else {
                groups.push((group, vec![item]));
            }
        }

        groups.into_iter().filter(|(_, v)| !v.is_empty()).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    fn rules() -> Vec<PriorityRule> {
        vec![
            PriorityRule {
                name: "Major Releases & Breakthroughs".to_string(),
                priority: 100,
                regex: r"(?i)(release|introducing|launch|gpt-\d)".to_string(),
            },
            PriorityRule {
                name: "Research & Benchmarks".to_string(),
                priority: 50,
                regex: r"(?i)(paper|benchmark|dataset)".to_string(),
            },
            PriorityRule {
                name: "General News & Updates".to_string(),
                priority: 10,
                regex: r".*".to_string(),
            },
        ]
    }

    fn item(title: &str) -> NewsItem {
        NewsItem::new(title, "https://example.com", Utc::now(), "Test")
    }

    #[test]
    fn classifies_by_first_matching_rule() {
        let classifier = PriorityClassifier::new(&rules());

        let mut release = item("Introducing GPT-5");
        classifier.classify_item(&mut release);
        assert_eq!(release.priority_score, 100);
        assert_eq!(
            release.priority_group.as_deref(),
            Some("Major Releases & Breakthroughs")
        );

        let mut paper = item("A new benchmark for reasoning");
        classifier.classify_item(&mut paper);
        assert_eq!(paper.priority_score, 50);

        let mut other = item("Some unrelated update");
        classifier.classify_item(&mut other);
        assert_eq!(other.priority_group.as_deref(), Some("General News & Updates"));
    }

    #[test]
    fn groups_preserve_priority_order_and_drop_empty() {
        let classifier = PriorityClassifier::new(&rules());
        let items = classifier.classify_items(vec![
            item("Launch of a model"),
            item("General chatter"),
        ]);
        let groups = classifier.group_by_priority(items);
        // Research group is empty and must be dropped.
        assert_eq!(groups.len(), 2);
        assert_eq!(groups[0].0, "Major Releases & Breakthroughs");
        assert_eq!(groups[1].0, "General News & Updates");
    }
}
