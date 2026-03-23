//! Benchmark test for batch reranker inference vs sequential inference.
//! Demonstrates the performance improvement from batching.

#[cfg(test)]
mod reranker_batch_tests {
    use std::time::Instant;
    use voidm_core::reranker::CrossEncoderReranker;

    #[tokio::test]
    #[ignore] // Run with: cargo test --test reranker_batch_benchmark -- --ignored
    async fn test_batch_reranking_performance() {
        // Load model once
        let reranker = CrossEncoderReranker::load("ms-marco-TinyBERT")
            .await
            .expect("Should load reranker model");

        let query = "What is machine learning?";

        // Sample documents
        let documents = vec![
            "Machine learning is a subset of artificial intelligence that enables systems to learn from data.",
            "Deep learning uses neural networks with multiple layers to process information.",
            "Natural language processing helps computers understand human language.",
            "Computer vision enables machines to interpret visual information from images.",
            "Supervised learning requires labeled training data to train models.",
            "Unsupervised learning finds patterns in unlabeled data.",
            "Reinforcement learning uses rewards and penalties to train agents.",
            "Neural networks are inspired by biological neurons in the brain.",
            "Python is a popular programming language for machine learning.",
            "TensorFlow and PyTorch are common deep learning frameworks.",
        ];

        let doc_refs: Vec<&str> = documents.iter().map(|s| s.as_ref()).collect();

        // Benchmark batch reranking
        let start = Instant::now();
        let reranked_results = reranker
            .rerank(query, &doc_refs)
            .expect("Reranking should succeed");
        let batch_duration = start.elapsed();

        // Verify results
        assert_eq!(
            reranked_results.len(),
            doc_refs.len(),
            "Should return scores for all documents"
        );

        // All scores should be in [0, 1] range
        for result in &reranked_results {
            assert!(
                result.score >= 0.0 && result.score <= 1.0,
                "Score {} should be in [0,1] range",
                result.score
            );
        }

        // Verify sorted by descending score
        for i in 0..reranked_results.len() - 1 {
            assert!(
                reranked_results[i].score >= reranked_results[i + 1].score,
                "Results should be sorted by score descending"
            );
        }

        println!("\n📊 Batch Reranking Performance Test");
        println!("   Documents: {}", doc_refs.len());
        println!("   Model: {}", reranker.model_name());
        println!(
            "   Batch duration: {:.2}ms",
            batch_duration.as_secs_f64() * 1000.0
        );
        println!(
            "   Per-document average: {:.2}ms",
            (batch_duration.as_secs_f64() * 1000.0) / doc_refs.len() as f64
        );

        println!("\n   Top 3 Results:");
        for (rank, result) in reranked_results.iter().take(3).enumerate() {
            let doc = doc_refs[result.index];
            println!(
                "   #{} (score: {:.4}) {:.60}...",
                rank + 1,
                result.score,
                doc
            );
        }

        // Expected: < 100ms for 10 documents on modern hardware
        // If this exceeds 500ms, something is wrong with batch processing
        assert!(
            batch_duration.as_millis() < 500,
            "Batch reranking should be fast (< 500ms for 10 docs, got {:.0}ms)",
            batch_duration.as_millis()
        );
    }

    #[tokio::test]
    #[ignore] // Run with: cargo test --test reranker_batch_benchmark -- --ignored
    async fn test_batch_consistency() {
        // Test that batch processing gives same results as sequential
        let reranker = CrossEncoderReranker::load("ms-marco-TinyBERT")
            .await
            .expect("Should load reranker model");

        let query = "artificial intelligence";
        let documents = vec![
            "AI is transforming industries.",
            "Machine learning enables automation.",
            "Neural networks process data.",
        ];
        let doc_refs: Vec<&str> = documents.iter().map(|s| s.as_ref()).collect();

        // Get batch results
        let batch_results = reranker
            .rerank(query, &doc_refs)
            .expect("Batch reranking should succeed");

        // Get individual results (sequential)
        let mut individual_results = Vec::new();
        for (idx, doc) in doc_refs.iter().enumerate() {
            let score = reranker
                .score(query, doc)
                .expect("Individual scoring should succeed");
            individual_results.push((idx, score));
        }
        individual_results
            .sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

        // Compare results
        println!("\n✅ Batch vs Sequential Consistency Test");
        for (batch_result, (seq_idx, seq_score)) in
            batch_results.iter().zip(individual_results.iter())
        {
            println!(
                "   Batch [idx={}, score={:.4}] vs Sequential [idx={}, score={:.4}]",
                batch_result.index, batch_result.score, seq_idx, seq_score
            );

            // Scores should match closely (allowing small floating point differences)
            assert!(
                (batch_result.score - seq_score).abs() < 0.0001,
                "Batch and sequential scores should match: {:.6} vs {:.6}",
                batch_result.score,
                seq_score
            );
        }

        println!("   ✓ Batch and sequential results are consistent!");
    }
}
