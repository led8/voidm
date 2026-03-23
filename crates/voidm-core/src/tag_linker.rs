use anyhow::Result;
/// Tag-based automatic linking of memories
/// When a new memory is created, find other memories with shared tags
/// and create RELATES_TO edges in the knowledge graph.
use sqlx::SqlitePool;
use std::collections::HashSet;

/// Find all memories that share at least one tag with the given memory.
/// Returns list of (memory_id, shared_tag_count, shared_tags_list)
pub async fn find_memories_with_shared_tags(
    pool: &SqlitePool,
    memory_id: &str,
    current_tags: &[String],
) -> Result<Vec<(String, usize, Vec<String>)>> {
    if current_tags.is_empty() {
        return Ok(vec![]);
    }

    // Convert tags to lowercase for case-insensitive comparison
    let current_tags_lower: HashSet<String> =
        current_tags.iter().map(|t| t.to_lowercase()).collect();

    // Get all memories with their tags
    let all_memories: Vec<(String, String)> = sqlx::query_as::<_, (String, String)>(
        "SELECT id, tags FROM memories WHERE id != ? ORDER BY created_at DESC",
    )
    .bind(memory_id)
    .fetch_all(pool)
    .await?;

    let mut results = vec![];

    for (other_id, tags_json) in all_memories {
        // Parse tags JSON
        let tags: Vec<String> = match serde_json::from_str(&tags_json) {
            Ok(t) => t,
            Err(_) => continue,
        };

        // Find shared tags (case-insensitive)
        let other_tags_lower: HashSet<String> = tags.iter().map(|t| t.to_lowercase()).collect();

        let shared: HashSet<String> = current_tags_lower
            .intersection(&other_tags_lower)
            .cloned()
            .collect();

        if !shared.is_empty() {
            let shared_count = shared.len();
            let shared_list = shared.into_iter().collect::<Vec<_>>();
            results.push((other_id, shared_count, shared_list));
        }
    }

    // Sort by shared tag count (descending)
    results.sort_by(|a, b| b.1.cmp(&a.1));

    Ok(results)
}

/// Check if a link already exists between two memories
pub async fn link_exists(pool: &SqlitePool, source_id: &str, target_id: &str) -> Result<bool> {
    // Get node IDs
    let source_node: Option<i64> =
        sqlx::query_scalar("SELECT id FROM graph_nodes WHERE memory_id = ?")
            .bind(source_id)
            .fetch_optional(pool)
            .await?;

    let target_node: Option<i64> =
        sqlx::query_scalar("SELECT id FROM graph_nodes WHERE memory_id = ?")
            .bind(target_id)
            .fetch_optional(pool)
            .await?;

    if let (Some(src), Some(tgt)) = (source_node, target_node) {
        let exists: Option<i64> =
            sqlx::query_scalar("SELECT id FROM graph_edges WHERE source_id = ? AND target_id = ?")
                .bind(src)
                .bind(tgt)
                .fetch_optional(pool)
                .await?;

        Ok(exists.is_some())
    } else {
        Ok(false)
    }
}

/// Create a RELATES_TO edge between two memories
pub async fn create_tag_link(
    pool: &SqlitePool,
    source_id: &str,
    target_id: &str,
    shared_tags: &[String],
) -> Result<()> {
    // Check if link already exists (both directions)
    let forward_exists = link_exists(pool, source_id, target_id).await?;
    let backward_exists = link_exists(pool, target_id, source_id).await?;

    if forward_exists || backward_exists {
        return Ok(()); // Link already exists, don't create duplicate
    }

    // Get node IDs
    let source_node: Option<i64> =
        sqlx::query_scalar("SELECT id FROM graph_nodes WHERE memory_id = ?")
            .bind(source_id)
            .fetch_optional(pool)
            .await?;

    let target_node: Option<i64> =
        sqlx::query_scalar("SELECT id FROM graph_nodes WHERE memory_id = ?")
            .bind(target_id)
            .fetch_optional(pool)
            .await?;

    if let (Some(src), Some(tgt)) = (source_node, target_node) {
        let note = if shared_tags.is_empty() {
            "Shares tags with memory".to_string()
        } else {
            format!("Shares tags: {}", shared_tags.join(", "))
        };

        let now = chrono::Utc::now().to_rfc3339();

        sqlx::query(
            "INSERT OR IGNORE INTO graph_edges (source_id, target_id, rel_type, note, created_at)
             VALUES (?, ?, ?, ?, ?)",
        )
        .bind(src)
        .bind(tgt)
        .bind("RELATES_TO")
        .bind(&note)
        .bind(&now)
        .execute(pool)
        .await?;

        tracing::debug!(
            "Created tag-based link: {} → {} (shared tags: {})",
            source_id,
            target_id,
            shared_tags.join(", ")
        );
    }

    Ok(())
}

/// Auto-link a memory to all memories with shared tags (up to limit)
/// Returns count of links created
pub async fn auto_link_by_tags(
    pool: &SqlitePool,
    memory_id: &str,
    tags: &[String],
    max_links: usize,
) -> Result<usize> {
    let matches = find_memories_with_shared_tags(pool, memory_id, tags).await?;

    let mut count = 0;
    for (other_id, _shared_count, shared_tags) in matches.iter().take(max_links) {
        create_tag_link(pool, memory_id, other_id, &shared_tags).await?;
        count += 1;
    }

    if count > 0 {
        tracing::info!(
            "Auto-linked memory {} to {} other memories by shared tags",
            memory_id,
            count
        );
    }

    Ok(count)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_find_shared_tags() {
        let tags1 = vec![
            "kubernetes".to_string(),
            "docker".to_string(),
            "deployment".to_string(),
        ];
        let tags2 = vec!["Docker".to_string(), "containers".to_string()];

        let tags1_lower: HashSet<String> = tags1.iter().map(|t| t.to_lowercase()).collect();
        let tags2_lower: HashSet<String> = tags2.iter().map(|t| t.to_lowercase()).collect();

        let shared: HashSet<String> = tags1_lower.intersection(&tags2_lower).cloned().collect();

        assert_eq!(shared.len(), 1);
        assert!(shared.contains("docker"));
    }

    #[test]
    fn test_case_insensitive_matching() {
        let tag_lower = "kubernetes";
        let tag_upper = "KUBERNETES";
        let tag_mixed = "Kubernetes";

        assert_eq!(tag_lower.to_lowercase(), tag_upper.to_lowercase());
        assert_eq!(tag_lower.to_lowercase(), tag_mixed.to_lowercase());
    }
}
