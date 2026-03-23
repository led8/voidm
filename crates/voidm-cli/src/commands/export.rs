use anyhow::Result;
use clap::Args;
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use std::sync::Arc;
use voidm_core::{crud, db::Database, models::MemoryEdge, ontology::Concept, Config};

#[derive(Args)]
pub struct ExportArgs {
    /// Filter by scope prefix
    #[arg(long)]
    pub scope: Option<String>,

    /// Export format: json, markdown, full (json with all relationships)
    #[arg(long, default_value = "json")]
    pub format: String,

    /// Output file (default: stdout)
    #[arg(long, short = 'o')]
    pub output: Option<String>,

    #[arg(long, default_value = "1000")]
    pub limit: usize,

    /// Include all relationships and concepts
    #[arg(long)]
    pub with_edges: bool,

    /// Include ontology concepts
    #[arg(long)]
    pub with_concepts: bool,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ExportData {
    pub memories: Vec<voidm_core::models::Memory>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub edges: Vec<MemoryEdge>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub concepts: Vec<Concept>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub ontology_edges: Vec<voidm_core::models::OntologyEdgeForMigration>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<ExportMetadata>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ExportMetadata {
    pub total_memories: usize,
    pub total_edges: usize,
    pub total_concepts: usize,
    pub total_ontology_edges: usize,
    pub exported_at: String,
    pub scopes_included: Vec<String>,
}

pub async fn run(args: ExportArgs, pool: &SqlitePool, _config: &Config, _json: bool) -> Result<()> {
    let db = Arc::new(voidm_core::db::sqlite::SqliteDatabase { pool: pool.clone() });

    let memories = crud::list_memories(pool, args.scope.as_deref(), None, args.limit).await?;
    let mut edges = Vec::new();
    let mut concepts = Vec::new();
    let mut ontology_edges = Vec::new();

    // Get edges if requested or format is "full"
    if args.with_edges || args.format == "full" {
        edges = db.list_edges().await.unwrap_or_default();
    }

    // Get concepts if requested or format is "full"
    if args.with_concepts || args.format == "full" {
        concepts = db
            .list_concepts(args.scope.as_deref(), args.limit)
            .await
            .unwrap_or_default();
        ontology_edges = db.list_ontology_edges().await.unwrap_or_default();
    }

    let content = match args.format.as_str() {
        "json" | "full" => {
            let scopes: Vec<String> = memories
                .iter()
                .flat_map(|m| m.scopes.clone())
                .collect::<std::collections::HashSet<_>>()
                .into_iter()
                .collect();

            let export_data = ExportData {
                memories: memories.clone(),
                edges: edges.clone(),
                concepts: concepts.clone(),
                ontology_edges: ontology_edges.clone(),
                metadata: Some(ExportMetadata {
                    total_memories: memories.len(),
                    total_edges: edges.len(),
                    total_concepts: concepts.len(),
                    total_ontology_edges: ontology_edges.len(),
                    exported_at: chrono::Utc::now().to_rfc3339(),
                    scopes_included: scopes,
                }),
            };
            serde_json::to_string_pretty(&export_data)?
        }
        "markdown" => {
            let mut md = String::new();
            md.push_str("# voidm Memory Export\n\n");

            // Add metadata
            if !args.format.is_empty() {
                md.push_str(&format!(
                    "**Exported**: {}\n",
                    chrono::Utc::now().to_rfc3339()
                ));
                md.push_str(&format!("**Memories**: {}\n", memories.len()));
                if args.with_edges || args.format == "full" {
                    md.push_str(&format!("**Edges**: {}\n", edges.len()));
                }
                if args.with_concepts || args.format == "full" {
                    md.push_str(&format!("**Concepts**: {}\n", concepts.len()));
                    md.push_str(&format!("**Ontology Edges**: {}\n", ontology_edges.len()));
                }
                md.push_str("\n---\n\n");
            }

            // Export memories
            md.push_str("## Memories\n\n");
            for m in &memories {
                md.push_str(&format!("### {} [{}]\n\n", m.id, m.memory_type));
                md.push_str(&format!("- **Importance**: {}\n", m.importance));
                md.push_str(&format!(
                    "- **Quality**: {}\n",
                    m.quality_score.unwrap_or(0.0)
                ));
                md.push_str(&format!("- **Created**: {}\n", m.created_at));
                if !m.scopes.is_empty() {
                    md.push_str(&format!("- **Scopes**: {}\n", m.scopes.join(", ")));
                }
                if !m.tags.is_empty() {
                    md.push_str(&format!("- **Tags**: {}\n", m.tags.join(", ")));
                }
                md.push('\n');
                md.push_str(&m.content);
                md.push_str("\n\n---\n\n");
            }

            // Export edges if included
            if (args.with_edges || args.format == "full") && !edges.is_empty() {
                md.push_str("## Memory Relationships\n\n");
                for edge in &edges {
                    md.push_str(&format!(
                        "- `{}` **[{}]** → `{}`",
                        edge.from_id, edge.rel_type, edge.to_id
                    ));
                    if let Some(note) = &edge.note {
                        md.push_str(&format!(" ({})", note));
                    }
                    md.push('\n');
                }
                md.push_str("\n");
            }

            // Export concepts if included
            if (args.with_concepts || args.format == "full") && !concepts.is_empty() {
                md.push_str("## Concepts\n\n");
                for concept in &concepts {
                    md.push_str(&format!("### {} ({})\n\n", concept.name, concept.id));
                    if let Some(desc) = &concept.description {
                        md.push_str(&format!("{}\n\n", desc));
                    }
                    if let Some(scope) = &concept.scope {
                        md.push_str(&format!("**Scope**: {}\n\n", scope));
                    }
                }
            }

            // Export ontology edges if included
            if (args.with_concepts || args.format == "full") && !ontology_edges.is_empty() {
                md.push_str("## Ontology Relationships\n\n");
                for edge in &ontology_edges {
                    md.push_str(&format!(
                        "- `{}` ({}) **[{}]** → `{}` ({})\n",
                        edge.from_id, edge.from_type, edge.rel_type, edge.to_id, edge.to_type
                    ));
                }
            }

            md
        }
        other => anyhow::bail!(
            "Unknown export format: '{}'. Valid: json, markdown, full",
            other
        ),
    };

    if let Some(path) = args.output {
        std::fs::write(&path, &content)?;
        let msg = if args.with_edges || args.with_concepts {
            format!(
                "Exported {} memories + {} edges + {} concepts to {}",
                memories.len(),
                edges.len(),
                concepts.len(),
                path
            )
        } else {
            format!("Exported {} memories to {}", memories.len(), path)
        };
        eprintln!("{}", msg);
    } else {
        print!("{}", content);
    }
    Ok(())
}
