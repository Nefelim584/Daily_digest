//! Local extractive summarization via TextRank (Phase 29).
//!
//! Replaces the previous Mistral API summarizer. Builds a graph of sentences
//! weighted by cosine similarity over bag-of-words vectors, ranks them with a
//! PageRank-style iteration, and keeps the top-N sentences (in original order)
//! as bullet points. Fully local — no network, no model weights.

use std::collections::HashMap;

use crate::config::SummarizationConfig;
use crate::models::NewsItem;

const DAMPING: f64 = 0.85;
const ITERATIONS: usize = 40;
const CONVERGENCE_EPS: f64 = 1e-6;

/// A minimal English stopword set to reduce noise in similarity scoring.
const STOPWORDS: &[&str] = &[
    "the", "and", "for", "are", "but", "not", "you", "all", "any", "can", "her", "was", "one",
    "our", "out", "has", "had", "his", "him", "she", "its", "who", "get", "that", "this", "with",
    "from", "they", "will", "have", "been", "were", "their", "would", "about", "which", "there",
    "when", "what", "your", "than", "then", "them", "these", "those", "into", "over", "also",
    "such", "more", "most", "some", "only", "very", "just", "like", "between", "while",
];

/// Extractive summarizer using the TextRank algorithm.
pub struct ExtractiveSummarizer {
    config: SummarizationConfig,
}

impl ExtractiveSummarizer {
    pub fn new(config: SummarizationConfig) -> Self {
        Self { config }
    }

    /// Summarize a single item in place (sets `item.summary` on success).
    pub fn summarize_item(&self, item: &mut NewsItem) {
        if !self.config.enabled {
            return;
        }

        let text = match item.best_text() {
            Some(text) if !text.trim().is_empty() => text.to_string(),
            _ => return,
        };

        let top = self.textrank(&text, self.config.max_sentences.max(1));
        if top.is_empty() {
            return;
        }

        let bullets = top
            .iter()
            .map(|s| format!("- {}", s))
            .collect::<Vec<_>>()
            .join("\n");
        item.summary = Some(bullets);
    }

    /// Summarize a collection of items.
    pub fn summarize_items(&self, mut items: Vec<NewsItem>) -> Vec<NewsItem> {
        for item in &mut items {
            self.summarize_item(item);
        }
        items
    }

    /// Return the top-`top_n` ranked sentences, preserving original order.
    fn textrank(&self, text: &str, top_n: usize) -> Vec<String> {
        let sentences = split_sentences(text);
        if sentences.len() <= top_n {
            return sentences;
        }

        let token_sets: Vec<HashMap<String, usize>> =
            sentences.iter().map(|s| term_counts(&tokenize(s))).collect();

        let n = sentences.len();
        let mut sim = vec![vec![0.0f64; n]; n];
        for i in 0..n {
            for j in (i + 1)..n {
                let score = cosine(&token_sets[i], &token_sets[j]);
                sim[i][j] = score;
                sim[j][i] = score;
            }
        }

        let scores = pagerank(&sim);

        // Rank sentence indices by score (desc), take top_n, then restore order.
        let mut ranked: Vec<usize> = (0..n).collect();
        ranked.sort_by(|&a, &b| {
            scores[b]
                .partial_cmp(&scores[a])
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        ranked.truncate(top_n);
        ranked.sort_unstable();

        ranked.into_iter().map(|i| sentences[i].clone()).collect()
    }
}

/// Split text into sentences on `.`, `!`, `?` (and newlines) boundaries.
fn split_sentences(text: &str) -> Vec<String> {
    let chars: Vec<char> = text.chars().collect();
    let mut sentences = Vec::new();
    let mut current = String::new();

    for (i, &c) in chars.iter().enumerate() {
        if c == '\n' {
            let s = current.trim().to_string();
            if !s.is_empty() {
                sentences.push(s);
            }
            current.clear();
            continue;
        }

        current.push(c);

        if matches!(c, '.' | '!' | '?') {
            let next_is_boundary = chars.get(i + 1).is_none_or(|n| n.is_whitespace());
            if next_is_boundary {
                let s = current.trim().to_string();
                if !s.is_empty() {
                    sentences.push(s);
                }
                current.clear();
            }
        }
    }

    let tail = current.trim().to_string();
    if !tail.is_empty() {
        sentences.push(tail);
    }

    sentences
}

/// Tokenize into lowercase alphanumeric words, dropping short tokens and stopwords.
fn tokenize(sentence: &str) -> Vec<String> {
    sentence
        .to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| w.len() > 2 && !STOPWORDS.contains(w))
        .map(|w| w.to_string())
        .collect()
}

/// Build a term-frequency map from tokens.
fn term_counts(tokens: &[String]) -> HashMap<String, usize> {
    let mut counts = HashMap::new();
    for token in tokens {
        *counts.entry(token.clone()).or_insert(0) += 1;
    }
    counts
}

/// Cosine similarity between two bag-of-words vectors.
fn cosine(a: &HashMap<String, usize>, b: &HashMap<String, usize>) -> f64 {
    if a.is_empty() || b.is_empty() {
        return 0.0;
    }

    let dot: f64 = a
        .iter()
        .map(|(k, &va)| va as f64 * *b.get(k).unwrap_or(&0) as f64)
        .sum();
    if dot == 0.0 {
        return 0.0;
    }

    let norm_a: f64 = a.values().map(|&v| (v as f64).powi(2)).sum::<f64>().sqrt();
    let norm_b: f64 = b.values().map(|&v| (v as f64).powi(2)).sum::<f64>().sqrt();
    if norm_a == 0.0 || norm_b == 0.0 {
        return 0.0;
    }

    dot / (norm_a * norm_b)
}

/// Weighted PageRank over a symmetric similarity matrix.
fn pagerank(sim: &[Vec<f64>]) -> Vec<f64> {
    let n = sim.len();
    if n == 0 {
        return Vec::new();
    }

    let out_sums: Vec<f64> = (0..n).map(|j| sim[j].iter().sum()).collect();
    let mut scores = vec![1.0 / n as f64; n];

    for _ in 0..ITERATIONS {
        let mut next = vec![0.0; n];
        for (i, slot) in next.iter_mut().enumerate() {
            let mut rank_sum = 0.0;
            for j in 0..n {
                if j != i && out_sums[j] > 0.0 {
                    rank_sum += (sim[j][i] / out_sums[j]) * scores[j];
                }
            }
            *slot = (1.0 - DAMPING) + DAMPING * rank_sum;
        }

        let delta: f64 = next
            .iter()
            .zip(scores.iter())
            .map(|(a, b)| (a - b).abs())
            .sum();
        scores = next;
        if delta < CONVERGENCE_EPS {
            break;
        }
    }

    scores
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_sentences() {
        let s = split_sentences("Hello world. This is AI! Really? Yes.");
        assert_eq!(s.len(), 4);
    }

    #[test]
    fn short_text_returns_all_sentences() {
        let cfg = SummarizationConfig {
            enabled: true,
            max_sentences: 3,
        };
        let summarizer = ExtractiveSummarizer::new(cfg);
        let out = summarizer.textrank("Only one sentence here.", 3);
        assert_eq!(out.len(), 1);
    }

    #[test]
    fn selects_top_sentences_in_order() {
        let cfg = SummarizationConfig {
            enabled: true,
            max_sentences: 2,
        };
        let summarizer = ExtractiveSummarizer::new(cfg);
        let text = "The model achieves state of the art results on benchmarks. \
             Weather today is sunny and warm. \
             The model was trained on a large dataset of benchmarks and tasks. \
             Cats are nice animals.";
        let out = summarizer.textrank(text, 2);
        assert_eq!(out.len(), 2);
        // The two model/benchmark sentences should rank highest and keep order.
        assert!(out[0].contains("model"));
        assert!(out[1].contains("model"));
    }

    #[test]
    fn disabled_summarizer_does_nothing() {
        let cfg = SummarizationConfig {
            enabled: false,
            max_sentences: 3,
        };
        let summarizer = ExtractiveSummarizer::new(cfg);
        let mut item = NewsItem::new("T", "u", chrono::Utc::now(), "s");
        item.clean_snippet = Some("A. B. C. D. E.".to_string());
        summarizer.summarize_item(&mut item);
        assert!(item.summary.is_none());
    }
}
