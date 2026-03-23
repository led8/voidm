//! NLI-based relation classifier for ontology enrichment.
//!
//! Uses `cross-encoder/nli-deberta-v3-small` (ONNX) to classify the relationship
//! between two text fragments. Outputs entailment/neutral/contradiction scores
//! which are mapped to voidm ontology edge types.
//!
//! Models are downloaded on first use via hf-hub and cached in the voidm model
//! cache directory (same pattern as embeddings).

use anyhow::{Context, Result};
use once_cell::sync::OnceCell;
use ort::session::Session;
use ort::value::Tensor;
use std::path::PathBuf;
use std::sync::Mutex;

// ─── Public types ─────────────────────────────────────────────────────────────

/// Raw NLI output for a (premise, hypothesis) pair.
#[derive(Debug, Clone)]
pub struct NliScores {
    /// P(contradiction)
    pub contradiction: f32,
    /// P(neutral)
    pub neutral: f32,
    /// P(entailment)
    pub entailment: f32,
}

/// Suggested ontology relation derived from NLI scores + embedding similarity.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct RelationSuggestion {
    pub candidate_id: String,
    pub candidate_text: String,
    pub suggested_rel: String,
    pub confidence: f32,
    pub nli_entailment: f32,
    pub nli_contradiction: f32,
    pub similarity: f32,
}

// ─── Model state ──────────────────────────────────────────────────────────────

struct NliModel {
    session: Mutex<Session>,
    tokenizer: tokenizers::Tokenizer,
}

// Session is not Send by default in ort rc.9 — we wrap in a type that is.
// Safety: we only access from one thread at a time (protected by OnceCell + caller responsibility).
struct SendSession(NliModel);
unsafe impl Send for SendSession {}
unsafe impl Sync for SendSession {}

static NLI_MODEL: OnceCell<SendSession> = OnceCell::new();

const NLI_MODEL_ID: &str = "cross-encoder/nli-deberta-v3-small";
const NLI_ONNX_FILE: &str = "onnx/model.onnx";
const NLI_TOKENIZER_FILE: &str = "tokenizer.json";

// ─── Init & download ──────────────────────────────────────────────────────────

/// Load the NLI model (download on first use). Idempotent.
pub async fn ensure_nli_model() -> Result<()> {
    if NLI_MODEL.get().is_some() {
        return Ok(());
    }
    let model = load_or_download().await?;
    let _ = NLI_MODEL.set(SendSession(model));
    Ok(())
}

async fn load_or_download() -> Result<NliModel> {
    let cache_dir = nli_cache_dir();
    std::fs::create_dir_all(&cache_dir)
        .with_context(|| format!("Cannot create NLI cache dir: {}", cache_dir.display()))?;

    let onnx_path = cache_dir.join("model.onnx");
    let tokenizer_path = cache_dir.join("tokenizer.json");

    // Download if missing
    if !onnx_path.exists() || !tokenizer_path.exists() {
        tracing::info!("Downloading NLI model '{}' (first use) …", NLI_MODEL_ID);
        eprintln!(
            "Downloading NLI model '{}' (~180MB, first use only) …",
            NLI_MODEL_ID
        );
        download_model_files(&cache_dir).await?;
        tracing::info!("NLI model downloaded to {}", cache_dir.display());
        eprintln!("NLI model ready at {}", cache_dir.display());
    }

    build_session(&onnx_path, &tokenizer_path)
}

fn build_session(onnx_path: &PathBuf, tokenizer_path: &PathBuf) -> Result<NliModel> {
    let tokenizer = tokenizers::Tokenizer::from_file(tokenizer_path)
        .map_err(|e| anyhow::anyhow!("Failed to load tokenizer: {}", e))?;

    let session = Session::builder()
        .context("Failed to create ort session builder")?
        .commit_from_file(onnx_path)
        .context("Failed to load NLI ONNX model")?;

    Ok(NliModel {
        session: Mutex::new(session),
        tokenizer,
    })
}

async fn download_model_files(cache_dir: &PathBuf) -> Result<()> {
    let hf_cache = cache_dir
        .parent()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| cache_dir.clone());

    let api = hf_hub::api::tokio::ApiBuilder::new()
        .with_cache_dir(hf_cache)
        .build()
        .context("Failed to build hf-hub API")?;

    let repo = api.model(NLI_MODEL_ID.to_string());

    let onnx_src = repo
        .get(NLI_ONNX_FILE)
        .await
        .with_context(|| format!("Failed to download {} from {}", NLI_ONNX_FILE, NLI_MODEL_ID))?;
    std::fs::copy(&onnx_src, cache_dir.join("model.onnx"))
        .context("Failed to copy ONNX model to cache")?;

    let tok_src = repo.get(NLI_TOKENIZER_FILE).await.with_context(|| {
        format!(
            "Failed to download {} from {}",
            NLI_TOKENIZER_FILE, NLI_MODEL_ID
        )
    })?;
    std::fs::copy(&tok_src, cache_dir.join("tokenizer.json"))
        .context("Failed to copy tokenizer to cache")?;

    Ok(())
}

// ─── Inference ────────────────────────────────────────────────────────────────

/// Run NLI inference on a (premise, hypothesis) pair.
/// Returns softmaxed entailment/neutral/contradiction scores.
/// The model must be loaded first via `ensure_nli_model()`.
pub fn classify(premise: &str, hypothesis: &str) -> Result<NliScores> {
    let wrapper = NLI_MODEL
        .get()
        .context("NLI model not loaded. Call ensure_nli_model() first.")?;
    let model = &wrapper.0;

    // Tokenize as sentence pair
    let encoding = model
        .tokenizer
        .encode((premise, hypothesis), true)
        .map_err(|e| anyhow::anyhow!("Tokenization failed: {}", e))?;

    let input_ids: Vec<i64> = encoding.get_ids().iter().map(|&x| x as i64).collect();
    let attention_mask: Vec<i64> = encoding
        .get_attention_mask()
        .iter()
        .map(|&x| x as i64)
        .collect();
    let seq_len = input_ids.len();

    let ids_tensor = Tensor::<i64>::from_array(([1usize, seq_len], input_ids.into_boxed_slice()))
        .context("Failed to create input_ids tensor")?;

    let mask_tensor =
        Tensor::<i64>::from_array(([1usize, seq_len], attention_mask.into_boxed_slice()))
            .context("Failed to create attention_mask tensor")?;

    // Run inference and extract logits within the lock scope
    let logits_flat = {
        let mut session_lock = model.session.lock().unwrap();
        let outputs = session_lock
            .run(ort::inputs![
                "input_ids"      => ids_tensor,
                "attention_mask" => mask_tensor
            ])
            .context("NLI inference failed")?;

        // Extract logits [1, 3]: contradiction=0, neutral=1, entailment=2 (DeBERTa-NLI convention)
        let logits_value = outputs
            .get("logits")
            .context("No 'logits' output from NLI model")?;
        let logits_tensor = logits_value
            .try_extract_tensor::<f32>()
            .context("Failed to extract logits as f32 tensor")?;
        let (_shape, logits_raw) = logits_tensor;
        logits_raw.to_vec()
    };

    if logits_flat.len() < 3 {
        anyhow::bail!(
            "Unexpected logits shape: {} elements (expected 3)",
            logits_flat.len()
        );
    }

    let scores = softmax(&logits_flat[..3]);
    Ok(NliScores {
        contradiction: scores[0],
        neutral: scores[1],
        entailment: scores[2],
    })
}

// ─── Relation mapping ─────────────────────────────────────────────────────────

/// Map NLI scores + cosine similarity to the most likely ontology edge type.
///
/// Thresholds tuned empirically against nli-deberta-v3-small behaviour:
/// - The model tends toward neutral for factual/ontological pairs, so we
///   use neutral + high similarity as a positive signal, not just entailment.
/// - contradiction > 0.65                              → CONTRADICTS
/// - entailment > 0.50 + similarity > 0.80             → IS_A (strong inclusion)
/// - entailment > 0.40 + similarity > 0.65             → SUPPORTS
/// - (neutral > 0.60 OR entailment > 0.30) + sim 0.70+ → EXEMPLIFIES
/// - neutral > 0.70 + similarity 0.45–0.75             → RELATES_TO
/// - otherwise                                          → None
pub fn scores_to_rel(scores: &NliScores, similarity: f32) -> Option<(String, f32)> {
    if scores.contradiction > 0.80 {
        return Some(("CONTRADICTS".into(), scores.contradiction));
    }
    if scores.entailment > 0.50 && similarity > 0.80 {
        return Some(("IS_A".into(), scores.entailment * similarity));
    }
    if scores.entailment > 0.40 && similarity > 0.65 {
        return Some(("SUPPORTS".into(), scores.entailment * 0.9));
    }
    if (scores.neutral > 0.60 || scores.entailment > 0.30) && similarity > 0.70 {
        return Some((
            "EXEMPLIFIES".into(),
            (scores.entailment + scores.neutral * 0.5) * similarity,
        ));
    }
    if scores.neutral > 0.70 && similarity > 0.45 && similarity < 0.75 {
        return Some(("RELATES_TO".into(), scores.neutral * similarity));
    }
    None
}

/// Build relation suggestions for a new text against a list of candidates.
/// Each candidate: (id, text, cosine_similarity).
/// Returns suggestions sorted by confidence descending.
pub fn suggest_relations(
    new_text: &str,
    candidates: &[(String, String, f32)],
) -> Vec<RelationSuggestion> {
    let mut suggestions = Vec::new();
    for (id, text, similarity) in candidates {
        match classify(new_text, text) {
            Ok(scores) => {
                if let Some((rel, confidence)) = scores_to_rel(&scores, *similarity) {
                    suggestions.push(RelationSuggestion {
                        candidate_id: id.clone(),
                        candidate_text: text.chars().take(120).collect(),
                        suggested_rel: rel,
                        confidence,
                        nli_entailment: scores.entailment,
                        nli_contradiction: scores.contradiction,
                        similarity: *similarity,
                    });
                }
            }
            Err(e) => {
                tracing::warn!("NLI classify failed for candidate {}: {}", id, e);
            }
        }
    }
    suggestions.sort_by(|a, b| {
        b.confidence
            .partial_cmp(&a.confidence)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    suggestions
}

// ─── Contradiction check ──────────────────────────────────────────────────────

/// Check if `new_text` contradicts any existing text in `candidates`.
/// Returns the first strong contradiction (contradiction score > 0.65).
pub fn check_contradiction(
    new_text: &str,
    candidates: &[(String, String)], // (id, text)
) -> Option<(String, f32)> {
    for (id, text) in candidates {
        if let Ok(scores) = classify(new_text, text) {
            if scores.contradiction > 0.80 {
                return Some((id.clone(), scores.contradiction));
            }
        }
    }
    None
}

// ─── Latency benchmark ────────────────────────────────────────────────────────

/// Benchmark NLI inference latency. Returns average ms over `n` runs.
/// Model must be loaded first.
pub fn benchmark_latency(n: u32) -> Result<f64> {
    if NLI_MODEL.get().is_none() {
        anyhow::bail!("NLI model not loaded. Call ensure_nli_model() first.");
    }
    let premise = "A microservice is a small, independently deployable service.";
    let hypothesis = "A service mesh manages inter-service communication.";

    let start = std::time::Instant::now();
    for _ in 0..n {
        classify(premise, hypothesis)?;
    }
    let elapsed = start.elapsed().as_secs_f64() * 1000.0;
    Ok(elapsed / n as f64)
}

// ─── Paths & status ───────────────────────────────────────────────────────────

pub fn nli_cache_dir() -> PathBuf {
    crate::embeddings::embedding_cache_dir()
        .parent()
        .map(|p| p.join("nli"))
        .unwrap_or_else(|| crate::embeddings::embedding_cache_dir().join("nli"))
}

pub fn nli_model_downloaded() -> bool {
    let dir = nli_cache_dir();
    dir.join("model.onnx").exists() && dir.join("tokenizer.json").exists()
}

// ─── Helpers ──────────────────────────────────────────────────────────────────

fn softmax(logits: &[f32]) -> Vec<f32> {
    let max = logits.iter().cloned().fold(f32::NEG_INFINITY, f32::max);
    let exps: Vec<f32> = logits.iter().map(|&x| (x - max).exp()).collect();
    let sum: f32 = exps.iter().sum();
    exps.iter().map(|&x| x / sum).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_classify_contradiction() {
        ensure_nli_model().await.expect("model load");
        // Two unrelated statements — should score high contradiction or neutral
        let s = classify(
            "A microservice is a small, independently deployable service.",
            "A service mesh manages inter-service communication.",
        )
        .unwrap();
        // Model sees these as unrelated (contradiction in NLI terms for this model)
        assert!(
            s.contradiction > 0.5 || s.neutral > 0.5,
            "should not strongly entail"
        );
    }

    #[tokio::test]
    async fn test_model_inputs() {
        ensure_nli_model().await.expect("model load");
        let wrapper = NLI_MODEL.get().unwrap();
        let model = &wrapper.0;
        let session = model.session.lock().unwrap();
        let input_names: Vec<&str> = session.inputs().iter().map(|i| i.name()).collect();
        assert!(input_names.contains(&"input_ids"), "must have input_ids");
        assert!(
            input_names.contains(&"attention_mask"),
            "must have attention_mask"
        );
        assert!(
            !input_names.contains(&"token_type_ids"),
            "DeBERTa has no token_type_ids"
        );
    }

    #[tokio::test]
    async fn test_scores_to_rel_contradiction() {
        ensure_nli_model().await.expect("model load");
        let s = classify(
            "The server is always online and never goes down.",
            "The server experiences frequent outages and downtime.",
        )
        .unwrap();
        let rel = scores_to_rel(&s, 0.85);
        assert_eq!(rel.map(|(r, _)| r), Some("CONTRADICTS".to_string()));
    }
}
