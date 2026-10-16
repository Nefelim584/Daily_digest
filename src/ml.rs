//! TF-IDF vectorization, semantic deduplication, topic clustering, and
//! 2D projection for visualization (Phase 30).
//!
//! All pure, local, dependency-light numerics (no neural models, no BLAS).
//! Documents are short news snippets, so dimensionality is modest and the
//! n×n dual-PCA (classical MDS) projection is cheap.

use std::collections::{HashMap, HashSet};

use crate::models::NewsItem;

const KMEANS_ITERATIONS: usize = 25;
const POWER_ITERATIONS: usize = 100;

/// A minimal English stopword set (kept in sync conceptually with the summarizer).
const STOPWORDS: &[&str] = &[
    "the", "and", "for", "are", "but", "not", "you", "all", "any", "can", "her", "was", "one",
    "our", "out", "has", "had", "his", "him", "she", "its", "who", "get", "that", "this", "with",
    "from", "they", "will", "have", "been", "were", "their", "would", "about", "which", "there",
    "when", "what", "your", "than", "then", "them", "these", "those", "into", "over", "also",
    "such", "more", "most", "some", "only", "very", "just", "like", "between", "while",
];

/// A single L2-normalized sparse TF-IDF document vector (term index -> weight).
#[derive(Debug, Clone, Default)]
pub struct DocVector(pub HashMap<usize, f64>);

/// A fitted TF-IDF vocabulary and inverse-document-frequency table.
pub struct TfidfVectorizer {
    pub vocab: HashMap<String, usize>,
    pub idf: Vec<f64>,
}

impl TfidfVectorizer {
    /// Fit the vocabulary/IDF on a corpus and return the document vectors.
    pub fn fit(docs: &[String]) -> (Self, Vec<DocVector>) {
        let n = docs.len();
        let tokenized: Vec<Vec<String>> = docs.iter().map(|d| tokenize(d)).collect();

        let mut vocab: HashMap<String, usize> = HashMap::new();
        let mut df: Vec<usize> = Vec::new();

        for tokens in &tokenized {
            let unique: HashSet<&String> = tokens.iter().collect();
            for term in unique {
                let idx = match vocab.get(term) {
                    Some(&i) => i,
                    None => {
                        let i = df.len();
                        vocab.insert(term.clone(), i);
                        df.push(0);
                        i
                    }
                };
                df[idx] += 1;
            }
        }

        // Smoothed IDF (sklearn-style): ln((1+n)/(1+df)) + 1.
        let idf: Vec<f64> = df
            .iter()
            .map(|&d| ((1.0 + n as f64) / (1.0 + d as f64)).ln() + 1.0)
            .collect();

        let mut vectors = Vec::with_capacity(n);
        for tokens in &tokenized {
            let mut tf: HashMap<usize, f64> = HashMap::new();
            for term in tokens {
                if let Some(&i) = vocab.get(term) {
                    *tf.entry(i).or_insert(0.0) += 1.0;
                }
            }
            let mut vec: HashMap<usize, f64> =
                tf.into_iter().map(|(i, c)| (i, c * idf[i])).collect();
            l2_normalize(&mut vec);
            vectors.push(DocVector(vec));
        }

        (Self { vocab, idf }, vectors)
    }
}

/// Cosine similarity between two L2-normalized sparse vectors (= dot product).
pub fn cosine_sparse(a: &DocVector, b: &DocVector) -> f64 {
    dot_sparse(&a.0, &b.0)
}

/// Remove near-duplicate items by TF-IDF cosine similarity, keeping the first seen.
pub fn semantic_dedup(items: Vec<NewsItem>, threshold: f64) -> Vec<NewsItem> {
    if items.len() < 2 || threshold <= 0.0 {
        return items;
    }

    let docs: Vec<String> = items.iter().map(item_text).collect();
    let (_, vectors) = TfidfVectorizer::fit(&docs);

    let mut kept_indices: Vec<usize> = Vec::new();
    let mut mask = vec![false; items.len()];

    for i in 0..items.len() {
        let is_dup = kept_indices
            .iter()
            .any(|&k| cosine_sparse(&vectors[i], &vectors[k]) >= threshold);
        if !is_dup {
            kept_indices.push(i);
            mask[i] = true;
        }
    }

    items
        .into_iter()
        .zip(mask)
        .filter_map(|(item, keep)| if keep { Some(item) } else { None })
        .collect()
}

/// Assign a topic cluster id to each item via spherical k-means over TF-IDF.
/// Returns the number of clusters actually used.
pub fn assign_clusters(items: &mut [NewsItem], num_clusters: usize) -> usize {
    let n = items.len();
    if n == 0 {
        return 0;
    }

    let docs: Vec<String> = items.iter().map(item_text).collect();
    let (_, vectors) = TfidfVectorizer::fit(&docs);

    let k = resolve_k(num_clusters, n);
    if k <= 1 {
        for item in items.iter_mut() {
            item.cluster_id = Some(0);
        }
        return 1.min(n);
    }

    // Deterministic spread-out initialization.
    let mut centroids: Vec<HashMap<usize, f64>> =
        (0..k).map(|c| vectors[c * n / k].0.clone()).collect();
    let mut assignment = vec![0usize; n];

    for _ in 0..KMEANS_ITERATIONS {
        let mut changed = false;
        for (i, slot) in assignment.iter_mut().enumerate() {
            let mut best = 0usize;
            let mut best_sim = f64::MIN;
            for (c, centroid) in centroids.iter().enumerate() {
                let sim = dot_sparse(&vectors[i].0, centroid);
                if sim > best_sim {
                    best_sim = sim;
                    best = c;
                }
            }
            if *slot != best {
                changed = true;
            }
            *slot = best;
        }

        let mut new_centroids: Vec<HashMap<usize, f64>> = vec![HashMap::new(); k];
        let mut counts = vec![0usize; k];
        for (i, &c) in assignment.iter().enumerate() {
            counts[c] += 1;
            for (&idx, &v) in &vectors[i].0 {
                *new_centroids[c].entry(idx).or_insert(0.0) += v;
            }
        }
        for c in 0..k {
            if counts[c] == 0 {
                // Reseed an empty cluster to avoid collapse.
                new_centroids[c] = vectors[c % n].0.clone();
            } else {
                l2_normalize(&mut new_centroids[c]);
            }
        }
        centroids = new_centroids;

        if !changed {
            break;
        }
    }

    for (item, &c) in items.iter_mut().zip(assignment.iter()) {
        item.cluster_id = Some(c);
    }
    k
}

/// Assign topic clusters and compute 2D projection coordinates for visualization in one pass.
pub fn cluster_and_project(items: &mut [NewsItem], num_clusters: usize) -> (usize, Vec<(f64, f64)>) {
    if items.is_empty() {
        return (0, Vec::new());
    }
    let k = assign_clusters(items, num_clusters);
    let docs: Vec<String> = items.iter().map(item_text).collect();
    let (_, vectors) = TfidfVectorizer::fit(&docs);
    let coords = project_2d(&vectors);
    (k, coords)
}

/// Project document vectors to 2D coordinates via dual PCA / classical MDS.
pub fn project_2d(vectors: &[DocVector]) -> Vec<(f64, f64)> {
    let n = vectors.len();
    if n == 0 {
        return Vec::new();
    }
    if n == 1 {
        return vec![(0.0, 0.0)];
    }

    // Gram matrix of inner products.
    let mut gram = vec![vec![0.0f64; n]; n];
    for i in 0..n {
        for j in i..n {
            let d = dot_sparse(&vectors[i].0, &vectors[j].0);
            gram[i][j] = d;
            gram[j][i] = d;
        }
    }

    // Double-center to obtain the centered Gram matrix (dual PCA).
    let row_means: Vec<f64> = gram
        .iter()
        .map(|row| row.iter().sum::<f64>() / n as f64)
        .collect();
    let total_mean = row_means.iter().sum::<f64>() / n as f64;
    let mut b = vec![vec![0.0f64; n]; n];
    for i in 0..n {
        for j in 0..n {
            b[i][j] = gram[i][j] - row_means[i] - row_means[j] + total_mean;
        }
    }

    let (v1, l1) = power_iteration(&b);
    let b2 = deflate(&b, &v1, l1);
    let (v2, l2) = power_iteration(&b2);

    let s1 = l1.max(0.0).sqrt();
    let s2 = l2.max(0.0).sqrt();

    (0..n).map(|i| (v1[i] * s1, v2[i] * s2)).collect()
}

// ----------------------------------------------------------------------------
// Internal helpers
// ----------------------------------------------------------------------------

pub fn item_text(item: &NewsItem) -> String {
    let body = item.best_text().unwrap_or("");
    format!("{} {}", item.title, body)
}

fn tokenize(text: &str) -> Vec<String> {
    text.to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| w.len() > 2 && !STOPWORDS.contains(w))
        .map(|w| w.to_string())
        .collect()
}

fn l2_normalize(vec: &mut HashMap<usize, f64>) {
    let norm = vec.values().map(|v| v * v).sum::<f64>().sqrt();
    if norm > 0.0 {
        for v in vec.values_mut() {
            *v /= norm;
        }
    }
}

fn dot_sparse(a: &HashMap<usize, f64>, b: &HashMap<usize, f64>) -> f64 {
    let (small, large) = if a.len() <= b.len() { (a, b) } else { (b, a) };
    small
        .iter()
        .map(|(k, v)| v * large.get(k).copied().unwrap_or(0.0))
        .sum()
}

/// Resolve the number of clusters: explicit value, else heuristic ~sqrt(n/2).
fn resolve_k(num_clusters: usize, n: usize) -> usize {
    let k = if num_clusters > 0 {
        num_clusters
    } else {
        ((n as f64 / 2.0).sqrt().round() as usize).max(1)
    };
    k.clamp(1, n)
}

/// Dominant eigenvector/eigenvalue of a symmetric matrix via power iteration.
fn power_iteration(m: &[Vec<f64>]) -> (Vec<f64>, f64) {
    let n = m.len();
    let mut v = vec![1.0 / (n as f64).sqrt(); n];
    let mut eigenvalue = 0.0;

    for _ in 0..POWER_ITERATIONS {
        let mut w = vec![0.0; n];
        for i in 0..n {
            let mut s = 0.0;
            for j in 0..n {
                s += m[i][j] * v[j];
            }
            w[i] = s;
        }

        let norm = w.iter().map(|x| x * x).sum::<f64>().sqrt();
        if norm < 1e-12 {
            break;
        }
        for x in w.iter_mut() {
            *x /= norm;
        }

        let diff: f64 = w.iter().zip(v.iter()).map(|(a, b)| (a - b).abs()).sum();
        v = w;
        eigenvalue = norm;
        if diff < 1e-9 {
            break;
        }
    }

    (v, eigenvalue)
}

/// Deflate a symmetric matrix by removing the given eigen-component.
fn deflate(m: &[Vec<f64>], v: &[f64], lambda: f64) -> Vec<Vec<f64>> {
    let n = m.len();
    let mut result = m.to_vec();
    for i in 0..n {
        for j in 0..n {
            result[i][j] -= lambda * v[i] * v[j];
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    fn item(title: &str, body: &str) -> NewsItem {
        let mut it = NewsItem::new(title, "https://example.com", Utc::now(), "Test");
        it.clean_snippet = Some(body.to_string());
        it
    }

    #[test]
    fn tfidf_similar_docs_have_high_cosine() {
        let docs = vec![
            "openai releases a new language model".to_string(),
            "openai announces a new language model today".to_string(),
            "weather is sunny and warm outside".to_string(),
        ];
        let (_, vectors) = TfidfVectorizer::fit(&docs);
        let sim_related = cosine_sparse(&vectors[0], &vectors[1]);
        let sim_unrelated = cosine_sparse(&vectors[0], &vectors[2]);
        assert!(sim_related > sim_unrelated);
    }

    #[test]
    fn dedup_removes_near_duplicates() {
        let items = vec![
            item("OpenAI releases new model", "a brand new language model from openai"),
            item("OpenAI releases a new model", "a brand new language model by openai"),
            item("Weather report", "sunny and warm across the region today"),
        ];
        let deduped = semantic_dedup(items, 0.6);
        assert_eq!(deduped.len(), 2);
    }

    #[test]
    fn clusters_are_assigned() {
        let mut items = vec![
            item("Model A", "deep learning transformer model training"),
            item("Model B", "transformer model training on large datasets"),
            item("Cooking", "recipe for pasta with tomato sauce"),
            item("Cooking 2", "recipe for pizza with cheese and tomato"),
        ];
        let k = assign_clusters(&mut items, 2);
        assert_eq!(k, 2);
        assert!(items.iter().all(|i| i.cluster_id.is_some()));
    }

    #[test]
    fn projection_returns_coord_per_doc() {
        let docs = vec![
            "alpha beta gamma".to_string(),
            "beta gamma delta".to_string(),
            "epsilon zeta eta".to_string(),
        ];
        let (_, vectors) = TfidfVectorizer::fit(&docs);
        let coords = project_2d(&vectors);
        assert_eq!(coords.len(), 3);
    }
}
