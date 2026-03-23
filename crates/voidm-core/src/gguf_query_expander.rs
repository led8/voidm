//! GGUF-based query expansion using llama-gguf.
//!
//! This module provides query expansion using GGUF format models,
//! specifically optimized for the tobil/qmd-query-expansion-1.7B model.
//!
//! Features:
//! - Uses llama-gguf Engine for GGUF model inference
//! - Structured output parsing (lex:/vec:/hyde: format)
//! - Automatic model caching via HuggingFace hub
//! - Optional feature (requires --features gguf)
//!
//! Model Details:
//! - tobil/qmd-query-expansion-1.7B-q4_k_m.gguf (1223 MB)
//! - Base model: Qwen3-1.7B
//! - Output format: lex:/vec:/hyde: (lexical, vector, hypothetical)

#[cfg(feature = "gguf")]
use anyhow::{anyhow, Context, Result};
#[cfg(feature = "gguf")]
use std::path::PathBuf;

// Struct definition for both feature and non-feature cases
/// GGUF-based query expander for qmd model
pub struct GgufQueryExpander {
    #[cfg(feature = "gguf")]
    model_name: String,
    #[cfg(not(feature = "gguf"))]
    _private: (),
}

#[cfg(feature = "gguf")]
impl GgufQueryExpander {
    /// Create a new GGUF query expander
    pub fn new(model_name: String) -> Self {
        Self { model_name }
    }

    /// Expand a query using GGUF model
    pub async fn expand(&self, query: &str) -> Result<String> {
        tracing::info!("GGUF query expansion: Starting for query: '{}'", query);
        tracing::debug!("GGUF query expansion: model_name={}", self.model_name);

        // Use llama-gguf to perform inference
        self.expand_with_gguf(query).await
    }

    /// Internal expansion using llama-gguf
    async fn expand_with_gguf(&self, query: &str) -> Result<String> {
        // Get or download the model
        let model_path = Self::get_model_path(&self.model_name).await?;

        tracing::debug!("GGUF: Loading model from: {}", model_path.display());

        // Load the model (with caching)
        let engine = Self::load_model(&self.model_name, &model_path)?;

        tracing::debug!("GGUF: Model loaded successfully, preparing prompt");

        // Prepare the prompt
        let prompt = Self::prepare_prompt(query);
        tracing::debug!("GGUF: Prompt prepared, length={}", prompt.len());

        // Run inference
        let output = engine
            .generate(&prompt, 100)
            .context("GGUF inference failed")?;

        tracing::debug!("GGUF: Inference complete, output length={}", output.len());

        // Parse the structured output (lex:/vec:/hyde: format)
        let expanded = Self::parse_structured_output(&output, query)?;

        tracing::debug!("GGUF: Parsed expansion result");

        Ok(expanded)
    }

    /// Load model with caching
    #[cfg(feature = "gguf")]
    fn load_model(_model_name: &str, model_path: &PathBuf) -> Result<llama_gguf::engine::Engine> {
        // Note: llama-gguf Engine is not Clone, so we create a new one each time
        // The underlying GGUF file is mmap'd so this is relatively cheap
        tracing::debug!("GGUF: Loading model from: {}", model_path.display());

        llama_gguf::engine::Engine::load(llama_gguf::engine::EngineConfig {
            model_path: model_path.to_string_lossy().to_string(),
            temperature: 0.1, // Low temperature for more consistent output
            top_k: 40,
            top_p: 0.9,
            max_tokens: 100,
            ..Default::default()
        })
        .context(format!(
            "Failed to load GGUF model from: {}",
            model_path.display()
        ))
    }

    /// Get or download the model file
    async fn get_model_path(model_name: &str) -> Result<PathBuf> {
        use hf_hub::api::sync::Api;

        // Resolve HuggingFace model ID
        let hf_id = Self::get_huggingface_id(model_name)
            .ok_or_else(|| anyhow!("Unknown GGUF model: {}", model_name))?;

        tracing::info!("GGUF: Resolving model from HuggingFace: {}", hf_id);

        // Use HuggingFace hub to get the model path (with caching)
        let api = Api::new().context("Failed to initialize HuggingFace API")?;

        let repo = api.model(hf_id.clone());

        // Get the specific GGUF file
        let filename = if model_name.contains("tobil") {
            "qmd-query-expansion-1.7B-q4_k_m.gguf"
        } else {
            return Err(anyhow!("Unknown GGUF model filename for: {}", model_name));
        };

        let model_path = repo.get(filename).context(format!(
            "Failed to download GGUF model from HuggingFace: {}",
            hf_id
        ))?;

        tracing::info!("GGUF: Model ready at: {}", model_path.display());

        Ok(model_path)
    }

    /// Prepare the prompt for query expansion
    fn prepare_prompt(query: &str) -> String {
        // QMD-optimized prompt for structured expansion output
        format!(
            r#"Expand the following search query with related terms for better retrieval:

Query: {}
lex: "#,
            query
        )
    }

    /// Parse structured GGUF output (lex:/vec:/hyde: format)
    fn parse_structured_output(output: &str, original_query: &str) -> Result<String> {
        let output = output.trim();

        tracing::debug!(
            "GGUF: Parsing output (first 200 chars): {}",
            &output.chars().take(200).collect::<String>()
        );

        // Extract terms from structured output
        let mut keywords = Vec::new();
        let mut semantic_phrases = Vec::new();

        // Parse lex: section (lexical terms)
        if let Some(lex_start) = output.find("lex:") {
            let lex_content = &output[lex_start + 4..];
            // Get content until next section or end
            let lex_end = lex_content.find("vec:").unwrap_or(lex_content.len());
            let lex_terms = lex_content[..lex_end].trim();

            keywords.extend(
                lex_terms
                    .split(',')
                    .map(|s| s.trim())
                    .filter(|s| !s.is_empty())
                    .collect::<Vec<_>>(),
            );
        }

        // Parse vec: section (vector/semantic terms)
        if let Some(vec_start) = output.find("vec:") {
            let vec_content = &output[vec_start + 4..];
            // Get content until next section or end
            let vec_end = vec_content.find("hyde:").unwrap_or(vec_content.len());
            let vec_terms = vec_content[..vec_end].trim();

            semantic_phrases.extend(
                vec_terms
                    .split(',')
                    .map(|s| s.trim())
                    .filter(|s| !s.is_empty())
                    .collect::<Vec<_>>(),
            );
        }

        // Parse hyde: section (hypothetical document terms)
        if let Some(hyde_start) = output.find("hyde:") {
            let hyde_content = &output[hyde_start + 5..].trim();
            keywords.extend(
                hyde_content
                    .split(',')
                    .map(|s| s.trim())
                    .filter(|s| !s.is_empty())
                    .collect::<Vec<_>>(),
            );
        }

        // Combine all terms
        let all_terms: Vec<&str> = keywords
            .iter()
            .chain(semantic_phrases.iter())
            .copied()
            .collect();

        if all_terms.is_empty() {
            tracing::warn!("GGUF: No terms extracted from output, falling back to original query");
            return Ok(original_query.to_string());
        }

        // Check if original query is already the first term
        let first_term = all_terms.first().unwrap_or(&"");
        let result = if first_term.eq_ignore_ascii_case(original_query) {
            // Original query is already present, use as-is
            all_terms.join(", ")
        } else {
            // Prepend original query
            format!("{}, {}", original_query, all_terms.join(", "))
        };

        tracing::debug!(
            "GGUF: Expansion result (first 200 chars): {}",
            result.chars().take(200).collect::<String>()
        );

        Ok(result)
    }

    /// Check if a model name should use GGUF backend
    pub fn should_use_gguf(model_name: &str) -> bool {
        model_name.contains("tobil") || model_name.contains("qmd")
    }

    /// Get the HuggingFace model ID for the given model name
    pub fn get_huggingface_id(model_name: &str) -> Option<String> {
        match model_name {
            name if name.contains("tobil/qmd-query-expansion-1.7B")
                || name == "tobil/qmd-query-expansion-1.7B" =>
            {
                Some("tobil/qmd-query-expansion-1.7B-gguf".to_string())
            }
            _ => None,
        }
    }
}

#[cfg(feature = "gguf")]
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_should_use_gguf() {
        assert!(GgufQueryExpander::should_use_gguf(
            "tobil/qmd-query-expansion-1.7B"
        ));
        assert!(GgufQueryExpander::should_use_gguf("qmd-something"));
        assert!(!GgufQueryExpander::should_use_gguf("tinyllama"));
        assert!(!GgufQueryExpander::should_use_gguf("gpt2-small"));
    }

    #[test]
    fn test_get_huggingface_id() {
        let result = GgufQueryExpander::get_huggingface_id("tobil/qmd-query-expansion-1.7B");
        assert_eq!(
            result,
            Some("tobil/qmd-query-expansion-1.7B-gguf".to_string())
        );

        let result = GgufQueryExpander::get_huggingface_id("tinyllama");
        assert_eq!(result, None);
    }

    #[test]
    fn test_prepare_prompt() {
        let prompt = GgufQueryExpander::prepare_prompt("docker");
        assert!(prompt.contains("docker"));
        assert!(prompt.contains("lex:"));
    }

    #[test]
    fn test_parse_structured_output_lex() {
        let output = "lex: containers, orchestration, deployment";
        let result = GgufQueryExpander::parse_structured_output(output, "docker").unwrap();
        assert!(result.contains("docker"));
        assert!(result.contains("containers"));
        assert!(result.contains("orchestration"));
    }

    #[test]
    fn test_parse_structured_output_all_sections() {
        let output =
            "lex: containers, images\nvec: containerization, orchestration\nhyde: Docker Compose";
        let result = GgufQueryExpander::parse_structured_output(output, "docker").unwrap();
        assert!(result.contains("docker"));
        assert!(result.contains("containers"));
        assert!(result.contains("containerization"));
        assert!(result.contains("Docker Compose"));
    }

    #[test]
    fn test_parse_structured_output_empty_falls_back() {
        let output = "";
        let result = GgufQueryExpander::parse_structured_output(output, "docker").unwrap();
        // Should return original query when parsing fails
        assert_eq!(result, "docker");
    }

    #[test]
    fn test_parse_structured_output_avoids_duplicate() {
        // If original query is already the first term, don't duplicate
        let output = "lex: docker, containers, images";
        let result = GgufQueryExpander::parse_structured_output(output, "docker").unwrap();
        // Should not have "docker, docker, ..."
        let docker_count = result.matches("docker").count();
        assert_eq!(docker_count, 1);
    }
}

#[cfg(not(feature = "gguf"))]
impl GgufQueryExpander {
    pub fn new(_model_name: String) -> Self {
        Self { _private: () }
    }

    pub async fn expand(&self, _query: &str) -> anyhow::Result<String> {
        Err(anyhow::anyhow!(
            "GGUF support not compiled in. Rebuild with --features gguf"
        ))
    }

    pub fn should_use_gguf(_model_name: &str) -> bool {
        false
    }

    pub fn get_huggingface_id(_model_name: &str) -> Option<String> {
        None
    }
}
