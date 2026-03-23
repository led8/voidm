//! Passage extraction for cross-encoder reranking.
//!
//! This module provides intelligent passage extraction to find relevant content
//! for reranking. Instead of passing full documents to cross-encoders (which are
//! trained on short passages), we extract sentences containing query terms along
//! with surrounding context.

use crate::config::PassageExtractionConfig;

/// Extract the best passage from a document that contains query terms
///
/// # Strategy
/// 1. Split document into sentences
/// 2. Find sentences containing any query term
/// 3. Extract best match with context (±context_sentences)
/// 4. If no match, fallback to first N characters
pub fn extract_best_passage(
    content: &str,
    query: &str,
    config: &PassageExtractionConfig,
) -> String {
    if !config.enabled {
        // Fallback to old truncation behavior
        return truncate_content(content, config.fallback_length);
    }

    // Parse query into terms (use String for simplicity)
    let query_terms: Vec<String> = query
        .split_whitespace()
        .filter(|t| t.len() > 2) // Skip very short terms
        .map(|t| t.to_lowercase())
        .collect();

    if query_terms.is_empty() {
        return truncate_content(content, config.fallback_length);
    }

    // Split into sentences
    let sentences = split_sentences(content);
    if sentences.is_empty() {
        return truncate_content(content, config.fallback_length);
    }

    // Find best matching sentence
    let best_match = find_best_match(&sentences, &query_terms);

    match best_match {
        Some((idx, _score)) => {
            // Extract passage with context
            let passage = extract_passage_with_context(&sentences, idx, config.context_sentences);

            // Ensure minimum length
            if passage.len() >= config.min_passage_length {
                tracing::debug!(
                    "Passage extraction: Found match at sentence {}, passage length: {}",
                    idx,
                    passage.len()
                );
                passage
            } else {
                tracing::debug!(
                    "Passage extraction: Match too short ({}), using fallback",
                    passage.len()
                );
                truncate_content(content, config.fallback_length)
            }
        }
        None => {
            tracing::debug!("Passage extraction: No query match found, using fallback");
            truncate_content(content, config.fallback_length)
        }
    }
}

/// Split document into sentences
fn split_sentences(content: &str) -> Vec<String> {
    let mut sentences = Vec::new();
    let mut current = String::new();

    for c in content.chars() {
        current.push(c);

        // Check for sentence endings
        if matches!(c, '.' | '!' | '?') {
            let trimmed = current.trim();
            if !trimmed.is_empty() && trimmed.len() > 2 {
                sentences.push(trimmed.to_string());
            }
            current.clear();
        }
    }

    // Add remaining content if any
    let trimmed = current.trim();
    if !trimmed.is_empty() && trimmed.len() > 2 {
        sentences.push(trimmed.to_string());
    }

    sentences
}

/// Find the best matching sentence containing query terms
/// Returns (index, match_score) where score is number of matching terms
fn find_best_match(sentences: &[String], query_terms: &[String]) -> Option<(usize, usize)> {
    let mut best_idx = None;
    let mut best_score = 0;

    for (idx, sentence) in sentences.iter().enumerate() {
        let sentence_lower = sentence.to_lowercase();
        let mut score = 0;

        // Count how many query terms appear in this sentence
        for term in query_terms {
            if sentence_lower.contains(term) {
                score += 1;
            }
        }

        // Prefer earlier matches if tie (more natural context)
        if score > best_score {
            best_score = score;
            best_idx = Some(idx);
        }
    }

    best_idx.map(|idx| (idx, best_score))
}

/// Extract passage with context around a sentence
fn extract_passage_with_context(
    sentences: &[String],
    center_idx: usize,
    context_sentences: usize,
) -> String {
    let start = center_idx.saturating_sub(context_sentences);
    let end = (center_idx + context_sentences + 1).min(sentences.len());

    sentences[start..end].join(" ")
}

/// Truncate content to maximum length, breaking at word boundaries
fn truncate_content(content: &str, max_length: usize) -> String {
    if content.len() <= max_length {
        return content.to_string();
    }

    // Find word boundary before max_length
    let truncated = &content[..max_length];
    match truncated.rfind(|c: char| c.is_whitespace()) {
        Some(idx) => truncated[..idx].to_string(),
        None => truncated.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_split_sentences() {
        let text = "Hello world. This is a test. What is this?";
        let sentences = split_sentences(text);
        assert_eq!(sentences.len(), 3);
        assert!(sentences[0].contains("Hello"));
        assert!(sentences[1].contains("This is a test"));
        assert!(sentences[2].contains("What is this"));
    }

    #[test]
    fn test_find_best_match_single_term() {
        let sentences = vec![
            "The quick brown fox".to_string(),
            "Machine learning is great".to_string(),
            "Python is powerful".to_string(),
        ];
        let query_terms = vec!["python".to_string()];

        let result = find_best_match(&sentences, &query_terms);
        assert_eq!(result, Some((2, 1)));
    }

    #[test]
    fn test_find_best_match_multiple_terms() {
        let sentences = vec![
            "Hello world".to_string(),
            "Machine learning python is great".to_string(),
            "Java programming language".to_string(),
        ];
        let query_terms = vec![
            "machine".to_string(),
            "learning".to_string(),
            "python".to_string(),
        ];

        let result = find_best_match(&sentences, &query_terms);
        assert_eq!(result, Some((1, 3))); // All 3 terms in sentence 1
    }

    #[test]
    fn test_find_best_match_no_match() {
        let sentences = vec!["Hello world".to_string(), "Good morning".to_string()];
        let query_terms = vec!["python".to_string()];

        let result = find_best_match(&sentences, &query_terms);
        assert_eq!(result, None);
    }

    #[test]
    fn test_extract_passage_with_context() {
        let sentences = vec![
            "First sentence.".to_string(),
            "Second sentence.".to_string(),
            "Third sentence.".to_string(),
            "Fourth sentence.".to_string(),
        ];

        let passage = extract_passage_with_context(&sentences, 1, 1);
        assert!(passage.contains("First"));
        assert!(passage.contains("Second"));
        assert!(passage.contains("Third"));
    }

    #[test]
    fn test_extract_passage_start() {
        let sentences = vec![
            "First sentence.".to_string(),
            "Second sentence.".to_string(),
            "Third sentence.".to_string(),
        ];

        let passage = extract_passage_with_context(&sentences, 0, 1);
        assert!(passage.contains("First"));
        assert!(passage.contains("Second"));
        assert!(!passage.contains("Third"));
    }

    #[test]
    fn test_extract_best_passage_with_match() {
        let content = "The weather is nice. Machine learning is interesting. Python is powerful.";
        let config = PassageExtractionConfig::default();

        let passage = extract_best_passage(content, "python", &config);
        assert!(passage.contains("Python"));
        assert!(passage.contains("interesting")); // context
    }

    #[test]
    fn test_extract_best_passage_no_match() {
        let content = "The weather is nice. The sun is shining. The sky is blue.";
        let config = PassageExtractionConfig::default();

        let passage = extract_best_passage(content, "python", &config);
        // Should use fallback (first 400 chars)
        assert!(passage.len() <= 400);
    }

    #[test]
    fn test_extract_best_passage_disabled() {
        let content = "The weather is nice. Python is great. The sun shines bright.";
        let mut config = PassageExtractionConfig::default();
        config.enabled = false;

        let passage = extract_best_passage(content, "python", &config);
        // Should use truncation
        assert!(passage.len() <= 400);
    }

    #[test]
    fn test_truncate_content() {
        let long_text = "word1 word2 word3 word4 word5";
        let truncated = truncate_content(long_text, 15);
        assert_eq!(truncated, "word1 word2");
    }
}
