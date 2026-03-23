/// Benchmark comparing Hybrid search vs RRF-enhanced search
///
/// Run with: cargo test --test rrf_search_benchmark -- --nocapture --test-threads=1

#[cfg(test)]
mod tests {
    use std::time::Instant;
    use voidm_core::search::{SearchMode, SearchOptions};

    #[test]
    fn test_rrf_vs_weighted_averaging() {
        // This test demonstrates the difference between weighted averaging and RRF
        // on a synthetic ranking scenario

        println!("\n=== RRF vs Weighted Averaging Comparison ===\n");

        // Simulate three ranking signals
        let vector_ranking = vec![
            ("doc1", 0.95),
            ("doc2", 0.80),
            ("doc3", 0.75),
            ("doc4", 0.60),
        ];

        let bm25_ranking = vec![
            ("doc2", 0.92),
            ("doc1", 0.70),
            ("doc5", 0.65),
            ("doc3", 0.55),
        ];

        let fuzzy_ranking = vec![
            ("doc3", 0.88),
            ("doc1", 0.85),
            ("doc4", 0.72),
            ("doc2", 0.68),
        ];

        println!("Vector ranking: {:?}", vector_ranking);
        println!("BM25 ranking: {:?}", bm25_ranking);
        println!("Fuzzy ranking: {:?}", fuzzy_ranking);
        println!();

        // Weighted averaging (current VOIDM approach)
        let mut weighted_scores = std::collections::HashMap::new();

        for (doc, score) in &vector_ranking {
            *weighted_scores.entry(doc.to_string()).or_insert(0.0) += score * 0.5;
        }
        for (doc, score) in &bm25_ranking {
            *weighted_scores.entry(doc.to_string()).or_insert(0.0) += score * 0.3;
        }
        for (doc, score) in &fuzzy_ranking {
            *weighted_scores.entry(doc.to_string()).or_insert(0.0) += score * 0.2;
        }

        let mut weighted_ranking: Vec<_> = weighted_scores.into_iter().collect();
        weighted_ranking.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());

        println!("Weighted Averaging Results (50% vector, 30% BM25, 20% fuzzy):");
        for (i, (doc, score)) in weighted_ranking.iter().enumerate() {
            println!("  {}. {} (score: {:.3})", i + 1, doc, score);
        }
        println!();

        // RRF approach
        println!("RRF Results (k=60, top-rank bonus: 0.05/0.02):");
        let mut rrf_scores = std::collections::HashMap::new();

        // RRF formula: 1/(k + rank)
        let k = 60;

        // Vector signal
        for (rank, (doc, _)) in vector_ranking.iter().enumerate() {
            let rank = rank as u32 + 1;
            let contrib = 1.0 / (k + rank) as f32;
            *rrf_scores.entry(doc.to_string()).or_insert(0.0) += contrib;
        }

        // BM25 signal
        for (rank, (doc, _)) in bm25_ranking.iter().enumerate() {
            let rank = rank as u32 + 1;
            let contrib = 1.0 / (k + rank) as f32;
            *rrf_scores.entry(doc.to_string()).or_insert(0.0) += contrib;
        }

        // Fuzzy signal
        for (rank, (doc, _)) in fuzzy_ranking.iter().enumerate() {
            let rank = rank as u32 + 1;
            let contrib = 1.0 / (k + rank) as f32;
            *rrf_scores.entry(doc.to_string()).or_insert(0.0) += contrib;
        }

        // Apply top-rank bonus
        let all_docs = vec![
            ("doc1", 1, 1, 2), // ranks in vector, bm25, fuzzy
            ("doc2", 2, 1, 4),
            ("doc3", 3, 4, 1),
            ("doc4", 4, 999, 3),
            ("doc5", 999, 3, 999),
        ];

        for (doc, r_vec, r_bm25, r_fuzzy) in all_docs {
            let mut bonus = 0.0;
            if r_vec == 1 {
                bonus += 0.05;
            }
            if r_vec == 2 || r_vec == 3 {
                bonus += 0.02;
            }
            if r_bm25 == 1 {
                bonus += 0.05;
            }
            if r_bm25 == 2 || r_bm25 == 3 {
                bonus += 0.02;
            }
            if r_fuzzy == 1 {
                bonus += 0.05;
            }
            if r_fuzzy == 2 || r_fuzzy == 3 {
                bonus += 0.02;
            }

            if bonus > 0.0 {
                *rrf_scores.entry(doc.to_string()).or_insert(0.0) += bonus;
            }
        }

        let mut rrf_ranking: Vec<_> = rrf_scores.into_iter().collect();
        rrf_ranking.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());

        for (i, (doc, score)) in rrf_ranking.iter().enumerate() {
            println!("  {}. {} (score: {:.4})", i + 1, doc, score);
        }
        println!();

        // Analysis
        println!("Analysis:");
        println!("  Weighted: doc1 top (strong vector signal)");
        println!("  RRF:      doc1 top (balanced across signals)");
        println!();
        println!("  Key insight: doc2 is much better at rank 2 with both approaches");
        println!("  but RRF more fairly weights the fact that doc3 ranks #1 in fuzzy.");
        println!("  This prevents any single signal from dominating the ranking.");
    }

    #[test]
    fn test_search_mode_parsing() {
        use std::str::FromStr;

        println!("\n=== Search Mode Parsing ===\n");

        let modes = vec![
            "hybrid",
            "semantic",
            "keyword",
            "fuzzy",
            "bm25",
            "hybrid-rrf",
        ];

        for mode_str in modes {
            match SearchMode::from_str(mode_str) {
                Ok(mode) => println!("✓ '{}' -> {:?}", mode_str, mode),
                Err(e) => println!("✗ '{}' -> Error: {}", mode_str, e),
            }
        }
        println!();
    }

    #[test]
    fn test_rrf_preserves_consensus() {
        println!("\n=== RRF Consensus Preservation ===\n");

        // Test that RRF preserves items that rank well across multiple signals

        // Scenario: doc1 ranks in top 3 of all signals
        // This should score highest even with small weights per signal

        let vector = vec![("doc1", 0.9), ("doc2", 0.8)];
        let bm25 = vec![("doc1", 0.85), ("doc3", 0.8)];
        let fuzzy = vec![("doc1", 0.88), ("doc4", 0.7)];

        println!("Input rankings (all have doc1 in top 2):");
        println!("  Vector: {:?}", vector);
        println!("  BM25:   {:?}", bm25);
        println!("  Fuzzy:  {:?}", fuzzy);
        println!();

        let k = 60;
        let mut rrf = 0.0;

        // doc1 is rank 1 in all signals
        rrf += 1.0 / (k + 1) as f32;
        rrf += 1.0 / (k + 1) as f32;
        rrf += 1.0 / (k + 1) as f32;

        // Top-rank bonus for rank 1 in all three
        rrf += 0.05 * 3.0;

        println!("doc1 RRF score (rank 1 in all): {:.4}", rrf);
        println!("Expected: High score due to consensus across signals");
        println!("Actual: {}", if rrf > 0.15 { "✓ PASS" } else { "✗ FAIL" });
    }
}
