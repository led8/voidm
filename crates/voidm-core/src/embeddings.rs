use anyhow::{Context, Result};
use once_cell::sync::OnceCell;
use std::sync::Mutex;

static EMBEDDER: OnceCell<Mutex<fastembed::TextEmbedding>> = OnceCell::new();

/// Initialize the embedding model (lazy, called on first use).
pub fn get_embedder(model_name: &str) -> Result<&'static Mutex<fastembed::TextEmbedding>> {
    EMBEDDER.get_or_try_init(|| {
        let embedder = init_embedder(model_name)?;
        Ok(Mutex::new(embedder))
    })
}

fn init_embedder(model_name: &str) -> Result<fastembed::TextEmbedding> {
    use fastembed::{InitOptions, TextEmbedding};

    let model = model_from_name(model_name)?;
    let cache_dir = embedding_cache_dir();

    tracing::info!(
        "Loading embedding model {} from {}",
        model_name,
        cache_dir.display()
    );

    let opts = InitOptions::new(model)
        .with_cache_dir(cache_dir)
        .with_show_download_progress(true);

    TextEmbedding::try_new(opts)
        .with_context(|| format!("Failed to load embedding model '{}'", model_name))
}

fn model_from_name(name: &str) -> Result<fastembed::EmbeddingModel> {
    use fastembed::EmbeddingModel::*;
    match name {
        "Xenova/all-MiniLM-L6-v2" | "all-MiniLM-L6-v2" => Ok(AllMiniLML6V2),
        "BAAI/bge-small-en-v1.5" | "bge-small-en-v1.5" => Ok(BGESmallENV15),
        "BAAI/bge-base-en-v1.5" | "bge-base-en-v1.5" => Ok(BGEBaseENV15),
        "BAAI/bge-large-en-v1.5" | "bge-large-en-v1.5" => Ok(BGELargeENV15),
        "nomic-embed-text-v1" | "NomicEmbedTextV1" => Ok(NomicEmbedTextV1),
        "nomic-embed-text-v1.5" | "NomicEmbedTextV15" => Ok(NomicEmbedTextV15),
        "mxbai-embed-large-v1" | "MxbaiEmbedLargeV1" => Ok(MxbaiEmbedLargeV1),
        "multilingual-e5-small" => Ok(MultilingualE5Small),
        "multilingual-e5-base" => Ok(MultilingualE5Base),
        "multilingual-e5-large" => Ok(MultilingualE5Large),
        other => Err(anyhow::anyhow!(
            "Unknown embedding model: '{}'. Run 'voidm models list' to see available models.",
            other
        )),
    }
}

pub fn embedding_cache_dir() -> std::path::PathBuf {
    if let Ok(xdg) = std::env::var("XDG_CACHE_HOME") {
        if !xdg.is_empty() {
            return std::path::PathBuf::from(xdg).join("voidm/models");
        }
    }
    dirs::home_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join(".cache/voidm/models")
}

/// Embed a single text, returns the float vector.
pub fn embed_text(model_name: &str, text: &str) -> Result<Vec<f32>> {
    let embedder_lock = get_embedder(model_name)?;
    let mut embedder = embedder_lock.lock().unwrap();
    let mut results = embedder.embed(vec![text.to_string()], None)?;
    results.pop().context("Embedding returned empty result")
}

/// Embed multiple texts in one batch.
pub fn embed_batch(model_name: &str, texts: &[String]) -> Result<Vec<Vec<f32>>> {
    let embedder_lock = get_embedder(model_name)?;
    let mut embedder = embedder_lock.lock().unwrap();
    embedder
        .embed(texts.to_vec(), None)
        .context("Batch embedding failed")
}

/// List available models.
pub fn list_models() -> Vec<ModelInfo> {
    vec![
        ModelInfo {
            name: "Xenova/all-MiniLM-L6-v2".into(),
            dims: 384,
            description: "Fast, compact. Default.".into(),
        },
        ModelInfo {
            name: "BAAI/bge-small-en-v1.5".into(),
            dims: 384,
            description: "BGE small, English.".into(),
        },
        ModelInfo {
            name: "BAAI/bge-base-en-v1.5".into(),
            dims: 768,
            description: "BGE base, English.".into(),
        },
        ModelInfo {
            name: "BAAI/bge-large-en-v1.5".into(),
            dims: 1024,
            description: "BGE large, English.".into(),
        },
        ModelInfo {
            name: "nomic-embed-text-v1.5".into(),
            dims: 768,
            description: "Nomic, long context.".into(),
        },
        ModelInfo {
            name: "mxbai-embed-large-v1".into(),
            dims: 1024,
            description: "MxBAI large, best quality.".into(),
        },
        ModelInfo {
            name: "multilingual-e5-base".into(),
            dims: 768,
            description: "Multilingual.".into(),
        },
    ]
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct ModelInfo {
    pub name: String,
    pub dims: usize,
    pub description: String,
}
