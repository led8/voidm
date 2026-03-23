//! Benchmark: Semantic Dedup vs Fuzzy Matching
//!
//! This test compares the quality and performance of semantic deduplication
//! against traditional fuzzy string matching (Jaro-Winkler).
//!
//! Run with: cargo test --test semantic_dedup_benchmark -- --nocapture --ignored

use std::time::Instant;
use voidm_core::semantic_dedup;

#[derive(Debug, Clone)]
struct ConceptPair {
    name1: &'static str,
    name2: &'static str,
    should_merge: bool,     // Ground truth: should these concepts merge?
    category: &'static str, // For grouping results
}

/// Test data: concept pairs with ground truth merge decisions
fn get_test_pairs() -> Vec<ConceptPair> {
    vec![
        // RELATED (should merge, high semantic similarity)
        ConceptPair {
            name1: "Docker",
            name2: "Dockerfile",
            should_merge: true,
            category: "related",
        },
        ConceptPair {
            name1: "API",
            name2: "API Design",
            should_merge: true,
            category: "related",
        },
        ConceptPair {
            name1: "Configuration",
            name2: "Config",
            should_merge: true,
            category: "related",
        },
        ConceptPair {
            name1: "Authentication",
            name2: "Authorization",
            should_merge: true,
            category: "related",
        },
        ConceptPair {
            name1: "Database",
            name2: "Database Management",
            should_merge: true,
            category: "related",
        },
        ConceptPair {
            name1: "Testing",
            name2: "Unit Test",
            should_merge: true,
            category: "related",
        },
        ConceptPair {
            name1: "HTTP",
            name2: "HTTPS",
            should_merge: true,
            category: "related",
        },
        ConceptPair {
            name1: "JSON",
            name2: "JSON Schema",
            should_merge: true,
            category: "related",
        },
        // DIFFERENT (should NOT merge, low semantic similarity)
        ConceptPair {
            name1: "Docker",
            name2: "Python",
            should_merge: false,
            category: "different",
        },
        ConceptPair {
            name1: "API",
            name2: "Database",
            should_merge: false,
            category: "different",
        },
        ConceptPair {
            name1: "Docker",
            name2: "Cooking",
            should_merge: false,
            category: "different",
        },
        ConceptPair {
            name1: "Authentication",
            name2: "Cooking",
            should_merge: false,
            category: "different",
        },
        ConceptPair {
            name1: "HTTP",
            name2: "Pizza",
            should_merge: false,
            category: "different",
        },
        ConceptPair {
            name1: "Testing",
            name2: "Cooking",
            should_merge: false,
            category: "different",
        },
        // BORDERLINE (uncertain, test how well each method handles)
        ConceptPair {
            name1: "Docker",
            name2: "Containerization",
            should_merge: true,
            category: "borderline", // Docker IS a containerization tool
        },
        ConceptPair {
            name1: "Python",
            name2: "PyTorch",
            should_merge: false,
            category: "borderline", // Fuzzy match but semantically different
        },
        ConceptPair {
            name1: "REST",
            name2: "RESTful",
            should_merge: true,
            category: "borderline",
        },
        ConceptPair {
            name1: "Kubernetes",
            name2: "K8s",
            should_merge: true,
            category: "borderline", // Common abbreviation
        },
    ]
}

/// Calculate Jaro-Winkler similarity (fuzzy matching)
fn fuzzy_similarity(s1: &str, s2: &str) -> f32 {
    strsim::jaro_winkler(&s1.to_lowercase(), &s2.to_lowercase()) as f32
}

/// Score a prediction: 1.0 if correct, 0.0 if wrong
fn score_prediction(similarity: f32, threshold: f32, should_merge: bool) -> f32 {
    let predicted_merge = similarity >= threshold;
    if predicted_merge == should_merge {
        1.0
    } else {
        0.0
    }
}

/// Run benchmark comparing fuzzy vs semantic dedup
#[test]
#[ignore] // Run with: cargo test --test semantic_dedup_benchmark -- --nocapture --ignored
fn benchmark_fuzzy_vs_semantic() {
    println!("\n╔══════════════════════════════════════════════════════════════╗");
    println!("║  BENCHMARK: Fuzzy Matching vs Semantic Deduplication        ║");
    println!("╚══════════════════════════════════════════════════════════════╝\n");

    let test_pairs = get_test_pairs();
    println!("Test pairs: {}", test_pairs.len());
    println!("Categories: related (8), different (6), borderline (4)\n");

    // FUZZY MATCHING BENCHMARK
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!("📊 FUZZY MATCHING (Jaro-Winkler)");
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");

    let fuzzy_threshold = 0.85;
    let start = Instant::now();
    let mut fuzzy_scores = Vec::new();
    let mut fuzzy_correct = 0;

    for pair in &test_pairs {
        let sim = fuzzy_similarity(pair.name1, pair.name2);
        let score = score_prediction(sim, fuzzy_threshold, pair.should_merge);
        fuzzy_scores.push((pair, sim, score));
        if score > 0.5 {
            fuzzy_correct += 1;
        }
    }

    let fuzzy_duration = start.elapsed();
    let fuzzy_accuracy = fuzzy_correct as f32 / test_pairs.len() as f32;

    println!("Threshold: {}", fuzzy_threshold);
    println!(
        "Latency: {:.2}ms (all pairs)",
        fuzzy_duration.as_secs_f32() * 1000.0
    );
    println!(
        "Accuracy: {}/{} ({:.1}%)",
        fuzzy_correct,
        test_pairs.len(),
        fuzzy_accuracy * 100.0
    );
    println!();

    // Show fuzzy results by category
    println!("Results by category:");
    for category in &["related", "different", "borderline"] {
        let category_pairs: Vec<_> = fuzzy_scores
            .iter()
            .filter(|(p, _, _)| p.category == *category)
            .collect();

        let correct = category_pairs.iter().filter(|(_, _, s)| *s > 0.5).count();
        let cat_accuracy = correct as f32 / category_pairs.len() as f32;
        println!(
            "  {}: {}/{} ({:.0}%)",
            category,
            correct,
            category_pairs.len(),
            cat_accuracy * 100.0
        );

        for (pair, sim, score) in &category_pairs {
            let status = if *score > 0.5 { "✓" } else { "✗" };
            println!(
                "    {} {} vs {}: {:.3} ({})",
                status,
                pair.name1,
                pair.name2,
                sim,
                if pair.should_merge {
                    "should merge"
                } else {
                    "shouldn't merge"
                }
            );
        }
    }

    // SEMANTIC DEDUP BENCHMARK
    println!("\n━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!("🧠 SEMANTIC DEDUP (MiniLM Embeddings)");
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");

    let semantic_threshold = 0.75;
    let model_name = "Xenova/all-MiniLM-L6-v2";

    let start = Instant::now();
    let mut semantic_scores = Vec::new();
    let mut semantic_correct = 0;
    let mut semantic_errors = 0;

    for pair in &test_pairs {
        match semantic_dedup::similarity(pair.name1, pair.name2, model_name) {
            Ok(sim) => {
                let score = score_prediction(sim, semantic_threshold, pair.should_merge);
                semantic_scores.push((pair, sim, score));
                if score > 0.5 {
                    semantic_correct += 1;
                }
            }
            Err(e) => {
                eprintln!(
                    "Error computing semantic similarity for {} vs {}: {}",
                    pair.name1, pair.name2, e
                );
                semantic_errors += 1;
            }
        }
    }

    let semantic_duration = start.elapsed();
    let semantic_accuracy = if semantic_scores.is_empty() {
        0.0
    } else {
        semantic_correct as f32 / semantic_scores.len() as f32
    };

    println!("Threshold: {}", semantic_threshold);
    println!(
        "Latency: {:.2}ms ({} pairs)",
        semantic_duration.as_secs_f32() * 1000.0,
        semantic_scores.len()
    );
    println!(
        "Per-pair latency: {:.2}ms",
        semantic_duration.as_secs_f32() * 1000.0 / semantic_scores.len() as f32
    );
    println!(
        "Accuracy: {}/{} ({:.1}%)",
        semantic_correct,
        semantic_scores.len(),
        semantic_accuracy * 100.0
    );
    if semantic_errors > 0 {
        println!("⚠ Errors: {}", semantic_errors);
    }
    println!();

    // Show semantic results by category
    println!("Results by category:");
    for category in &["related", "different", "borderline"] {
        let category_pairs: Vec<_> = semantic_scores
            .iter()
            .filter(|(p, _, _)| p.category == *category)
            .collect();

        if category_pairs.is_empty() {
            continue;
        }

        let correct = category_pairs.iter().filter(|(_, _, s)| *s > 0.5).count();
        let cat_accuracy = correct as f32 / category_pairs.len() as f32;
        println!(
            "  {}: {}/{} ({:.0}%)",
            category,
            correct,
            category_pairs.len(),
            cat_accuracy * 100.0
        );

        for (pair, sim, score) in &category_pairs {
            let status = if *score > 0.5 { "✓" } else { "✗" };
            println!(
                "    {} {} vs {}: {:.3} ({})",
                status,
                pair.name1,
                pair.name2,
                sim,
                if pair.should_merge {
                    "should merge"
                } else {
                    "shouldn't merge"
                }
            );
        }
    }

    // COMPARISON & SUMMARY
    println!("\n━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!("📈 COMPARISON SUMMARY");
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");

    println!("\nMethod         | Accuracy  | Latency    | Per-Pair  | Notes");
    println!("───────────────┼───────────┼────────────┼───────────┼──────────────────────");
    println!(
        "Fuzzy          | {:.1}%     | {:.2}ms     | {:.3}ms   | Fast, baseline",
        fuzzy_accuracy * 100.0,
        fuzzy_duration.as_secs_f32() * 1000.0,
        fuzzy_duration.as_secs_f32() * 1000.0 / test_pairs.len() as f32
    );
    println!(
        "Semantic       | {:.1}%     | {:.2}ms     | {:.2}ms    | Slower, better quality",
        semantic_accuracy * 100.0,
        semantic_duration.as_secs_f32() * 1000.0,
        semantic_duration.as_secs_f32() * 1000.0 / semantic_scores.len() as f32
    );

    let accuracy_improvement = (semantic_accuracy - fuzzy_accuracy) * 100.0;
    let latency_ratio = semantic_duration.as_secs_f32() / fuzzy_duration.as_secs_f32();

    println!("\nQuality Impact:");
    println!("  Accuracy improvement: {:.1}%", accuracy_improvement);
    println!("  Latency increase: {:.1}x slower", latency_ratio);

    if accuracy_improvement > 10.0 {
        println!("  ✅ VERDICT: Semantic dedup worth the latency cost");
    } else if accuracy_improvement > 0.0 {
        println!("  ⚠️  VERDICT: Marginal improvement, consider trade-off");
    } else {
        println!("  ❌ VERDICT: Fuzzy matching sufficient");
    }

    // DETAILED DIFFERENCE ANALYSIS
    println!("\n━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!("🔍 DETAILED DIFFERENCES (Fuzzy vs Semantic)");
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");

    let mut improvements = Vec::new();
    let mut regressions = Vec::new();

    for pair in &test_pairs {
        let fuzzy_sim = fuzzy_similarity(pair.name1, pair.name2);
        let fuzzy_pred = fuzzy_sim >= fuzzy_threshold;

        let semantic_sim = semantic_scores
            .iter()
            .find(|(p, _, _)| p.name1 == pair.name1 && p.name2 == pair.name2)
            .map(|(_, s, _)| *s);

        if let Some(sem_sim) = semantic_sim {
            let semantic_pred = sem_sim >= semantic_threshold;
            let fuzzy_correct = fuzzy_pred == pair.should_merge;
            let semantic_correct = semantic_pred == pair.should_merge;

            if !fuzzy_correct && semantic_correct {
                improvements.push((pair.name1, pair.name2, fuzzy_sim, sem_sim));
            } else if fuzzy_correct && !semantic_correct {
                regressions.push((pair.name1, pair.name2, fuzzy_sim, sem_sim));
            }
        }
    }

    if !improvements.is_empty() {
        println!("\n✅ Cases where semantic dedup improved:");
        for (n1, n2, fuzzy, sem) in improvements {
            println!(
                "  {} vs {}: fuzzy {:.3} → semantic {:.3}",
                n1, n2, fuzzy, sem
            );
        }
    }

    if !regressions.is_empty() {
        println!("\n❌ Cases where semantic dedup regressed:");
        for (n1, n2, fuzzy, sem) in regressions {
            println!(
                "  {} vs {}: fuzzy {:.3} → semantic {:.3}",
                n1, n2, fuzzy, sem
            );
        }
    }

    println!("\n━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!("✅ Benchmark complete\n");
}
