//! NER-based entity extraction for ontology enrichment.
//!
//! Uses `Xenova/bert-base-NER` (quantized ONNX, ~103MB) to extract named entities
//! from raw text, mapping them to candidate ontology concepts.
//!
//! Labels: B-PER, I-PER, B-ORG, I-ORG, B-LOC, I-LOC, B-MISC, I-MISC, O
//! Model downloaded on first use via hf-hub, cached alongside NLI model.

use anyhow::{Context, Result};
use once_cell::sync::OnceCell;
use ort::session::Session;
use ort::value::Tensor;
use std::path::PathBuf;

// ─── Public types ─────────────────────────────────────────────────────────────

/// A named entity extracted from text.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct NamedEntity {
    /// Surface form as it appears in the text
    pub text: String,
    /// CoNLL-03 entity type: PER, ORG, LOC, MISC
    pub entity_type: String,
    /// Confidence score (mean token probability over the span)
    pub score: f32,
    /// Character offset start
    pub start: usize,
    /// Character offset end
    pub end: usize,
}

/// A candidate concept derived from a named entity.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ConceptCandidate {
    pub name: String,
    pub entity_type: String,
    pub score: f32,
    /// True if a concept with this name already exists in the DB
    pub already_exists: bool,
    pub existing_id: Option<String>,
}

// ─── Label mapping ────────────────────────────────────────────────────────────

// CoNLL-03 id2label in model order
const LABELS: &[&str] = &[
    "O", "B-MISC", "I-MISC", "B-PER", "I-PER", "B-ORG", "I-ORG", "B-LOC", "I-LOC",
];

fn label_to_type(label: &str) -> Option<&'static str> {
    match label {
        "B-PER" | "I-PER" => Some("PER"),
        "B-ORG" | "I-ORG" => Some("ORG"),
        "B-LOC" | "I-LOC" => Some("LOC"),
        "B-MISC" | "I-MISC" => Some("MISC"),
        _ => None,
    }
}

fn is_begin(label: &str) -> bool {
    label.starts_with('B')
}

use std::sync::Mutex;

// ─── Model state ──────────────────────────────────────────────────────────────

struct NerModel {
    session: Mutex<Session>,
    tokenizer: tokenizers::Tokenizer,
}

struct SendNer(NerModel);
unsafe impl Send for SendNer {}
unsafe impl Sync for SendNer {}

static NER_MODEL: OnceCell<SendNer> = OnceCell::new();

const NER_MODEL_ID: &str = "Xenova/bert-base-NER";
const NER_ONNX_FILE: &str = "onnx/model_quantized.onnx";
const NER_TOKENIZER_FILE: &str = "tokenizer.json";

// ─── Init & download ──────────────────────────────────────────────────────────

pub async fn ensure_ner_model() -> Result<()> {
    if NER_MODEL.get().is_some() {
        return Ok(());
    }
    let model = load_or_download().await?;
    let _ = NER_MODEL.set(SendNer(model));
    Ok(())
}

async fn load_or_download() -> Result<NerModel> {
    let cache_dir = ner_cache_dir();
    std::fs::create_dir_all(&cache_dir)
        .with_context(|| format!("Cannot create NER cache dir: {}", cache_dir.display()))?;

    let onnx_path = cache_dir.join("model_quantized.onnx");
    let tokenizer_path = cache_dir.join("tokenizer.json");

    if !onnx_path.exists() || !tokenizer_path.exists() {
        tracing::info!("Downloading NER model '{}' (first use) …", NER_MODEL_ID);
        eprintln!(
            "Downloading NER model '{}' (~103MB, first use only) …",
            NER_MODEL_ID
        );
        download_model_files(&cache_dir).await?;
        eprintln!("NER model ready at {}", cache_dir.display());
    }

    let tokenizer = tokenizers::Tokenizer::from_file(&tokenizer_path)
        .map_err(|e| anyhow::anyhow!("Failed to load NER tokenizer: {}", e))?;

    let session = Session::builder()
        .context("Failed to create ort session builder")?
        .commit_from_file(&onnx_path)
        .context("Failed to load NER ONNX model")?;

    Ok(NerModel {
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

    let repo = api.model(NER_MODEL_ID.to_string());

    let onnx_src = repo
        .get(NER_ONNX_FILE)
        .await
        .with_context(|| format!("Failed to download {} from {}", NER_ONNX_FILE, NER_MODEL_ID))?;
    std::fs::copy(&onnx_src, cache_dir.join("model_quantized.onnx"))
        .context("Failed to copy NER ONNX model to cache")?;

    let tok_src = repo.get(NER_TOKENIZER_FILE).await.with_context(|| {
        format!(
            "Failed to download {} from {}",
            NER_TOKENIZER_FILE, NER_MODEL_ID
        )
    })?;
    std::fs::copy(&tok_src, cache_dir.join("tokenizer.json"))
        .context("Failed to copy NER tokenizer to cache")?;

    Ok(())
}

// ─── Inference ────────────────────────────────────────────────────────────────

/// Extract named entities from `text`.
/// Returns a list of entities with their type, span, and confidence score.
pub fn extract_entities(text: &str) -> Result<Vec<NamedEntity>> {
    let wrapper = NER_MODEL
        .get()
        .context("NER model not loaded. Call ensure_ner_model() first.")?;
    let model = &wrapper.0;

    // Tokenize — no truncation, but BERT has a 512 token limit
    let encoding = model
        .tokenizer
        .encode(text, true)
        .map_err(|e| anyhow::anyhow!("NER tokenization failed: {}", e))?;

    let ids = encoding.get_ids();
    let seq_len = ids.len().min(512); // hard cap at BERT max

    let input_ids: Vec<i64> = ids[..seq_len].iter().map(|&x| x as i64).collect();
    let attention_mask: Vec<i64> = encoding.get_attention_mask()[..seq_len]
        .iter()
        .map(|&x| x as i64)
        .collect();
    let token_type_ids: Vec<i64> = encoding.get_type_ids()[..seq_len]
        .iter()
        .map(|&x| x as i64)
        .collect();

    let ids_tensor = Tensor::<i64>::from_array(([1usize, seq_len], input_ids.into_boxed_slice()))
        .context("NER input_ids tensor")?;

    let mask_tensor =
        Tensor::<i64>::from_array(([1usize, seq_len], attention_mask.into_boxed_slice()))
            .context("NER attention_mask tensor")?;

    let type_tensor =
        Tensor::<i64>::from_array(([1usize, seq_len], token_type_ids.into_boxed_slice()))
            .context("NER token_type_ids tensor")?;

    // Perform inference and extract logits within the lock scope
    let logits_flat = {
        let mut session_lock = model.session.lock().unwrap();
        let outputs = session_lock
            .run(ort::inputs![
                "input_ids"      => ids_tensor,
                "attention_mask" => mask_tensor,
                "token_type_ids" => type_tensor
            ])
            .context("NER inference failed")?;

        // logits: [1, seq_len, num_labels=9]
        let logits_value = outputs
            .get("logits")
            .context("No 'logits' output from NER model")?;
        let logits_tensor = logits_value
            .try_extract_tensor::<f32>()
            .context("Failed to extract NER logits")?;
        let (_shape, logits_raw) = logits_tensor;
        logits_raw.to_vec()
    };

    let num_labels = LABELS.len();
    let token_offsets = encoding.get_offsets();

    // Decode per-token labels
    let mut token_labels: Vec<(&str, f32)> = Vec::new();
    for t in 0..seq_len {
        let start = t * num_labels;
        if start + num_labels > logits_flat.len() {
            break;
        }
        let token_logits = &logits_flat[start..start + num_labels];
        let probs = softmax(token_logits);
        let (best_idx, best_prob) = probs
            .iter()
            .enumerate()
            .max_by(|a, b| a.1.partial_cmp(b.1).unwrap_or(std::cmp::Ordering::Equal))
            .map(|(i, p)| (i, *p))
            .unwrap_or((0, 1.0));
        let label = if best_idx < LABELS.len() {
            LABELS[best_idx]
        } else {
            "O"
        };
        token_labels.push((label, best_prob));
    }

    // Aggregate tokens into entity spans using BIO labels + subword continuation.
    //
    // The model assigns B/I labels to root tokens but subword fragments (##X)
    // sometimes get O labels even when they're part of the entity — e.g.
    // "Rabbit ##M ##Q" → [B-ORG, I-ORG, O] but the full span is still "RabbitMQ".
    //
    // Strategy: once a B- span starts, absorb ALL subsequent ## tokens
    // (wordpiece continuations) regardless of their predicted label, then
    // continue absorbing I-XXX tokens. This reconstructs compound tokens
    // like "RabbitMQ", "Docker", "GitHub", "PostgreSQL" correctly.
    let mut entities: Vec<NamedEntity> = Vec::new();
    let mut i = 0usize;

    // We need the raw token strings to detect ## subword continuations
    let tokens: &[String] = encoding.get_tokens();

    while i < token_labels.len() {
        let (label, prob) = token_labels[i];
        if let Some(entity_type) = label_to_type(label) {
            if is_begin(label) {
                // Start of a new entity span
                let char_start = if i < token_offsets.len() {
                    token_offsets[i].0
                } else {
                    0
                };
                let mut char_end = if i < token_offsets.len() {
                    token_offsets[i].1
                } else {
                    0
                };
                let mut scores = vec![prob];

                // Consume continuation tokens:
                // 1. ## subword fragments (always part of this word token)
                // 2. I-XXX tokens with matching entity type
                let mut j = i + 1;
                while j < token_labels.len() {
                    let is_subword = j < tokens.len() && tokens[j].starts_with("##");
                    let (next_label, next_prob) = token_labels[j];
                    let next_type = label_to_type(next_label);
                    let is_continuation = next_type == Some(entity_type) && !is_begin(next_label);

                    if is_subword || is_continuation {
                        char_end = if j < token_offsets.len() {
                            token_offsets[j].1
                        } else {
                            char_end
                        };
                        scores.push(next_prob);
                        j += 1;
                    } else {
                        break;
                    }
                }

                let span_text = text
                    .get(char_start..char_end)
                    .unwrap_or("")
                    .trim()
                    .to_string();

                // Skip special tokens [CLS], [SEP], empty spans, single chars
                if !span_text.is_empty() && span_text.len() > 1 && !span_text.starts_with('[') {
                    let mean_score = scores.iter().sum::<f32>() / scores.len() as f32;
                    entities.push(NamedEntity {
                        text: span_text,
                        entity_type: entity_type.to_string(),
                        score: mean_score,
                        start: char_start,
                        end: char_end,
                    });
                }

                i = j;
                continue;
            }
        }
        i += 1;
    }

    // Deduplicate by normalized text (case-insensitive), keep highest score
    entities.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
    entities.retain(|e| seen.insert(e.text.to_lowercase()));

    Ok(entities)
}

// ─── Concept candidate derivation ─────────────────────────────────────────────

/// Convert extracted entities to concept candidates, checking for existing concepts.
pub async fn entities_to_candidates(
    entities: &[NamedEntity],
    pool: &sqlx::SqlitePool,
) -> Result<Vec<ConceptCandidate>> {
    let mut candidates = Vec::new();

    for entity in entities {
        // Check if a concept with this name already exists (case-insensitive)
        let existing: Option<(String,)> =
            sqlx::query_as("SELECT id FROM ontology_concepts WHERE lower(name) = lower(?)")
                .bind(&entity.text)
                .fetch_optional(pool)
                .await?;

        candidates.push(ConceptCandidate {
            name: entity.text.clone(),
            entity_type: entity.entity_type.clone(),
            score: entity.score,
            already_exists: existing.is_some(),
            existing_id: existing.map(|(id,)| id),
        });
    }

    Ok(candidates)
}

// ─── Paths & status ───────────────────────────────────────────────────────────

pub fn ner_cache_dir() -> PathBuf {
    crate::embeddings::embedding_cache_dir()
        .parent()
        .map(|p| p.join("ner"))
        .unwrap_or_else(|| crate::embeddings::embedding_cache_dir().join("ner"))
}

pub fn ner_model_downloaded() -> bool {
    let dir = ner_cache_dir();
    dir.join("model_quantized.onnx").exists() && dir.join("tokenizer.json").exists()
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
    async fn test_model_inputs() {
        ensure_ner_model().await.expect("model load");
        let wrapper = NER_MODEL.get().unwrap();
        let model = &wrapper.0;
        let session = model.session.lock().unwrap();
        let input_names: Vec<&str> = session.inputs().iter().map(|i| i.name()).collect();
        assert!(input_names.contains(&"input_ids"));
        assert!(input_names.contains(&"attention_mask"));
        assert!(
            input_names.contains(&"token_type_ids"),
            "BERT-NER uses token_type_ids"
        );
    }

    #[tokio::test]
    async fn test_extract_orgs_and_locs() {
        ensure_ner_model().await.expect("model load");
        let entities =
            extract_entities("Google and Microsoft are building cloud services in London.")
                .expect("extraction failed");
        let names: Vec<&str> = entities.iter().map(|e| e.text.as_str()).collect();
        assert!(
            names.contains(&"Google"),
            "should extract Google: got {:?}",
            names
        );
        assert!(
            names.contains(&"Microsoft"),
            "should extract Microsoft: got {:?}",
            names
        );
        assert!(
            names.contains(&"London"),
            "should extract London: got {:?}",
            names
        );
    }

    #[tokio::test]
    async fn test_extract_compound_tokens() {
        ensure_ner_model().await.expect("model load");
        // These are the known-problematic compound names that caused truncation
        // before the ## subword-aware span stitching fix.
        let entities = extract_entities(
            "Netflix uses RabbitMQ and PostgreSQL. Docker and GitHub are key tools. Stripe processes payments."
        ).expect("extraction failed");
        let names: Vec<&str> = entities.iter().map(|e| e.text.as_str()).collect();
        println!("Extracted: {:?}", names);
        // Full compound forms — not truncated
        assert!(
            names.contains(&"RabbitMQ"),
            "should reconstruct RabbitMQ (was 'Rabbit'): got {:?}",
            names
        );
        assert!(
            names.contains(&"Docker"),
            "should reconstruct Docker (was 'Dock'): got {:?}",
            names
        );
        assert!(
            names.contains(&"GitHub"),
            "should reconstruct GitHub: got {:?}",
            names
        );
        assert!(
            names.contains(&"Netflix"),
            "should extract Netflix: got {:?}",
            names
        );
    }

    #[tokio::test]
    async fn test_extract_empty() {
        ensure_ner_model().await.expect("model load");
        let entities = extract_entities("the quick brown fox").expect("extraction failed");
        // No proper entities in this text
        assert!(entities.is_empty() || entities.iter().all(|e| e.score < 0.9));
    }
}
