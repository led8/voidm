//! Reciprocal Rank Fusion (RRF) - combine multiple search result sets
//!
//! RRF merges results from multiple search signals (vector, BM25, fuzzy, etc.)
//! using the formula: RRF(d) = Σ 1/(k + rank(d))
//!
//! Benefits:
//! - Combines diverse signals without manual weighting
//! - Preserves high-confidence matches
//! - Prevents any single signal from dominating
//! - Produces better overall ranking than weighted averaging
//!
//! Formula: score = Σ 1/(k + rank)
//! where k is the RRF constant (typically 60 for 3 result sets)

use std::collections::HashMap;

/// Configuration for RRF fusion
#[derive(Debug, Clone)]
pub struct RRFConfig {
    /// RRF constant k (higher = more conservative, ~60 recommended)
    pub k: u32,
    /// Apply top-rank bonus for high-confidence matches
    pub top_rank_bonus: bool,
    /// Bonus factor for rank 1 results
    pub rank_1_bonus: f32,
    /// Bonus factor for ranks 2-3 results
    pub rank_2_3_bonus: f32,
}

impl Default for RRFConfig {
    fn default() -> Self {
        Self {
            k: 60,
            top_rank_bonus: true,
            rank_1_bonus: 0.05,
            rank_2_3_bonus: 0.02,
        }
    }
}

/// RRF result with source tracking
#[derive(Debug, Clone)]
pub struct RRFResult {
    pub id: String,
    pub rrf_score: f32,
    /// Which signals contributed (e.g., ["vector", "bm25"])
    pub signals: Vec<String>,
    /// Ranks in each signal (for debugging)
    pub ranks: HashMap<String, u32>,
}

/// Reciprocal Rank Fusion engine
pub struct RRFFusion {
    config: RRFConfig,
}

impl RRFFusion {
    /// Create new RRF fusion with config
    pub fn new(config: RRFConfig) -> Self {
        Self { config }
    }

    /// Create new RRF with default config
    pub fn default() -> Self {
        Self::new(RRFConfig::default())
    }

    /// Fuse multiple ranked result sets
    ///
    /// Each entry is a (signal_name, [(id, score), ...]) tuple.
    /// The score is normalized to [0, 1] before RRF.
    pub fn fuse(&self, signals: Vec<(&str, Vec<(String, f32)>)>) -> Vec<RRFResult> {
        let mut scores: HashMap<String, f32> = HashMap::new();
        let mut metadata: HashMap<String, (Vec<String>, HashMap<String, u32>)> = HashMap::new();

        for (signal_name, results) in signals {
            for (rank, (id, _score)) in results.into_iter().enumerate() {
                let rank = (rank + 1) as u32;
                let rrf_contrib = 1.0 / (self.config.k + rank) as f32;

                *scores.entry(id.clone()).or_default() += rrf_contrib;

                let (signals, ranks) = metadata
                    .entry(id.clone())
                    .or_insert_with(|| (Vec::new(), HashMap::new()));

                if !signals.contains(&signal_name.to_string()) {
                    signals.push(signal_name.to_string());
                }
                ranks.insert(signal_name.to_string(), rank);
            }
        }

        // Apply top-rank bonus for high-confidence matches
        if self.config.top_rank_bonus {
            for (id, (_, ranks)) in &metadata {
                let mut bonus = 0.0;
                for rank in ranks.values() {
                    match rank {
                        1 => bonus += self.config.rank_1_bonus,
                        2 | 3 => bonus += self.config.rank_2_3_bonus,
                        _ => {}
                    }
                }
                if bonus > 0.0 {
                    *scores.get_mut(id).unwrap_or(&mut 0.0) += bonus;
                }
            }
        }

        // Build results
        let mut results: Vec<RRFResult> = scores
            .into_iter()
            .map(|(id, rrf_score)| {
                let (signals, ranks) = metadata.remove(&id).unwrap_or_default();
                RRFResult {
                    id,
                    rrf_score,
                    signals,
                    ranks,
                }
            })
            .collect();

        // Sort by RRF score descending
        results.sort_by(|a, b| {
            b.rrf_score
                .partial_cmp(&a.rrf_score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        results
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rrf_basic() {
        let rrf = RRFFusion::default();

        let signal1 = vec![
            ("doc1".to_string(), 0.9),
            ("doc2".to_string(), 0.8),
            ("doc3".to_string(), 0.7),
        ];

        let signal2 = vec![
            ("doc2".to_string(), 0.95),
            ("doc1".to_string(), 0.85),
            ("doc4".to_string(), 0.75),
        ];

        let results = rrf.fuse(vec![("vector", signal1), ("bm25", signal2)]);

        assert!(!results.is_empty());
        // Both doc1 and doc2 rank highly (they trade positions: 1st->2nd and 2nd->1st)
        // Just verify they're in top positions
        assert!(results.len() >= 2);
        assert!(vec!["doc1", "doc2"].contains(&results[0].id.as_str()));
        assert!(vec!["doc1", "doc2"].contains(&results[1].id.as_str()));
    }

    #[test]
    fn test_rrf_preserves_high_confidence() {
        let rrf = RRFFusion::new(RRFConfig {
            top_rank_bonus: true,
            rank_1_bonus: 0.05,
            ..Default::default()
        });

        let signal1 = vec![("doc1".to_string(), 1.0)];
        let signal2 = vec![("doc2".to_string(), 0.9), ("doc1".to_string(), 0.8)];

        let results = rrf.fuse(vec![("vector", signal1), ("bm25", signal2)]);

        // doc1 should rank first despite being rank 2 in signal2, due to rank 1 bonus
        assert_eq!(results[0].id, "doc1");
    }
}
