use anyhow::Result;
use sqlx::SqlitePool;

/// Get or create a graph node for a memory_id.
pub async fn upsert_node(pool: &SqlitePool, memory_id: &str) -> Result<i64> {
    sqlx::query("INSERT OR IGNORE INTO graph_nodes (memory_id) VALUES (?)")
        .bind(memory_id)
        .execute(pool)
        .await?;
    let id: i64 = sqlx::query_scalar("SELECT id FROM graph_nodes WHERE memory_id = ?")
        .bind(memory_id)
        .fetch_one(pool)
        .await?;
    Ok(id)
}

/// Delete a graph node and all its edges (cascade via FK).
pub async fn delete_node(pool: &SqlitePool, memory_id: &str) -> Result<()> {
    sqlx::query("DELETE FROM graph_nodes WHERE memory_id = ?")
        .bind(memory_id)
        .execute(pool)
        .await?;
    Ok(())
}

/// Create an edge between two memory_ids. Returns the edge id.
pub async fn upsert_edge(
    pool: &SqlitePool,
    from_memory_id: &str,
    to_memory_id: &str,
    rel_type: &str,
    note: Option<&str>,
) -> Result<i64> {
    let from_node = upsert_node(pool, from_memory_id).await?;
    let to_node = upsert_node(pool, to_memory_id).await?;
    let now = chrono::Utc::now().to_rfc3339();

    sqlx::query(
        "INSERT OR IGNORE INTO graph_edges (source_id, target_id, rel_type, note, created_at)
         VALUES (?, ?, ?, ?, ?)",
    )
    .bind(from_node)
    .bind(to_node)
    .bind(rel_type)
    .bind(note)
    .bind(&now)
    .execute(pool)
    .await?;

    let edge_id: i64 = sqlx::query_scalar(
        "SELECT id FROM graph_edges WHERE source_id = ? AND target_id = ? AND rel_type = ?",
    )
    .bind(from_node)
    .bind(to_node)
    .bind(rel_type)
    .fetch_one(pool)
    .await?;

    Ok(edge_id)
}

/// Delete a specific edge between two memories.
pub async fn delete_edge(
    pool: &SqlitePool,
    from_memory_id: &str,
    rel_type: &str,
    to_memory_id: &str,
) -> Result<bool> {
    let from_node: Option<i64> =
        sqlx::query_scalar("SELECT id FROM graph_nodes WHERE memory_id = ?")
            .bind(from_memory_id)
            .fetch_optional(pool)
            .await?;

    let to_node: Option<i64> = sqlx::query_scalar("SELECT id FROM graph_nodes WHERE memory_id = ?")
        .bind(to_memory_id)
        .fetch_optional(pool)
        .await?;

    match (from_node, to_node) {
        (Some(f), Some(t)) => {
            let r = sqlx::query(
                "DELETE FROM graph_edges WHERE source_id = ? AND target_id = ? AND rel_type = ?",
            )
            .bind(f)
            .bind(t)
            .bind(rel_type)
            .execute(pool)
            .await?;
            Ok(r.rows_affected() > 0)
        }
        _ => Ok(false),
    }
}
