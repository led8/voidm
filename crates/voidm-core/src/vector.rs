use anyhow::{Context, Result};
use sqlx::SqlitePool;

/// Ensure vec_memories virtual table exists with the correct dimension.
/// Creates it if absent, drops and recreates if dimension mismatches.
pub async fn ensure_vector_table(pool: &SqlitePool, dim: usize) -> Result<()> {
    let existing: Option<String> = sqlx::query_scalar(
        "SELECT sql FROM sqlite_master WHERE type='table' AND name='vec_memories'",
    )
    .fetch_optional(pool)
    .await?;

    if let Some(ddl) = existing {
        // Check if dimension matches
        let expected = format!("float[{}]", dim);
        if ddl.to_lowercase().contains(&expected.to_lowercase()) {
            return Ok(()); // Already correct
        }
        tracing::warn!("vec_memories dimension mismatch, dropping and recreating");
        sqlx::query("DROP TABLE IF EXISTS vec_memories")
            .execute(pool)
            .await?;
    }

    let sql = format!(
        "CREATE VIRTUAL TABLE vec_memories USING vec0(memory_id TEXT, embedding float[{}])",
        dim
    );
    sqlx::query(&sql)
        .execute(pool)
        .await
        .context("Failed to create vec_memories virtual table")?;

    tracing::info!("Created vec_memories with {} dimensions", dim);
    Ok(())
}

/// Store an embedding for a memory. Returns Ok(()) if vec_memories doesn't exist yet (graceful).
pub async fn store_embedding(pool: &SqlitePool, memory_id: &str, embedding: &[f32]) -> Result<()> {
    let bytes = floats_to_bytes(embedding);
    sqlx::query(
        "INSERT INTO vec_memories (memory_id, embedding) VALUES (?, ?) ON CONFLICT(memory_id) DO UPDATE SET embedding = excluded.embedding"
    )
    .bind(memory_id)
    .bind(&bytes)
    .execute(pool)
    .await
    .context("Failed to store embedding")?;
    Ok(())
}

/// Delete an embedding when a memory is deleted.
pub async fn delete_embedding(pool: &SqlitePool, memory_id: &str) -> Result<()> {
    // vec0 tables don't support DELETE directly in all versions; use DELETE WHERE
    let _ = sqlx::query("DELETE FROM vec_memories WHERE memory_id = ?")
        .bind(memory_id)
        .execute(pool)
        .await;
    Ok(())
}

/// Check if vec_memories table exists.
pub async fn vec_table_exists(pool: &SqlitePool) -> Result<bool> {
    let exists: Option<String> = sqlx::query_scalar(
        "SELECT name FROM sqlite_master WHERE type='table' AND name='vec_memories'",
    )
    .fetch_optional(pool)
    .await?;
    Ok(exists.is_some())
}

/// ANN search: returns (memory_id, distance) pairs, closest first.
/// distance is cosine distance (0 = identical, 1 = orthogonal).
pub async fn ann_search(
    pool: &SqlitePool,
    query_embedding: &[f32],
    limit: usize,
) -> Result<Vec<(String, f32)>> {
    let bytes = floats_to_bytes(query_embedding);
    let rows: Vec<(String, f32)> = sqlx::query_as(
        "SELECT memory_id, distance FROM vec_memories WHERE embedding MATCH ? AND k = ?",
    )
    .bind(&bytes)
    .bind(limit as i64)
    .fetch_all(pool)
    .await
    .context("ANN search failed")?;
    Ok(rows)
}

/// Re-embed all memories atomically: create vec_memories_new, populate, then swap.
pub async fn reembed_all(
    pool: &SqlitePool,
    model_name: &str,
    new_dim: usize,
    batch_size: usize,
) -> Result<()> {
    use crate::embeddings;

    // Clean up stale temp table if present
    sqlx::query("DROP TABLE IF EXISTS vec_memories_new")
        .execute(pool)
        .await?;

    // Create new table
    let sql = format!(
        "CREATE VIRTUAL TABLE vec_memories_new USING vec0(memory_id TEXT, embedding float[{}])",
        new_dim
    );
    sqlx::query(&sql).execute(pool).await?;

    // Fetch all memory IDs and content
    let memories: Vec<(String, String)> =
        sqlx::query_as("SELECT id, content FROM memories ORDER BY rowid")
            .fetch_all(pool)
            .await?;

    let total = memories.len();
    tracing::info!("Re-embedding {} memories with {}", total, model_name);

    for (i, chunk) in memories.chunks(batch_size).enumerate() {
        let contents: Vec<String> = chunk.iter().map(|(_, c)| c.clone()).collect();
        let embeddings = embeddings::embed_batch(model_name, &contents)?;

        for ((id, _), embedding) in chunk.iter().zip(embeddings.iter()) {
            let bytes = floats_to_bytes(embedding);
            sqlx::query("INSERT INTO vec_memories_new (memory_id, embedding) VALUES (?, ?)")
                .bind(id)
                .bind(&bytes)
                .execute(pool)
                .await?;
        }
        tracing::info!(
            "Re-embedded batch {}/{}",
            i + 1,
            (total + batch_size - 1) / batch_size
        );
    }

    // Atomic swap
    let mut tx = pool.begin().await?;
    sqlx::query("DROP TABLE IF EXISTS vec_memories")
        .execute(&mut *tx)
        .await?;
    sqlx::query("ALTER TABLE vec_memories_new RENAME TO vec_memories")
        .execute(&mut *tx)
        .await?;
    sqlx::query("INSERT OR REPLACE INTO db_meta (key, value) VALUES ('embedding_model', ?)")
        .bind(model_name)
        .execute(&mut *tx)
        .await?;
    sqlx::query("INSERT OR REPLACE INTO db_meta (key, value) VALUES ('embedding_dim', ?)")
        .bind(new_dim.to_string())
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;

    tracing::info!("Re-embedding complete");
    Ok(())
}

fn floats_to_bytes(floats: &[f32]) -> Vec<u8> {
    floats.iter().flat_map(|f| f.to_le_bytes()).collect()
}

/// Check for stale vec_memories_new from interrupted reembed, and clean it up.
pub async fn cleanup_stale_temp_table(pool: &SqlitePool) -> Result<()> {
    let exists: Option<String> = sqlx::query_scalar(
        "SELECT name FROM sqlite_master WHERE type='table' AND name='vec_memories_new'",
    )
    .fetch_optional(pool)
    .await?;
    if exists.is_some() {
        tracing::warn!("Found stale vec_memories_new from interrupted reembed, cleaning up");
        sqlx::query("DROP TABLE vec_memories_new")
            .execute(pool)
            .await?;
    }
    Ok(())
}
