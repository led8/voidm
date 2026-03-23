//! Benchmark: Two-Pass Algorithm (Fuzzy + Semantic)
//!
//! This benchmark simulates the actual two-pass approach:
//! 1. Fast fuzzy pass filters candidates
//! 2. Semantic pass only on fuzzy matches
//!
//! Run with: cargo test --test semantic_dedup_twopass_benchmark -- --nocapture --ignored

use std::time::Instant;
use voidm_core::semantic_dedup;

#[derive(Debug, Clone)]
struct ConceptPair {
    name1: &'static str,
    name2: &'static str,
    should_merge: bool,
    #[allow(dead_code)]
    category: &'static str,
}

fn get_large_concept_set() -> Vec<ConceptPair> {
    // Simulate a larger database with 100 concepts = 4950 pairs
    // We'll use a representative sample
    vec![
        // RELATED (should merge)
        ConceptPair {
            name1: "Docker",
            name2: "Dockerfile",
            should_merge: true,
            category: "r",
        },
        ConceptPair {
            name1: "API",
            name2: "API Design",
            should_merge: true,
            category: "r",
        },
        ConceptPair {
            name1: "Configuration",
            name2: "Config",
            should_merge: true,
            category: "r",
        },
        ConceptPair {
            name1: "Authentication",
            name2: "Authorization",
            should_merge: true,
            category: "r",
        },
        ConceptPair {
            name1: "Database",
            name2: "DB",
            should_merge: true,
            category: "r",
        },
        ConceptPair {
            name1: "HTTP",
            name2: "HTTPS",
            should_merge: true,
            category: "r",
        },
        ConceptPair {
            name1: "JSON",
            name2: "JSON Schema",
            should_merge: true,
            category: "r",
        },
        ConceptPair {
            name1: "REST",
            name2: "RESTful",
            should_merge: true,
            category: "r",
        },
        ConceptPair {
            name1: "Testing",
            name2: "Unit Test",
            should_merge: true,
            category: "r",
        },
        ConceptPair {
            name1: "Kubernetes",
            name2: "K8s",
            should_merge: true,
            category: "r",
        },
        // DIFFERENT (shouldn't merge)
        ConceptPair {
            name1: "Docker",
            name2: "Python",
            should_merge: false,
            category: "d",
        },
        ConceptPair {
            name1: "API",
            name2: "Database",
            should_merge: false,
            category: "d",
        },
        ConceptPair {
            name1: "Docker",
            name2: "Cooking",
            should_merge: false,
            category: "d",
        },
        ConceptPair {
            name1: "Authentication",
            name2: "Food",
            should_merge: false,
            category: "d",
        },
        ConceptPair {
            name1: "HTTP",
            name2: "Pizza",
            should_merge: false,
            category: "d",
        },
        ConceptPair {
            name1: "Testing",
            name2: "Cooking",
            should_merge: false,
            category: "d",
        },
        ConceptPair {
            name1: "Database",
            name2: "Gardening",
            should_merge: false,
            category: "d",
        },
        ConceptPair {
            name1: "JSON",
            name2: "Cooking",
            should_merge: false,
            category: "d",
        },
        ConceptPair {
            name1: "REST",
            name2: "Furniture",
            should_merge: false,
            category: "d",
        },
        ConceptPair {
            name1: "Kubernetes",
            name2: "Cooking",
            should_merge: false,
            category: "d",
        },
        // BORDERLINE
        ConceptPair {
            name1: "Docker",
            name2: "Containerization",
            should_merge: true,
            category: "b",
        },
        ConceptPair {
            name1: "Python",
            name2: "PyTorch",
            should_merge: false,
            category: "b",
        },
    ]
}

fn fuzzy_similarity(s1: &str, s2: &str) -> f32 {
    strsim::jaro_winkler(&s1.to_lowercase(), &s2.to_lowercase()) as f32
}

fn score_prediction(similarity: f32, threshold: f32, should_merge: bool) -> f32 {
    let predicted_merge = similarity >= threshold;
    if predicted_merge == should_merge {
        1.0
    } else {
        0.0
    }
}

#[test]
#[ignore]
fn benchmark_twopass_algorithm() {
    println!("\n╔══════════════════════════════════════════════════════════════╗");
    println!("║  BENCHMARK: Two-Pass Algorithm (Fuzzy + Semantic)           ║");
    println!("╚══════════════════════════════════════════════════════════════╝\n");

    let pairs = get_large_concept_set();
    println!(
        "Test pairs: {} (simulating 100-concept database)\n",
        pairs.len()
    );

    let fuzzy_threshold = 0.85;
    let semantic_threshold = 0.75;
    let model_name = "Xenova/all-MiniLM-L6-v2";

    // PHASE 1: Fuzzy Pass
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!("PHASE 1️⃣: Fuzzy Matching (Fast Pre-Filter)");
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");

    let start = Instant::now();
    let mut fuzzy_matches = Vec::new();
    let mut fuzzy_results = Vec::new();
    let mut fuzzy_correct = 0;

    for pair in &pairs {
        let sim = fuzzy_similarity(pair.name1, pair.name2);
        let score = score_prediction(sim, fuzzy_threshold, pair.should_merge);
        fuzzy_results.push((pair, sim, score));

        // Track which pairs matched fuzzy threshold (for semantic pass)
        if sim >= fuzzy_threshold {
            fuzzy_matches.push((pair, sim));
        }

        if score > 0.5 {
            fuzzy_correct += 1;
        }
    }

    let fuzzy_duration = start.elapsed();
    let fuzzy_accuracy = fuzzy_correct as f32 / pairs.len() as f32;

    println!("Threshold: {}", fuzzy_threshold);
    println!("Total pairs: {}", pairs.len());
    println!(
        "Fuzzy matches: {} ({:.1}%)",
        fuzzy_matches.len(),
        fuzzy_matches.len() as f32 / pairs.len() as f32 * 100.0
    );
    println!("Latency: {:.2}ms", fuzzy_duration.as_secs_f32() * 1000.0);
    println!(
        "Accuracy: {}/{} ({:.1}%)",
        fuzzy_correct,
        pairs.len(),
        fuzzy_accuracy * 100.0
    );
    println!();

    // PHASE 2: Semantic Pass (only on fuzzy matches)
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!("PHASE 2️⃣: Semantic Filtering (Only Fuzzy Matches)");
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");

    let start_semantic = Instant::now();
    let mut semantic_results = Vec::new();
    let mut semantic_correct = 0;
    let mut semantic_improved = 0;

    for (pair, fuzzy_sim) in &fuzzy_matches {
        match semantic_dedup::similarity(pair.name1, pair.name2, model_name) {
            Ok(sem_sim) => {
                let score = score_prediction(sem_sim, semantic_threshold, pair.should_merge);
                semantic_results.push((*pair, sem_sim, score));
                if score > 0.5 {
                    semantic_correct += 1;
                }

                // Check if semantic refined the fuzzy prediction
                let fuzzy_pred = fuzzy_sim >= &fuzzy_threshold;
                let semantic_pred = sem_sim >= semantic_threshold;
                if fuzzy_pred != semantic_pred && semantic_pred == pair.should_merge {
                    semantic_improved += 1;
                }
            }
            Err(e) => {
                eprintln!("Error: {}", e);
            }
        }
    }

    let semantic_duration = start_semantic.elapsed();
    let semantic_accuracy = if semantic_results.is_empty() {
        0.0
    } else {
        semantic_correct as f32 / semantic_results.len() as f32
    };

    println!("Fuzzy matches to filter: {}", fuzzy_matches.len());
    println!(
        "Semantic latency: {:.2}ms",
        semantic_duration.as_secs_f32() * 1000.0
    );
    println!(
        "Per-pair latency: {:.2}ms",
        semantic_duration.as_secs_f32() * 1000.0 / fuzzy_matches.len() as f32
    );
    println!(
        "Semantic accuracy on fuzzy matches: {}/{} ({:.1}%)",
        semantic_correct,
        semantic_results.len(),
        semantic_accuracy * 100.0
    );
    println!("Cases where semantic refined result: {}", semantic_improved);
    println!();

    // OVERALL COMPARISON
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!("📊 TWO-PASS vs FUZZY-ONLY");
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");

    let total_twopass_latency = fuzzy_duration.as_secs_f32() + semantic_duration.as_secs_f32();
    let twopass_speedup = semantic_duration.as_secs_f32() / total_twopass_latency;

    println!("\nFuzzy-Only:");
    println!(
        "  Latency: {:.2}ms ({:.3}ms per pair)",
        fuzzy_duration.as_secs_f32() * 1000.0,
        fuzzy_duration.as_secs_f32() * 1000.0 / pairs.len() as f32
    );
    println!("  Accuracy: {:.1}%", fuzzy_accuracy * 100.0);

    println!("\nTwo-Pass (Fuzzy + Semantic):");
    println!(
        "  Fuzzy phase: {:.2}ms",
        fuzzy_duration.as_secs_f32() * 1000.0
    );
    println!(
        "  Semantic phase: {:.2}ms ({:.1}% of total)",
        semantic_duration.as_secs_f32() * 1000.0,
        twopass_speedup * 100.0
    );
    println!("  Total: {:.2}ms", total_twopass_latency * 1000.0);
    println!("  Accuracy (fuzzy part): {:.1}%", fuzzy_accuracy * 100.0);
    println!(
        "  Accuracy (semantic refined): {:.1}%",
        semantic_accuracy * 100.0
    );

    println!("\nEfficiency:");
    println!(
        "  Semantic needed for: {} / {} pairs ({:.1}%)",
        fuzzy_matches.len(),
        pairs.len(),
        fuzzy_matches.len() as f32 / pairs.len() as f32 * 100.0
    );
    println!(
        "  Latency reduction vs semantic-all: {:.0}x faster",
        (semantic_duration.as_secs_f32() * pairs.len() as f32)
            / (fuzzy_matches.len() as f32 * total_twopass_latency)
    );

    // Verdict
    println!("\n━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!("🎯 VERDICT");
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");

    if semantic_improved > 0 {
        println!("\n✅ Semantic refinement helped!");
        println!(
            "   {} borderline cases were correctly resolved by semantic filtering",
            semantic_improved
        );
        println!(
            "   Cost: Only {:.2}ms extra latency for entire database",
            semantic_duration.as_secs_f32() * 1000.0
        );
        println!("\n   RECOMMENDATION: Enable semantic dedup when:");
        println!("   - Accuracy is critical (merge operations are expensive to undo)");
        println!("   - Merge candidate review is manual (reduce false positives)");
        println!("   - Database size is moderate (< 10k concepts)");
    } else {
        println!("\n⚠️ Semantic refinement didn't improve accuracy on this dataset");
        println!("   Fuzzy matching alone may be sufficient");
        println!(
            "   Cost: {:.2}ms extra latency",
            semantic_duration.as_secs_f32() * 1000.0
        );
    }

    println!("\n━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!("✅ Benchmark complete\n");
}
