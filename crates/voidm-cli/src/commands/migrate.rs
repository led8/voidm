use anyhow::{Context, Result};
use clap::Parser;
use std::sync::Arc;
use voidm_core::db::Database;

#[derive(Parser, Clone)]
pub struct MigrateArgs {
    /// Source backend: 'sqlite' or 'neo4j'
    #[arg(value_name = "SOURCE")]
    pub from: String,

    /// Destination backend: 'sqlite' or 'neo4j'
    #[arg(value_name = "DEST")]
    pub to: String,

    /// Only migrate scopes matching this pattern (optional)
    #[arg(long)]
    pub scope_filter: Option<String>,

    /// Dry run: show what would be migrated without making changes
    #[arg(long)]
    pub dry_run: bool,

    /// Skip memories with these IDs (comma-separated)
    #[arg(long)]
    pub skip_ids: Option<String>,
}

pub async fn run(
    args: MigrateArgs,
    config: &voidm_core::Config,
    cli_db: Option<&str>,
    sqlite_path_override: Option<&str>,
    json: bool,
) -> Result<()> {
    // Validate backends
    let from_backend = args.from.to_lowercase();
    let to_backend = args.to.to_lowercase();

    if from_backend == to_backend {
        anyhow::bail!("Source and destination backends cannot be the same");
    }

    if ![from_backend.as_str(), to_backend.as_str()]
        .iter()
        .all(|b| matches!(*b, "sqlite" | "neo4j"))
    {
        anyhow::bail!("Backend must be 'sqlite' or 'neo4j'");
    }

    // Parse skip list
    let skip_ids: std::collections::HashSet<String> = args
        .skip_ids
        .unwrap_or_default()
        .split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();

    let sqlite_path = config
        .resolve_db_path(cli_db, sqlite_path_override)
        .path
        .to_string_lossy()
        .to_string();

    // Open source database
    let source_db: Arc<dyn voidm_core::db::Database> = if from_backend == "sqlite" {
        let pool = voidm_core::db::sqlite::open_sqlite_pool(&sqlite_path).await?;
        Arc::new(voidm_core::db::sqlite::SqliteDatabase { pool })
    } else {
        let neo4j_config = config
            .database
            .neo4j
            .as_ref()
            .context("Neo4j config missing for source")?;
        Arc::new(
            voidm_core::db::neo4j::Neo4jDatabase::connect(
                &neo4j_config.uri,
                &neo4j_config.username,
                &neo4j_config.password,
            )
            .await?,
        )
    };

    // Open destination database
    let dest_db: Arc<dyn voidm_core::db::Database> = if to_backend == "sqlite" {
        let pool = voidm_core::db::sqlite::open_sqlite_pool(&sqlite_path).await?;
        Arc::new(voidm_core::db::sqlite::SqliteDatabase { pool })
    } else {
        let neo4j_config = config
            .database
            .neo4j
            .as_ref()
            .context("Neo4j config missing for destination")?;
        Arc::new(
            voidm_core::db::neo4j::Neo4jDatabase::connect(
                &neo4j_config.uri,
                &neo4j_config.username,
                &neo4j_config.password,
            )
            .await?,
        )
    };

    // Ensure schemas are initialized
    if !args.dry_run {
        source_db.ensure_schema().await?;
        dest_db.ensure_schema().await?;
    }

    // Migrate memories
    migrate_memories(
        source_db.as_ref(),
        dest_db.as_ref(),
        config,
        &args.scope_filter,
        &skip_ids,
        args.dry_run,
        json,
    )
    .await?;

    // Migrate concepts
    migrate_concepts(
        source_db.as_ref(),
        dest_db.as_ref(),
        &args.scope_filter,
        args.dry_run,
        json,
    )
    .await?;

    // Migrate relationships (memory-to-memory edges)
    migrate_relationships(
        source_db.as_ref(),
        dest_db.as_ref(),
        &skip_ids,
        args.dry_run,
        json,
    )
    .await?;

    // Migrate ontology edges (concept-concept, concept-memory, etc.)
    migrate_ontology_edges(
        source_db.as_ref(),
        dest_db.as_ref(),
        &skip_ids,
        args.dry_run,
        json,
    )
    .await?;

    if !json {
        println!("\n✓ Migration complete!");
    } else {
        println!(
            "{}",
            serde_json::json!({
                "status": "success",
                "message": "Migration complete"
            })
        );
    }

    Ok(())
}

async fn migrate_memories(
    source: &(impl Database + ?Sized),
    dest: &(impl Database + ?Sized),
    config: &voidm_core::Config,
    scope_filter: &Option<String>,
    skip_ids: &std::collections::HashSet<String>,
    dry_run: bool,
    json: bool,
) -> Result<()> {
    let memories = source.list_memories(Some(10000)).await?;

    let mut migrated = 0;
    let mut skipped = 0;

    for mem in memories {
        // Skip if in skip list
        if skip_ids.contains(&mem.id) {
            skipped += 1;
            continue;
        }

        // Filter by scope if specified
        if let Some(filter) = scope_filter {
            if !mem.scopes.iter().any(|s| s.contains(filter)) {
                skipped += 1;
                continue;
            }
        }

        if dry_run {
            migrated += 1;
            if !json {
                println!(
                    "  [DRY RUN] Would migrate memory: {} ({})",
                    mem.id, mem.memory_type
                );
            }
            continue;
        }

        let req = voidm_core::models::AddMemoryRequest {
            id: Some(mem.id.clone()),
            content: mem.content.clone(),
            memory_type: mem.memory_type.parse()?,
            scopes: mem.scopes.clone(),
            tags: mem.tags.clone(),
            importance: mem.importance,
            metadata: mem.metadata.clone(),
            links: vec![],
        };

        let _ = dest.add_memory(req, config).await?;
        migrated += 1;

        if !json && migrated % 100 == 0 {
            println!("  Migrated {} memories...", migrated);
        }
    }

    if !json {
        println!("Memories: {} migrated, {} skipped", migrated, skipped);
    }

    Ok(())
}

async fn migrate_concepts(
    source: &(impl Database + ?Sized),
    dest: &(impl Database + ?Sized),
    scope_filter: &Option<String>,
    dry_run: bool,
    json: bool,
) -> Result<()> {
    let concepts = source.list_concepts(None, 10000).await?;

    let mut migrated = 0;
    let mut skipped = 0;

    for concept in concepts {
        // Filter by scope if specified
        if let Some(filter) = scope_filter {
            if !concept
                .scope
                .as_ref()
                .map(|s| s.contains(filter))
                .unwrap_or(false)
            {
                skipped += 1;
                continue;
            }
        }

        if dry_run {
            migrated += 1;
            if !json {
                println!(
                    "  [DRY RUN] Would migrate concept: {} ({})",
                    concept.id, concept.name
                );
            }
            continue;
        }

        let _ = dest
            .add_concept(
                &concept.name,
                concept.description.as_deref(),
                concept.scope.as_deref(),
                Some(&concept.id),
            )
            .await?;
        migrated += 1;

        if !json && migrated % 100 == 0 {
            println!("  Migrated {} concepts...", migrated);
        }
    }

    if !json {
        println!("Concepts: {} migrated, {} skipped", migrated, skipped);
    }

    Ok(())
}

async fn migrate_relationships(
    source: &(impl Database + ?Sized),
    dest: &(impl Database + ?Sized),
    skip_ids: &std::collections::HashSet<String>,
    dry_run: bool,
    json: bool,
) -> Result<()> {
    let edges = source.list_edges().await?;

    let mut migrated = 0;
    let mut skipped = 0;

    for edge in edges {
        // Skip if either endpoint is in skip list
        if skip_ids.contains(&edge.from_id) || skip_ids.contains(&edge.to_id) {
            skipped += 1;
            continue;
        }

        if dry_run {
            migrated += 1;
            if !json {
                println!(
                    "  [DRY RUN] Would migrate edge: {} -> {} ({})",
                    edge.from_id, edge.to_id, edge.rel_type
                );
            }
            continue;
        }

        // Parse edge type from string
        let rel_type = match edge.rel_type.as_str() {
            "SUPPORTS" => voidm_core::models::EdgeType::Supports,
            "CONTRADICTS" => voidm_core::models::EdgeType::Contradicts,
            "PRECEDES" => voidm_core::models::EdgeType::Precedes,
            "DERIVED_FROM" => voidm_core::models::EdgeType::DerivedFrom,
            "RELATES_TO" => voidm_core::models::EdgeType::RelatesTo,
            "EXEMPLIFIES" => voidm_core::models::EdgeType::Exemplifies,
            "PART_OF" => voidm_core::models::EdgeType::PartOf,
            _ => {
                if !json {
                    eprintln!(
                        "  Warning: Unknown edge type '{}', skipping edge {} -> {}",
                        edge.rel_type, edge.from_id, edge.to_id
                    );
                }
                skipped += 1;
                continue;
            }
        };

        match dest
            .link_memories(&edge.from_id, &rel_type, &edge.to_id, edge.note.as_deref())
            .await
        {
            Ok(resp) => {
                if resp.created {
                    migrated += 1;
                    if !json && migrated % 10 == 0 {
                        println!("  Migrated {} edges...", migrated);
                    }
                } else {
                    if !json {
                        eprintln!(
                            "  ERROR: Edge NOT created (MATCH failed?) {} -> {} ({})",
                            edge.from_id, edge.to_id, edge.rel_type
                        );
                    }
                    skipped += 1;
                }
            }
            Err(e) => {
                if !json {
                    eprintln!(
                        "  ERROR: {} -> {} ({}): {}",
                        edge.from_id, edge.to_id, edge.rel_type, e
                    );
                }
                skipped += 1;
            }
        }
    }

    if !json {
        println!("Edges: {} migrated, {} skipped", migrated, skipped);
    }

    Ok(())
}

async fn migrate_ontology_edges(
    source: &(impl Database + ?Sized),
    dest: &(impl Database + ?Sized),
    skip_ids: &std::collections::HashSet<String>,
    dry_run: bool,
    json: bool,
) -> Result<()> {
    let edges = source.list_ontology_edges().await?;

    let mut migrated = 0;
    let mut skipped = 0;
    let mut failed = 0;

    for edge in edges {
        // Skip if either endpoint is in skip list
        if skip_ids.contains(&edge.from_id) || skip_ids.contains(&edge.to_id) {
            skipped += 1;
            continue;
        }

        if dry_run {
            migrated += 1;
            if !json {
                println!(
                    "  [DRY RUN] Would migrate ontology edge: {} ({}) -> {} ({}) [{}]",
                    edge.from_id, edge.from_type, edge.to_id, edge.to_type, edge.rel_type
                );
            }
            continue;
        }

        // Try to create the ontology edge
        match dest
            .create_ontology_edge(
                &edge.from_id,
                &edge.from_type,
                &edge.rel_type,
                &edge.to_id,
                &edge.to_type,
            )
            .await
        {
            Ok(true) => {
                migrated += 1;
                if !json && migrated % 100 == 0 {
                    println!("  Migrated {} ontology edges...", migrated);
                }
            }
            Ok(false) => {
                if !json && failed < 5 {
                    eprintln!(
                        "  Warning: Ontology edge not created: {} -> {}",
                        edge.from_id, edge.to_id
                    );
                }
                failed += 1;
            }
            Err(e) => {
                if !json && failed < 5 {
                    eprintln!("  Error: {} -> {}: {}", edge.from_id, edge.to_id, e);
                }
                failed += 1;
            }
        }
    }

    if !json {
        println!(
            "Ontology Edges: {} migrated, {} failed, {} skipped",
            migrated, failed, skipped
        );
    }

    Ok(())
}
