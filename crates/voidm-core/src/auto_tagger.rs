//! Automatic tag generation and enrichment for memory content.
//!
//! Combines three strategies for generating tags:
//! 1. NER (Named Entity Extraction) - entity recognition from content
//! 2. TF (Term Frequency) - keyword extraction via frequency analysis
//! 3. Rules - type-specific patterns and hardcoded lists
//!
//! Tags are merged with user-provided tags, deduplicated, and returned.

use crate::config::Config;
use crate::models::{AddMemoryRequest, MemoryType};
use anyhow::Result;
use std::collections::{HashMap, HashSet};

// ─── Stopwords list ───────────────────────────────────────────────────────────

const STOPWORDS: &[&str] = &[
    // articles
    "the",
    "a",
    "an",
    // conjunctions
    "and",
    "or",
    "but",
    // prepositions
    "in",
    "on",
    "at",
    "to",
    "for",
    "of",
    "is",
    "are",
    "was",
    "were",
    // auxiliary verbs
    "be",
    "been",
    "being",
    "have",
    "has",
    "had",
    "do",
    "does",
    "did",
    // modals
    "will",
    "would",
    "could",
    "should",
    "may",
    "might",
    "must",
    "can",
    // pronouns
    "this",
    "that",
    "these",
    "those",
    "i",
    "you",
    "he",
    "she",
    "it",
    "we",
    "they",
    // interrogatives
    "what",
    "which",
    "who",
    "when",
    "where",
    "why",
    "how",
    // negation
    "not",
    "no",
    "yes",
    // other
    "as",
    "by",
    "with",
    "from",
    // domain-specific
    "memory",
    "note",
    "tag",
    "voidm",
    "system",
    "data",
    "information",
];

// ─── Type-specific keywords and patterns ────────────────────────────────────

const ACTION_VERBS: &[&str] = &[
    "attended",
    "visited",
    "deployed",
    "implemented",
    "learned",
    "participated",
    "created",
    "built",
    "designed",
    "configured",
    "installed",
    "managed",
    "led",
    "discussed",
    "presented",
    "wrote",
    "published",
    "reviewed",
];

const TECH_KEYWORDS: &[&str] = &[
    "docker",
    "kubernetes",
    "terraform",
    "ansible",
    "jenkins",
    "gitlab",
    "github",
    "aws",
    "gcp",
    "azure",
    "docker-compose",
    "python",
    "rust",
    "go",
    "javascript",
    "java",
    "typescript",
    "react",
    "vue",
    "angular",
    "flask",
    "fastapi",
    "spring",
    "rails",
    "postgresql",
    "mysql",
    "mongodb",
    "redis",
    "elasticsearch",
    "nginx",
    "apache",
    "linux",
    "windows",
    "macos",
];

const RESOURCE_TYPES: &[&str] = &[
    "server",
    "cluster",
    "container",
    "service",
    "pipeline",
    "infrastructure",
    "database",
    "cache",
    "api",
    "endpoint",
    "queue",
];

// ─── Main public function ──────────────────────────────────────────────────────

/// Enrich memory tags by extracting tags from content and merging with user-provided tags.
/// Returns early if auto-tagging is disabled, logging only.
pub fn enrich_memory_tags(req: &mut AddMemoryRequest, config: &Config) -> Result<()> {
    // Check if auto-tagging is enabled
    if !should_enable_auto_tagging(config) {
        return Ok(());
    }

    // Extract tags using three strategies
    let entity_tags = extract_entity_tags(&req.content, config).unwrap_or_default();
    let keyword_tags = extract_keyword_tags(&req.content, config);
    let type_specific_tags = extract_type_specific_tags(&req.content, &req.memory_type, config);

    // Merge all auto-generated tags with user tags
    let mut all_auto_tags = entity_tags;
    all_auto_tags.extend(keyword_tags);
    all_auto_tags.extend(type_specific_tags);

    // Deduplicate auto-generated tags for storage
    let auto_tags_dedup: Vec<String> = all_auto_tags
        .iter()
        .collect::<HashSet<_>>()
        .into_iter()
        .map(|s| s.clone())
        .collect();

    // Store auto-generated tags separately for display (in metadata)
    if !auto_tags_dedup.is_empty() {
        if let Ok(auto_tags_json) = serde_json::to_value(&auto_tags_dedup) {
            if let Some(obj) = req.metadata.as_object_mut() {
                obj.insert("auto_generated_tags".to_string(), auto_tags_json);
            }
        }
    }

    let final_tags = merge_tags(&all_auto_tags, &req.tags, config);
    req.tags = final_tags;
    Ok(())
}

// ─── Configuration helpers ───────────────────────────────────────────────────

fn should_enable_auto_tagging(_config: &Config) -> bool {
    // For now, auto-tagging is always enabled by default
    // Future: add config.memory.auto_tagging.enabled check
    true
}

fn get_max_tags(_config: &Config) -> usize {
    // Future: read from config.memory.auto_tagging.max_tags
    20
}

fn get_confidence_threshold(_config: &Config) -> f32 {
    // Future: read from config.memory.auto_tagging.confidence_threshold
    0.5
}

fn should_use_stopwords(_config: &Config) -> bool {
    // Future: read from config.memory.auto_tagging.use_stopwords
    true
}

// ─── Tag extraction: NER ───────────────────────────────────────────────────────

fn extract_entity_tags(content: &str, config: &Config) -> Result<Vec<String>> {
    // Call NER module to extract entities
    match crate::ner::extract_entities(content) {
        Ok(entities) => {
            let threshold = get_confidence_threshold(config);
            let tags: Vec<String> = entities
                .iter()
                .filter(|e| e.score >= threshold)
                .map(|e| normalize_tag(&e.text))
                .collect::<HashSet<_>>() // deduplicate
                .into_iter()
                .collect();
            Ok(tags)
        }
        Err(e) => {
            tracing::warn!("NER extraction failed: {}. Skipping entity tags.", e);
            Ok(vec![])
        }
    }
}

// ─── Tag extraction: Term Frequency ────────────────────────────────────────────

fn extract_keyword_tags(content: &str, config: &Config) -> Vec<String> {
    let use_stopwords = should_use_stopwords(config);

    // Tokenize and count frequencies
    let tokens = tokenize(content);
    let mut freqs: HashMap<String, usize> = HashMap::new();

    for token in tokens {
        if use_stopwords && is_stopword(&token) {
            continue;
        }
        if token.len() < 3 {
            continue; // skip very short tokens
        }
        *freqs.entry(token).or_insert(0) += 1;
    }

    // Keep top 8 keywords by frequency
    let mut keywords: Vec<_> = freqs.into_iter().collect();
    keywords.sort_by(|a, b| b.1.cmp(&a.1));

    keywords
        .iter()
        .take(8)
        .map(|(k, _)| k.to_string())
        .collect()
}

fn tokenize(text: &str) -> Vec<String> {
    text.to_lowercase()
        .split(|c: char| !c.is_alphanumeric() && c != '-')
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .collect()
}

fn is_stopword(word: &str) -> bool {
    STOPWORDS.contains(&word.to_lowercase().as_str())
}

// ─── Tag extraction: Type-Specific Rules ────────────────────────────────────

fn extract_type_specific_tags(
    content: &str,
    memory_type: &MemoryType,
    _config: &Config,
) -> Vec<String> {
    match memory_type {
        MemoryType::Episodic => extract_episodic_tags(content),
        MemoryType::Semantic => extract_semantic_tags(content),
        MemoryType::Procedural => extract_procedural_tags(content),
        MemoryType::Conceptual => extract_conceptual_tags(content),
        MemoryType::Contextual => extract_contextual_tags(content),
    }
}

fn extract_episodic_tags(content: &str) -> Vec<String> {
    let mut tags = Vec::new();
    let content_lower = content.to_lowercase();

    // Extract action verbs
    for verb in ACTION_VERBS {
        if content_lower.contains(verb) {
            tags.push(verb.to_string());
        }
    }

    // Extract year patterns (simple: any 4-digit number starting with 19 or 20)
    let words: Vec<&str> = content_lower.split_whitespace().collect();
    for word in words {
        if let Ok(year) = word.parse::<u32>() {
            if (1900..=2100).contains(&year) {
                tags.push(year.to_string());
            }
        }
    }

    tags
}

fn extract_semantic_tags(content: &str) -> Vec<String> {
    let mut tags = Vec::new();
    let content_lower = content.to_lowercase();

    // Look for definition patterns: "is", "means", "refers to", "defined as"
    let definition_phrases = ["is a", "means", "refers to", "defined as", "can be"];
    for phrase in &definition_phrases {
        if content_lower.contains(phrase) {
            tags.push("definition".to_string());
            break;
        }
    }

    tags
}

fn extract_procedural_tags(content: &str) -> Vec<String> {
    let mut tags = Vec::new();
    let content_lower = content.to_lowercase();

    // Extract technology keywords
    for tech in TECH_KEYWORDS {
        if content_lower.contains(tech) {
            tags.push(tech.to_string());
        }
    }

    // Extract action verbs
    for verb in ACTION_VERBS {
        if content_lower.contains(verb) {
            tags.push(verb.to_string());
        }
    }

    // Extract resource types
    for resource in RESOURCE_TYPES {
        if content_lower.contains(resource) {
            tags.push(resource.to_string());
        }
    }

    tags
}

fn extract_conceptual_tags(content: &str) -> Vec<String> {
    let mut tags = Vec::new();
    let content_lower = content.to_lowercase();

    // Look for definition patterns
    let definition_phrases = ["is", "means", "concept of", "idea of"];
    for phrase in &definition_phrases {
        if content_lower.contains(phrase) {
            tags.push("conceptual".to_string());
            break;
        }
    }

    tags
}

fn extract_contextual_tags(content: &str) -> Vec<String> {
    // Similar to semantic for now
    extract_semantic_tags(content)
}

// ─── Utility functions ────────────────────────────────────────────────────────

fn normalize_tag(tag: &str) -> String {
    tag.to_lowercase()
        .replace(" ", "-")
        .replace("_", "-")
        .replace(".", "-")
        .replace(",", "")
}

/// Merge auto-generated tags with user-provided tags.
///
/// Logic:
/// 1. Start with user tags (these take precedence)
/// 2. Add auto-generated tags
/// 3. Deduplicate (case-insensitive)
/// 4. Remove substrings (keep longer tags)
/// 5. Limit to max_tags
fn merge_tags(auto_tags: &[String], user_tags: &[String], config: &Config) -> Vec<String> {
    let max_tags = get_max_tags(config);

    // Start with user tags
    let mut all_tags = user_tags.to_vec();

    // Add auto tags, deduplicating during addition
    let mut seen = HashSet::new();
    for tag in user_tags {
        seen.insert(tag.to_lowercase());
    }

    for tag in auto_tags {
        let normalized = tag.to_lowercase();
        if !seen.contains(&normalized) {
            seen.insert(normalized);
            all_tags.push(tag.clone());
        }
    }

    // Remove substrings (keep longer tags)
    let mut filtered = Vec::new();
    for tag in &all_tags {
        let mut is_substring = false;
        for other in &all_tags {
            if tag != other && other.to_lowercase().contains(&tag.to_lowercase()) {
                is_substring = true;
                break;
            }
        }
        if !is_substring {
            filtered.push(tag.clone());
        }
    }

    // Limit to max_tags
    filtered.truncate(max_tags);
    filtered
}
