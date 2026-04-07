use crate::db::Database;
use crate::models::{AddMemoryRequest, MemoryType};
use crate::Config;
use anyhow::Result;
use std::collections::HashSet;
use std::str::FromStr;

/// Migrate memories from source to destination database
pub async fn migrate_memories(
    source: &(impl Database + ?Sized),
    dest: &(impl Database + ?Sized),
    config: &Config,
    scope_filter: &Option<String>,
    skip_ids: &HashSet<String>,
    dry_run: bool,
) -> Result<(u32, u32)> {
    let memories = source.list_memories(Some(10000)).await?;

    let mut migrated = 0;
    let mut skipped = 0;

    for mem in memories {
        if skip_ids.contains(&mem.id) {
            skipped += 1;
            continue;
        }

        if let Some(filter) = scope_filter {
            let matches = mem.scopes.iter().any(|s| s.contains(filter));
            if !matches {
                skipped += 1;
                continue;
            }
        }

        if dry_run {
            migrated += 1;
            continue;
        }

        let memory_type = MemoryType::from_str(&mem.memory_type).unwrap_or(MemoryType::Semantic);

        let req = AddMemoryRequest {
            id: Some(mem.id),
            content: mem.content,
            memory_type,
            scopes: mem.scopes,
            tags: mem.tags,
            importance: mem.importance,
            metadata: mem.metadata,
            links: vec![],
            title: mem.title,
            context: mem.context,
        };

        match dest.add_memory(req, config).await {
            Ok(_) => migrated += 1,
            Err(e) => {
                anyhow::bail!("Failed to create memory in destination: {}", e);
            }
        }

        if migrated % 100 == 0 {
            println!("  Migrated {} memories...", migrated);
        }
    }

    Ok((migrated, skipped))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{sqlite::SqliteDatabase, Database};
    use std::sync::Arc;

    async fn create_test_db() -> Result<Arc<dyn Database>> {
        let pool = crate::db::sqlite::open_sqlite_pool(":memory:").await?;
        let db = SqliteDatabase { pool };
        db.ensure_schema().await?;
        Ok(Arc::new(db))
    }

    #[tokio::test]
    async fn test_migrate_memories_basic() -> Result<()> {
        let source = create_test_db().await?;
        let dest = create_test_db().await?;
        let config = Config::default();

        let req = AddMemoryRequest {
            id: None,
            content: "Test memory".to_string(),
            memory_type: MemoryType::Episodic,
            scopes: vec!["test".to_string()],
            tags: vec![],
            importance: 5,
            metadata: serde_json::json!({}),
            links: vec![],
            title: None,
            context: None,
        };
        let mem = source.add_memory(req, &config).await?;

        let skip_ids = HashSet::new();
        let (migrated, skipped) = migrate_memories(
            source.as_ref(),
            dest.as_ref(),
            &config,
            &None,
            &skip_ids,
            false,
        )
        .await?;

        assert_eq!(migrated, 1);
        assert_eq!(skipped, 0);

        let dest_mems = dest.list_memories(Some(10)).await?;
        assert_eq!(dest_mems.len(), 1);
        assert_eq!(dest_mems[0].content, "Test memory");
        assert_eq!(dest_mems[0].id, mem.id);

        Ok(())
    }

    #[tokio::test]
    async fn test_migrate_memories_preserves_ids() -> Result<()> {
        let source = create_test_db().await?;
        let dest = create_test_db().await?;
        let config = Config::default();

        let specific_id = "test-id-12345";
        let req = AddMemoryRequest {
            id: Some(specific_id.to_string()),
            content: "Memory with ID".to_string(),
            memory_type: MemoryType::Semantic,
            scopes: vec!["project/test".to_string()],
            tags: vec!["test".to_string()],
            importance: 8,
            metadata: serde_json::json!({}),
            links: vec![],
            title: None,
            context: None,
        };
        let mem = source.add_memory(req, &config).await?;
        assert_eq!(mem.id, specific_id);

        let skip_ids = HashSet::new();
        migrate_memories(
            source.as_ref(),
            dest.as_ref(),
            &config,
            &None,
            &skip_ids,
            false,
        )
        .await?;

        let dest_mems = dest.list_memories(Some(10)).await?;
        assert_eq!(dest_mems[0].id, specific_id);

        Ok(())
    }

    #[tokio::test]
    async fn test_migrate_with_skip_ids() -> Result<()> {
        let source = create_test_db().await?;
        let dest = create_test_db().await?;
        let config = Config::default();

        for id in &["keep-this", "skip-this", "keep-this-too"] {
            source
                .add_memory(
                    AddMemoryRequest {
                        id: Some(id.to_string()),
                        content: format!("Memory {}", id),
                        memory_type: MemoryType::Episodic,
                        scopes: vec![],
                        tags: vec![],
                        importance: 5,
                        metadata: serde_json::json!({}),
                        links: vec![],
                        title: None,
                        context: None,
                    },
                    &config,
                )
                .await?;
        }

        let mut skip_ids = HashSet::new();
        skip_ids.insert("skip-this".to_string());

        let (migrated, skipped) = migrate_memories(
            source.as_ref(),
            dest.as_ref(),
            &config,
            &None,
            &skip_ids,
            false,
        )
        .await?;

        assert_eq!(migrated, 2);
        assert_eq!(skipped, 1);

        let dest_mems = dest.list_memories(Some(10)).await?;
        assert_eq!(dest_mems.len(), 2);

        Ok(())
    }

    #[tokio::test]
    async fn test_dry_run_no_modifications() -> Result<()> {
        let source = create_test_db().await?;
        let dest = create_test_db().await?;
        let config = Config::default();

        source
            .add_memory(
                AddMemoryRequest {
                    id: Some("test".to_string()),
                    content: "Test".to_string(),
                    memory_type: MemoryType::Episodic,
                    scopes: vec![],
                    tags: vec![],
                    importance: 5,
                    metadata: serde_json::json!({}),
                    links: vec![],
                    title: None,
                    context: None,
                },
                &config,
            )
            .await?;

        let skip_ids = HashSet::new();
        let (migrated, _) = migrate_memories(
            source.as_ref(),
            dest.as_ref(),
            &config,
            &None,
            &skip_ids,
            true,
        )
        .await?;

        assert_eq!(migrated, 1);

        let dest_mems = dest.list_memories(Some(10)).await?;
        assert_eq!(dest_mems.len(), 0);

        Ok(())
    }

    #[tokio::test]
    async fn test_migrate_with_scope_filter() -> Result<()> {
        let source = create_test_db().await?;
        let dest = create_test_db().await?;
        let config = Config::default();

        source
            .add_memory(
                AddMemoryRequest {
                    id: None,
                    content: "Project A memory".to_string(),
                    memory_type: MemoryType::Episodic,
                    scopes: vec!["project/alpha".to_string()],
                    tags: vec![],
                    importance: 5,
                    metadata: serde_json::json!({}),
                    links: vec![],
                    title: None,
                    context: None,
                },
                &config,
            )
            .await?;

        source
            .add_memory(
                AddMemoryRequest {
                    id: None,
                    content: "Project B memory".to_string(),
                    memory_type: MemoryType::Episodic,
                    scopes: vec!["project/beta".to_string()],
                    tags: vec![],
                    importance: 5,
                    metadata: serde_json::json!({}),
                    links: vec![],
                    title: None,
                    context: None,
                },
                &config,
            )
            .await?;

        let skip_ids = HashSet::new();
        let scope_filter = Some("project/alpha".to_string());

        let (migrated, _) = migrate_memories(
            source.as_ref(),
            dest.as_ref(),
            &config,
            &scope_filter,
            &skip_ids,
            false,
        )
        .await?;

        assert_eq!(migrated, 1);

        let dest_mems = dest.list_memories(Some(10)).await?;
        assert_eq!(dest_mems.len(), 1);
        assert_eq!(dest_mems[0].content, "Project A memory");

        Ok(())
    }

    #[tokio::test]
    async fn test_sqlite_to_sqlite_full_migration() -> Result<()> {
        let source = create_test_db().await?;
        let dest = create_test_db().await?;
        let config = Config::default();

        // Create diverse test data
        for i in 0..5 {
            source
                .add_memory(
                    AddMemoryRequest {
                        id: Some(format!("mem{}", i)),
                        content: format!("Memory {}", i),
                        memory_type: if i % 2 == 0 {
                            MemoryType::Episodic
                        } else {
                            MemoryType::Semantic
                        },
                        scopes: vec![format!("scope{}", i % 2)],
                        tags: vec![format!("tag{}", i)],
                        importance: ((i as i64) % 10) + 1,
                        metadata: serde_json::json!({"index": i}),
                        links: vec![],
                        title: None,
                        context: None,
                    },
                    &config,
                )
                .await?;
        }

        // Migrate all
        let skip_ids = HashSet::new();
        let (migrated, skipped) = migrate_memories(
            source.as_ref(),
            dest.as_ref(),
            &config,
            &None,
            &skip_ids,
            false,
        )
        .await?;

        assert_eq!(migrated, 5);
        assert_eq!(skipped, 0);

        let dest_mems = dest.list_memories(Some(10)).await?;
        assert_eq!(dest_mems.len(), 5);

        // Verify all IDs preserved
        for i in 0..5 {
            let expected_id = format!("mem{}", i);
            assert!(dest_mems.iter().any(|m| m.id == expected_id));
        }

        Ok(())
    }
}
