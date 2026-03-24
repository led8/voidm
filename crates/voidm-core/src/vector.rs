use anyhow::{Context, Result};
use sqlx::{SqliteConnection, SqlitePool};

const VECTOR_TABLE_NAME: &str = "vec_memories";
const LEGACY_REEMBED_TEMP_TABLE_PREFIX: &str = "vec_memories_new";

/// Ensure vec_memories virtual table exists with the correct dimension.
/// Creates it if absent, drops and recreates if dimension mismatches.
pub async fn ensure_vector_table(pool: &SqlitePool, dim: usize) -> Result<()> {
    cleanup_stale_temp_table(pool).await?;

    let existing: Option<String> =
        sqlx::query_scalar("SELECT sql FROM sqlite_master WHERE type='table' AND name = ?")
            .bind(VECTOR_TABLE_NAME)
            .fetch_optional(pool)
            .await?;

    if let Some(ddl) = existing {
        let expected = format!("float[{}]", dim);
        if ddl.to_lowercase().contains(&expected.to_lowercase()) {
            return Ok(());
        }
        tracing::warn!("vec_memories dimension mismatch, dropping and recreating");
        drop_active_vector_table(pool).await?;
    }

    create_vector_table(pool, VECTOR_TABLE_NAME, dim).await?;

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
    sqlite_object_exists(pool, VECTOR_TABLE_NAME).await
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

/// Re-embed all memories by rebuilding vec_memories in place.
pub async fn reembed_all(
    pool: &SqlitePool,
    model_name: &str,
    new_dim: usize,
    batch_size: usize,
) -> Result<()> {
    use crate::embeddings;

    drop_active_vector_table(pool).await?;
    cleanup_stale_temp_table(pool).await?;
    create_vector_table(pool, VECTOR_TABLE_NAME, new_dim)
        .await
        .context("Failed to create vec_memories during reembed")?;

    let result: Result<()> = async {
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
                sqlx::query("INSERT INTO vec_memories (memory_id, embedding) VALUES (?, ?)")
                    .bind(id)
                    .bind(&bytes)
                    .execute(pool)
                    .await
                    .with_context(|| {
                        format!(
                            "Failed to store embedding for memory '{}' during reembed",
                            id
                        )
                    })?;
            }
            tracing::info!(
                "Re-embedded batch {}/{}",
                i + 1,
                (total + batch_size - 1) / batch_size
            );
        }

        sqlx::query("INSERT OR REPLACE INTO db_meta (key, value) VALUES ('embedding_model', ?)")
            .bind(model_name)
            .execute(pool)
            .await
            .context("Failed to update embedding_model metadata during reembed")?;
        sqlx::query("INSERT OR REPLACE INTO db_meta (key, value) VALUES ('embedding_dim', ?)")
            .bind(new_dim.to_string())
            .execute(pool)
            .await
            .context("Failed to update embedding_dim metadata during reembed")?;

        tracing::info!("Re-embedding complete");
        Ok(())
    }
    .await;

    if let Err(err) = result {
        if let Err(cleanup_err) = reset_vector_table_after_reembed_failure(pool, new_dim).await {
            tracing::warn!(
                "Failed to reset vec_memories after re-embed error: {}",
                cleanup_err
            );
        }
        return Err(err);
    }

    Ok(())
}

fn floats_to_bytes(floats: &[f32]) -> Vec<u8> {
    floats.iter().flat_map(|f| f.to_le_bytes()).collect()
}

/// Check for stale vec_memories_new* artifacts from older reembed runs.
///
/// When vec_memories exists and vec_memories_new does not, legacy
/// vec_memories_new_* shadow tables may still back the live virtual table.
/// In that case, leave them alone until vec_memories is dropped explicitly.
pub async fn cleanup_stale_temp_table(pool: &SqlitePool) -> Result<()> {
    let current_exists = sqlite_object_exists(pool, VECTOR_TABLE_NAME).await?;
    let temp_base_exists = sqlite_object_exists(pool, LEGACY_REEMBED_TEMP_TABLE_PREFIX).await?;

    if current_exists && !temp_base_exists {
        return Ok(());
    }

    let cleaned = cleanup_objects_with_prefix(pool, LEGACY_REEMBED_TEMP_TABLE_PREFIX).await?;
    if cleaned > 0 {
        tracing::warn!(
            "Found {} stale {}* object(s) from interrupted reembed, cleaned them up",
            cleaned,
            LEGACY_REEMBED_TEMP_TABLE_PREFIX
        );
    }
    Ok(())
}

async fn create_vector_table(pool: &SqlitePool, table_name: &str, dim: usize) -> Result<()> {
    let sql = format!(
        "CREATE VIRTUAL TABLE {} USING vec0(memory_id TEXT, embedding float[{}])",
        quote_sqlite_ident(table_name),
        dim
    );
    sqlx::query(&sql)
        .execute(pool)
        .await
        .with_context(|| format!("Failed to create {} virtual table", table_name))?;
    Ok(())
}

async fn sqlite_object_exists(pool: &SqlitePool, name: &str) -> Result<bool> {
    let exists: Option<String> =
        sqlx::query_scalar("SELECT name FROM sqlite_master WHERE name = ?")
            .bind(name)
            .fetch_optional(pool)
            .await?;
    Ok(exists.is_some())
}

async fn drop_active_vector_table(pool: &SqlitePool) -> Result<()> {
    if sqlite_object_exists(pool, VECTOR_TABLE_NAME).await? {
        let drop_sql = format!(
            "DROP TABLE IF EXISTS {}",
            quote_sqlite_ident(VECTOR_TABLE_NAME)
        );
        match sqlx::query(&drop_sql).execute(pool).await {
            Ok(_) => {}
            Err(err) => {
                tracing::warn!(
                    "Normal DROP TABLE for {} failed ({}); forcing schema cleanup",
                    VECTOR_TABLE_NAME,
                    err
                );
                let removed_current =
                    force_remove_objects_with_prefix_from_schema(pool, VECTOR_TABLE_NAME).await?;
                let removed_legacy = force_remove_objects_with_prefix_from_schema(
                    pool,
                    LEGACY_REEMBED_TEMP_TABLE_PREFIX,
                )
                .await?;
                if removed_current == 0 && removed_legacy == 0 {
                    return Err(err).context(
                        "Failed to drop broken vec_memories and no fallback cleanup removed objects",
                    );
                }
            }
        }
    }

    cleanup_objects_with_prefix(pool, VECTOR_TABLE_NAME).await?;
    cleanup_objects_with_prefix(pool, LEGACY_REEMBED_TEMP_TABLE_PREFIX).await?;
    Ok(())
}

async fn reset_vector_table_after_reembed_failure(pool: &SqlitePool, new_dim: usize) -> Result<()> {
    drop_active_vector_table(pool).await?;
    cleanup_stale_temp_table(pool).await?;
    create_vector_table(pool, VECTOR_TABLE_NAME, new_dim).await?;
    Ok(())
}

async fn cleanup_objects_with_prefix(pool: &SqlitePool, prefix: &str) -> Result<usize> {
    let objects = {
        let mut conn = pool.acquire().await?;
        list_sqlite_objects_with_prefix(&mut conn, prefix).await?
    };

    for (name, object_type) in &objects {
        let sql = format!(
            "DROP {} IF EXISTS {}",
            sqlite_drop_keyword(object_type),
            quote_sqlite_ident(name)
        );
        if let Err(err) = sqlx::query(&sql).execute(pool).await {
            tracing::warn!(
                "Normal drop failed for SQLite object '{}' ({}); forcing schema cleanup for prefix '{}'",
                name,
                err,
                prefix
            );
            force_remove_objects_with_prefix_from_schema(pool, prefix).await?;
            return Ok(objects.len());
        }
    }

    Ok(objects.len())
}

async fn force_remove_objects_with_prefix_from_schema(
    pool: &SqlitePool,
    prefix: &str,
) -> Result<usize> {
    let mut conn = pool.acquire().await?;
    let objects = list_sqlite_schema_rows_for_force_cleanup(&mut conn, prefix).await?;
    if objects.is_empty() {
        return Ok(0);
    }

    let schema_version: i64 = sqlx::query_scalar("PRAGMA schema_version")
        .fetch_one(&mut *conn)
        .await?;
    sqlx::query("PRAGMA writable_schema=ON")
        .execute(&mut *conn)
        .await?;

    let delete_result: Result<()> = async {
        for (name, _) in &objects {
            sqlx::query("DELETE FROM sqlite_master WHERE name = ?")
                .bind(name)
                .execute(&mut *conn)
                .await?;
        }
        sqlx::query(&format!("PRAGMA schema_version = {}", schema_version + 1))
            .execute(&mut *conn)
            .await?;
        Ok(())
    }
    .await;

    let disable_result = sqlx::query("PRAGMA writable_schema=OFF")
        .execute(&mut *conn)
        .await;
    disable_result.context("Failed to disable writable_schema after vector cleanup")?;
    delete_result?;

    Ok(objects.len())
}

async fn list_sqlite_objects_with_prefix(
    conn: &mut SqliteConnection,
    prefix: &str,
) -> Result<Vec<(String, String)>> {
    let pattern = format!("{}\\_%", escape_sqlite_like(prefix));
    let objects: Vec<(String, String)> = sqlx::query_as(
        r#"
        SELECT name, type
        FROM sqlite_master
        WHERE type IN ('table', 'index', 'view', 'trigger')
          AND (name = ? OR name LIKE ? ESCAPE '\')
        ORDER BY CASE WHEN name = ? THEN 0 ELSE 1 END, name
        "#,
    )
    .bind(prefix)
    .bind(pattern)
    .bind(prefix)
    .fetch_all(&mut *conn)
    .await?;

    Ok(objects)
}

async fn list_sqlite_schema_rows_for_force_cleanup(
    conn: &mut SqliteConnection,
    prefix: &str,
) -> Result<Vec<(String, String)>> {
    let pattern = format!("{}\\_%", escape_sqlite_like(prefix));
    let objects: Vec<(String, String)> = sqlx::query_as(
        r#"
        SELECT DISTINCT name, type
        FROM sqlite_master
        WHERE type IN ('table', 'index', 'view', 'trigger')
          AND (
                name = ?
             OR name LIKE ? ESCAPE '\'
             OR tbl_name = ?
             OR tbl_name LIKE ? ESCAPE '\'
          )
        ORDER BY CASE WHEN name = ? THEN 0 ELSE 1 END, name
        "#,
    )
    .bind(prefix)
    .bind(&pattern)
    .bind(prefix)
    .bind(&pattern)
    .bind(prefix)
    .fetch_all(&mut *conn)
    .await?;

    Ok(objects)
}

fn escape_sqlite_like(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len());
    for ch in value.chars() {
        match ch {
            '\\' | '%' | '_' => {
                escaped.push('\\');
                escaped.push(ch);
            }
            _ => escaped.push(ch),
        }
    }
    escaped
}

fn sqlite_drop_keyword(object_type: &str) -> &'static str {
    match object_type {
        "index" => "INDEX",
        "view" => "VIEW",
        "trigger" => "TRIGGER",
        _ => "TABLE",
    }
}

fn quote_sqlite_ident(name: &str) -> String {
    format!("\"{}\"", name.replace('"', "\"\""))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::sqlite::open_sqlite_pool;
    use crate::models::{AddMemoryRequest, MemoryType};
    use crate::Config;
    use anyhow::Result;

    async fn create_test_pool() -> Result<SqlitePool> {
        let pool = open_sqlite_pool(":memory:").await?;
        crate::migrate::run(&pool).await?;
        Ok(pool)
    }

    #[tokio::test]
    async fn cleanup_stale_temp_table_removes_shadow_tables_without_base() -> Result<()> {
        let pool = create_test_pool().await?;

        sqlx::query("CREATE TABLE vec_memories_new_info (id INTEGER)")
            .execute(&pool)
            .await?;
        sqlx::query("CREATE TABLE vec_memories_new_rowids (id INTEGER)")
            .execute(&pool)
            .await?;
        sqlx::query("CREATE TABLE vec_memories_newish (id INTEGER)")
            .execute(&pool)
            .await?;

        cleanup_stale_temp_table(&pool).await?;

        let stale_count: i64 = sqlx::query_scalar(
            r#"
            SELECT COUNT(*)
            FROM sqlite_master
            WHERE name = 'vec_memories_new'
               OR name LIKE 'vec\_memories\_new\_%' ESCAPE '\'
            "#,
        )
        .fetch_one(&pool)
        .await?;
        let unrelated_exists: Option<String> =
            sqlx::query_scalar("SELECT name FROM sqlite_master WHERE name = 'vec_memories_newish'")
                .fetch_optional(&pool)
                .await?;

        assert_eq!(stale_count, 0);
        assert_eq!(unrelated_exists.as_deref(), Some("vec_memories_newish"));
        Ok(())
    }

    #[tokio::test]
    async fn cleanup_stale_temp_table_preserves_legacy_shadows_for_live_table() -> Result<()> {
        let pool = create_test_pool().await?;

        sqlx::query("CREATE TABLE vec_memories (id INTEGER)")
            .execute(&pool)
            .await?;
        sqlx::query("CREATE TABLE vec_memories_new_info (id INTEGER)")
            .execute(&pool)
            .await?;
        sqlx::query("CREATE TABLE vec_memories_new_rowids (id INTEGER)")
            .execute(&pool)
            .await?;

        cleanup_stale_temp_table(&pool).await?;

        let legacy_shadow_count: i64 = sqlx::query_scalar(
            r#"
            SELECT COUNT(*)
            FROM sqlite_master
            WHERE name LIKE 'vec\_memories\_new\_%' ESCAPE '\'
            "#,
        )
        .fetch_one(&pool)
        .await?;

        assert_eq!(legacy_shadow_count, 2);
        Ok(())
    }

    #[tokio::test]
    async fn reembed_all_resets_table_after_embedding_failure() -> Result<()> {
        let pool = create_test_pool().await?;
        let mut config = Config::default();
        config.embeddings.enabled = false;

        crate::crud::add_memory(
            &pool,
            AddMemoryRequest {
                id: Some("reembed-cleanup-test".to_string()),
                content: "test memory".to_string(),
                memory_type: MemoryType::Semantic,
                scopes: vec!["voidm".to_string()],
                tags: vec![],
                importance: 5,
                metadata: serde_json::json!({}),
                links: vec![],
            },
            &config,
        )
        .await?;

        let err = reembed_all(&pool, "definitely-not-a-real-model", 384, 8)
            .await
            .expect_err("reembed should fail for an unknown model");
        assert!(
            err.to_string().contains("Unknown embedding model"),
            "unexpected error: {err}"
        );

        let stale_count: i64 = sqlx::query_scalar(
            r#"
            SELECT COUNT(*)
            FROM sqlite_master
            WHERE name = 'vec_memories_new'
               OR name LIKE 'vec\_memories\_new\_%' ESCAPE '\'
            "#,
        )
        .fetch_one(&pool)
        .await?;

        assert_eq!(stale_count, 0);
        assert!(vec_table_exists(&pool).await?);
        Ok(())
    }

    #[tokio::test]
    async fn reembed_all_recovers_from_broken_current_vec_table() -> Result<()> {
        let pool = create_test_pool().await?;

        create_vector_table(&pool, VECTOR_TABLE_NAME, 384).await?;
        remove_shadow_entries_from_schema_for_test(&pool, VECTOR_TABLE_NAME).await?;

        reembed_all(&pool, "Xenova/all-MiniLM-L6-v2", 384, 8).await?;

        let current_exists = sqlite_object_exists(&pool, VECTOR_TABLE_NAME).await?;
        let current_shadow_count: i64 = sqlx::query_scalar(
            r#"
            SELECT COUNT(*)
            FROM sqlite_master
            WHERE name LIKE 'vec\_memories\_%' ESCAPE '\'
            "#,
        )
        .fetch_one(&pool)
        .await?;
        let legacy_shadow_count: i64 = sqlx::query_scalar(
            r#"
            SELECT COUNT(*)
            FROM sqlite_master
            WHERE name LIKE 'vec\_memories\_new\_%' ESCAPE '\'
            "#,
        )
        .fetch_one(&pool)
        .await?;

        assert!(current_exists);
        assert!(current_shadow_count > 0);
        assert_eq!(legacy_shadow_count, 0);
        Ok(())
    }

    async fn remove_shadow_entries_from_schema_for_test(
        pool: &SqlitePool,
        prefix: &str,
    ) -> Result<()> {
        let mut conn = pool.acquire().await?;
        let schema_version: i64 = sqlx::query_scalar("PRAGMA schema_version")
            .fetch_one(&mut *conn)
            .await?;
        let pattern = format!("{}\\_%", escape_sqlite_like(prefix));

        sqlx::query("PRAGMA writable_schema=ON")
            .execute(&mut *conn)
            .await?;
        let delete_result: Result<()> = async {
            sqlx::query(
                "DELETE FROM sqlite_master WHERE name LIKE ? ESCAPE '\\' OR tbl_name LIKE ? ESCAPE '\\'",
            )
                .bind(&pattern)
                .bind(&pattern)
                .execute(&mut *conn)
                .await?;
            sqlx::query(&format!("PRAGMA schema_version = {}", schema_version + 1))
                .execute(&mut *conn)
                .await?;
            Ok(())
        }
        .await;
        let disable_result = sqlx::query("PRAGMA writable_schema=OFF")
            .execute(&mut *conn)
            .await;
        disable_result?;
        delete_result?;

        Ok(())
    }
}
