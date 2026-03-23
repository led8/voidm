use anyhow::Result;
use clap::{Args, Subcommand};
use sqlx::SqlitePool;
use uuid::Uuid;
use voidm_core::ontology::{self, HierarchyDirection, NodeKind, OntologyRelType};
use voidm_core::Config;

// ─── Top-level subcommand tree ────────────────────────────────────────────────

#[derive(Subcommand)]
pub enum OntologyCommands {
    /// Concept management
    #[command(subcommand)]
    Concept(ConceptCommands),
    /// Add a typed edge in the ontology graph
    Link(OntologyLinkArgs),
    /// Remove an ontology edge by id
    Unlink(OntologyUnlinkArgs),
    /// List all ontology edges for a node
    Edges(OntologyEdgesArgs),
    /// Show ancestors and descendants of a concept (IS_A hierarchy)
    Hierarchy(HierarchyArgs),
    /// List all instances of a concept, including subclasses
    Instances(InstancesArgs),
    /// Enrich all unenriched concepts with NLI relation suggestions (downloads model on first use)
    Enrich(EnrichArgs),
    /// Benchmark NLI inference latency
    Benchmark,
    /// Extract named entities from text and propose them as concept candidates
    Extract(ExtractArgs),
    /// Batch-enrich all memories with NER entity extraction, auto-linking to existing concepts
    EnrichMemories(EnrichMemoriesArgs),
    /// Auto-improve database: enrich memories + auto-merge duplicates (one command)
    #[command(alias = "improve")]
    AutoImprove(AutoImproveArgs),
}

// ─── Concept subcommands ──────────────────────────────────────────────────────

#[derive(Subcommand)]
pub enum ConceptCommands {
    /// Add a new concept
    Add(ConceptAddArgs),
    /// Get a concept by ID
    Get(ConceptGetArgs),
    /// List concepts
    List(ConceptListArgs),
    /// Delete a concept
    Delete(ConceptDeleteArgs),
    /// Merge source concept into target (retargets edges, deletes source)
    Merge(ConceptMergeArgs),
    /// Find merge candidates (similar concepts for deduplication)
    #[command(alias = "find-duplicates")]
    FindMergeCandidates(FindMergeCandidatesArgs),
    /// Preview batch merge plan (dry-run)
    MergeBatch(MergeBatchArgs),
    /// Execute previously previewed batch merge
    MergeBatchApply(MergeBatchApplyArgs),
    /// Rollback single merge operation
    RollbackMerge(RollbackMergeArgs),
    /// List merge operation history
    MergeHistory(MergeHistoryArgs),
    /// Auto-merge similar concepts (one-command database cleanup)
    #[command(alias = "auto", alias = "cleanup")]
    AutoMerge(AutoMergeArgs),
}

#[derive(Args)]
pub struct ConceptAddArgs {
    /// Concept name
    pub name: String,
    /// Optional description
    #[arg(long, short)]
    pub description: Option<String>,
    /// Optional scope (e.g. project/domain)
    #[arg(long, short)]
    pub scope: Option<String>,
    /// Run NLI enrichment: suggest relations to existing concepts (downloads model ~180MB on first use)
    #[arg(long)]
    pub enrich: bool,
}

#[derive(Args)]
pub struct EnrichArgs {
    /// Max candidates per concept to score (default: 10)
    #[arg(long, default_value = "10")]
    pub top_k: usize,
}

#[derive(Args)]
pub struct ExtractArgs {
    /// Text to extract entities from
    pub text: String,
    /// Minimum confidence score to include (0.0–1.0, default: 0.7)
    #[arg(long, default_value = "0.7")]
    pub min_score: f32,
    /// Automatically add confirmed candidates as concepts (without this flag, only proposes)
    #[arg(long)]
    pub add: bool,
    /// Scope to assign to auto-added concepts
    #[arg(long)]
    pub scope: Option<String>,
}

#[derive(Args)]
pub struct EnrichMemoriesArgs {
    /// Only process memories with this scope prefix
    #[arg(long, short)]
    pub scope: Option<String>,
    /// Minimum NER confidence score (default: 0.7)
    #[arg(long, default_value = "0.7")]
    pub min_score: f32,
    /// Automatically create missing concepts (otherwise only links to existing ones)
    #[arg(long)]
    pub add: bool,
    /// Re-process memories already enriched (default: skip them)
    #[arg(long)]
    pub force: bool,
    /// Show what would be done without writing anything
    #[arg(long)]
    pub dry_run: bool,
    /// Max number of memories to process (default: all)
    #[arg(long, default_value = "0")]
    pub limit: usize,
}

#[derive(Args)]
pub struct AutoImproveArgs {
    /// Minimum NER confidence score for enrichment (default: 0.7)
    #[arg(long, default_value = "0.7")]
    pub min_score: f32,
    /// Similarity threshold for auto-merge (default: 0.90)
    #[arg(long, default_value = "0.90")]
    pub threshold: f32,
    /// Dry-run: show what would be done without making changes
    #[arg(long)]
    pub dry_run: bool,
    /// Skip preview and run immediately
    #[arg(long)]
    pub force: bool,
    /// Only process memories with this scope prefix
    #[arg(long, short)]
    pub scope: Option<String>,
    /// Skip enrichment step, only merge duplicates (much faster)
    #[arg(long)]
    pub merge_only: bool,
}

#[derive(Args)]
pub struct ConceptGetArgs {
    /// Concept ID or short prefix (min 4 chars)
    pub id: String,
}

#[derive(Args)]
pub struct ConceptListArgs {
    /// Filter by scope prefix
    #[arg(long, short)]
    pub scope: Option<String>,
    /// Max results
    #[arg(long, default_value = "50")]
    pub limit: usize,
}

#[derive(Args)]
pub struct ConceptDeleteArgs {
    /// Concept ID or short prefix
    pub id: String,
}

#[derive(Args)]
pub struct ConceptMergeArgs {
    /// Source concept ID (to merge from)
    pub source: String,
    /// Target concept ID (to merge into)
    pub target: String,
}

#[derive(Args)]
pub struct FindMergeCandidatesArgs {
    /// Similarity threshold (0.0-1.0, default 0.8 for high similarity)
    #[arg(long, default_value = "0.8")]
    pub threshold: f32,
    /// Output file path for JSON results (default: stdout)
    #[arg(long, short)]
    pub output: Option<String>,
}

#[derive(Args)]
pub struct MergeBatchArgs {
    /// Path to merge plan JSON file
    #[arg(long, short)]
    pub from: String,
    /// Execute the merge plan (default is dry-run preview only)
    #[arg(long)]
    pub execute: bool,
}

#[derive(Args)]
pub struct MergeBatchApplyArgs {
    /// Batch ID from merge-batch preview
    pub batch_id: String,
}

#[derive(Args)]
pub struct RollbackMergeArgs {
    /// Merge operation ID to rollback
    pub merge_id: String,
}

#[derive(Args)]
pub struct MergeHistoryArgs {
    /// Filter by batch ID
    #[arg(long)]
    pub batch: Option<String>,
    /// Filter by status (pending, completed, rolled_back, failed)
    #[arg(long)]
    pub status: Option<String>,
}

#[derive(Args)]
pub struct AutoMergeArgs {
    /// Similarity threshold (0.0-1.0, default 0.90 for high confidence)
    #[arg(long, short, default_value = "0.90")]
    pub threshold: f32,
    /// Use semantic deduplication (if enabled in config) for better matching
    #[arg(long)]
    pub use_semantic: bool,
    /// Dry-run: show what would be merged without making changes
    #[arg(long)]
    pub dry_run: bool,
    /// Skip preview and merge immediately
    #[arg(long)]
    pub force: bool,
}

// ─── Edge subcommands ─────────────────────────────────────────────────────────

#[derive(Args)]
pub struct OntologyLinkArgs {
    /// Source ID (concept or memory)
    pub from: String,
    /// Source kind: concept | memory
    #[arg(long, default_value = "concept")]
    pub from_kind: String,
    /// Relation type: IS_A, INSTANCE_OF, HAS_PROPERTY, or any existing EdgeType
    pub rel: String,
    /// Target ID (concept or memory)
    pub to: String,
    /// Target kind: concept | memory
    #[arg(long, default_value = "concept")]
    pub to_kind: String,
    /// Optional note
    #[arg(long)]
    pub note: Option<String>,
}

#[derive(Args)]
pub struct OntologyUnlinkArgs {
    /// Edge ID (integer from 'voidm ontology edges <id>')
    pub edge_id: i64,
}

#[derive(Args)]
pub struct OntologyEdgesArgs {
    /// Concept or memory ID
    pub id: String,
}

#[derive(Args)]
pub struct HierarchyArgs {
    /// Concept ID or short prefix
    pub id: String,
}

#[derive(Args)]
pub struct InstancesArgs {
    /// Concept ID or short prefix
    pub id: String,
}

// ─── Dispatch ─────────────────────────────────────────────────────────────────

pub async fn run(
    cmd: OntologyCommands,
    pool: &SqlitePool,
    config: &Config,
    json: bool,
) -> Result<()> {
    match cmd {
        OntologyCommands::Concept(sub) => run_concept(sub, pool, config, json).await,
        OntologyCommands::Link(args) => run_link(args, pool, json).await,
        OntologyCommands::Unlink(args) => run_unlink(args, pool, json).await,
        OntologyCommands::Edges(args) => run_edges(args, pool, json).await,
        OntologyCommands::Hierarchy(args) => run_hierarchy(args, pool, json).await,
        OntologyCommands::Instances(args) => run_instances(args, pool, json).await,
        OntologyCommands::Enrich(args) => run_enrich(args, pool, config, json).await,
        OntologyCommands::Benchmark => run_benchmark(json).await,
        OntologyCommands::Extract(args) => run_extract(args, pool, json).await,
        OntologyCommands::EnrichMemories(args) => run_enrich_memories(args, pool, json).await,
        OntologyCommands::AutoImprove(args) => run_auto_improve(args, pool, json).await,
    }
}

// ─── Concept handlers ─────────────────────────────────────────────────────────

async fn run_concept(
    cmd: ConceptCommands,
    pool: &SqlitePool,
    config: &Config,
    json: bool,
) -> Result<()> {
    match cmd {
        ConceptCommands::Add(args) => {
            let concept = ontology::add_concept(
                pool,
                &args.name,
                args.description.as_deref(),
                args.scope.as_deref(),
            )
            .await?;

            // NLI enrichment if requested
            let suggestions = if args.enrich {
                let concept_text = format!(
                    "{}{}{}",
                    &concept.name,
                    concept
                        .description
                        .as_ref()
                        .map(|d| format!(" - {}", d))
                        .unwrap_or_default(),
                    concept
                        .scope
                        .as_ref()
                        .map(|s| format!(" ({})", s))
                        .unwrap_or_default()
                );
                run_enrichment_for_concept(&concept.id, &concept_text, pool, config, 10).await
            } else {
                vec![]
            };

            if json {
                let mut resp = serde_json::to_value(&concept)?;
                resp["suggested_relations"] = serde_json::to_value(&suggestions)?;
                println!("{}", serde_json::to_string_pretty(&resp)?);
            } else {
                println!("Concept added: {} ({})", concept.name, &concept.id[..8]);
                if let Some(ref d) = concept.description {
                    println!("  Description: {}", d);
                }
                if let Some(ref s) = concept.scope {
                    println!("  Scope: {}", s);
                }

                // Show similar concepts warning
                if !concept.similar_concepts.is_empty() {
                    println!("\n⚠ Similar concepts found (consider merging):");
                    for sim in &concept.similar_concepts {
                        println!("  [{}] {} ({:.1}% similar, {} edges) — voidm ontology concept merge {} {}",
                            &sim.id[..8], sim.name, sim.similarity * 100.0, sim.edge_count,
                            &concept.id[..8], &sim.id[..8]);
                    }
                }

                if !suggestions.is_empty() {
                    println!("\nSuggested relations ({}):", suggestions.len());
                    for s in &suggestions {
                        println!(
                            "  [{:.2}] {} --[{}]--> {} \"{}\"",
                            s.confidence,
                            &concept.id[..8],
                            s.suggested_rel,
                            &s.candidate_id[..8.min(s.candidate_id.len())],
                            &s.candidate_text[..60.min(s.candidate_text.len())]
                        );
                    }
                    println!("Use 'voidm ontology link' to confirm any of the above.");
                }
            }
        }
        ConceptCommands::Get(args) => {
            let concept = ontology::get_concept_with_instances(pool, &args.id).await?;
            if json {
                println!("{}", serde_json::to_string_pretty(&concept)?);
            } else {
                println!("[{}] {}", &concept.id[..8], concept.name);
                if let Some(ref d) = concept.description {
                    println!("  {}", d);
                }
                if let Some(ref s) = concept.scope {
                    println!("  scope: {}", s);
                }
                println!("  created: {}", concept.created_at);
                if !concept.superclasses.is_empty() {
                    println!(
                        "  IS_A: {}",
                        concept
                            .superclasses
                            .iter()
                            .map(|c| c.name.as_str())
                            .collect::<Vec<_>>()
                            .join(", ")
                    );
                }
                if !concept.subclasses.is_empty() {
                    println!(
                        "  Subclasses: {}",
                        concept
                            .subclasses
                            .iter()
                            .map(|c| c.name.as_str())
                            .collect::<Vec<_>>()
                            .join(", ")
                    );
                }
                if concept.instances.is_empty() {
                    println!("  Instances: none");
                } else {
                    println!("  Instances ({}):", concept.instances.len());
                    for inst in &concept.instances {
                        println!("    [{}] {}", &inst.memory_id[..8], inst.preview);
                    }
                }
            }
        }
        ConceptCommands::List(args) => {
            let concepts = ontology::list_concepts(pool, args.scope.as_deref(), args.limit).await?;
            if json {
                println!("{}", serde_json::to_string_pretty(&concepts)?);
            } else {
                if concepts.is_empty() {
                    println!(
                        "No concepts found. Use 'voidm ontology concept add <name>' to create one."
                    );
                } else {
                    for c in &concepts {
                        let scope_str = c
                            .scope
                            .as_deref()
                            .map(|s| format!(" ({})", s))
                            .unwrap_or_default();
                        let desc_str = c
                            .description
                            .as_deref()
                            .map(|d| {
                                if d.len() > 60 {
                                    format!(" — {}…", &d[..60])
                                } else {
                                    format!(" — {}", d)
                                }
                            })
                            .unwrap_or_default();
                        println!("[{}]{} {}{}", &c.id[..8], scope_str, c.name, desc_str);
                    }
                    println!("{} concept(s)", concepts.len());
                }
            }
        }
        ConceptCommands::Delete(args) => {
            let deleted = ontology::delete_concept(pool, &args.id).await?;
            if json {
                println!(
                    "{}",
                    serde_json::json!({ "deleted": deleted, "id": args.id })
                );
            } else if deleted {
                println!("Concept '{}' deleted.", args.id);
            } else {
                eprintln!("Concept '{}' not found.", args.id);
                std::process::exit(1);
            }
        }

        ConceptCommands::Merge(args) => {
            let result = ontology::merge_concepts(pool, &args.source, &args.target).await?;
            if json {
                println!("{}", serde_json::to_string(&result)?);
            } else {
                println!(
                    "Merged concept '{}' into '{}'",
                    result.source_name, result.target_name
                );
                println!("  Memory edges retargeted: {}", result.memory_edges_merged);
            }
        }

        ConceptCommands::FindMergeCandidates(args) => {
            let candidates = ontology::find_merge_candidates(pool, args.threshold).await?;

            // Prepare JSON output
            let json_output = serde_json::to_string_pretty(&candidates)?;

            // Write to file if --output specified
            if let Some(output_path) = &args.output {
                std::fs::write(output_path, &json_output)?;
                if !json {
                    println!(
                        "✓ Wrote {} merge candidates to {}",
                        candidates.len(),
                        output_path
                    );
                }
            } else if json {
                println!("{}", json_output);
            } else {
                if candidates.is_empty() {
                    println!(
                        "No merge candidates found at similarity >= {}",
                        args.threshold
                    );
                } else {
                    println!(
                        "Found {} merge candidates (similarity >= {}):\n",
                        candidates.len(),
                        args.threshold
                    );
                    for (idx, candidate) in candidates.iter().enumerate() {
                        println!(
                            "{}. [{}] {} ({} edges) → [{}] {} ({} edges)",
                            idx + 1,
                            candidate.source_id.chars().take(8).collect::<String>(),
                            candidate.source_name,
                            candidate.source_edges,
                            candidate.target_id.chars().take(8).collect::<String>(),
                            candidate.target_name,
                            candidate.target_edges
                        );
                        println!("   Similarity: {:.2}%", candidate.similarity * 100.0);
                        println!(
                            "   Action: voidm ontology concept merge {} {}\n",
                            candidate.source_id.chars().take(8).collect::<String>(),
                            candidate.target_id.chars().take(8).collect::<String>()
                        );
                    }
                }
            }
        }
        ConceptCommands::MergeBatch(args) => {
            handle_merge_batch(&args.from, args.execute, pool, json).await?;
        }
        ConceptCommands::MergeBatchApply(args) => {
            handle_merge_batch_apply(&args.batch_id, pool, json).await?;
        }
        ConceptCommands::RollbackMerge(args) => {
            handle_rollback_merge(&args.merge_id, pool, json).await?;
        }
        ConceptCommands::MergeHistory(args) => {
            handle_merge_history(args.batch.as_deref(), args.status.as_deref(), pool, json).await?;
        }
        ConceptCommands::AutoMerge(args) => {
            handle_automerge(
                args.threshold,
                args.use_semantic,
                args.dry_run,
                args.force,
                pool,
                json,
            )
            .await?;
        }
    }
    Ok(())
}

// ─── Edge handlers ────────────────────────────────────────────────────────────

async fn run_link(args: OntologyLinkArgs, pool: &SqlitePool, json: bool) -> Result<()> {
    let from_kind: NodeKind = args.from_kind.parse()?;
    let to_kind: NodeKind = args.to_kind.parse()?;
    let rel: OntologyRelType = args.rel.parse()?;

    // Resolve short IDs
    let from_id = resolve_node_id(pool, &args.from, &from_kind).await?;
    let to_id = resolve_node_id(pool, &args.to, &to_kind).await?;

    let edge = ontology::add_ontology_edge(
        pool,
        &from_id,
        from_kind,
        &rel,
        &to_id,
        to_kind,
        args.note.as_deref(),
    )
    .await?;

    if json {
        println!("{}", serde_json::to_string_pretty(&edge)?);
    } else {
        println!(
            "Linked: {} ({}) --[{}]--> {} ({})",
            &edge.from_id[..8],
            edge.from_type,
            edge.rel_type,
            &edge.to_id[..8],
            edge.to_type
        );
    }
    Ok(())
}

async fn run_unlink(args: OntologyUnlinkArgs, pool: &SqlitePool, json: bool) -> Result<()> {
    let deleted = ontology::delete_ontology_edge(pool, args.edge_id).await?;
    if json {
        println!(
            "{}",
            serde_json::json!({ "deleted": deleted, "edge_id": args.edge_id })
        );
    } else if deleted {
        println!("Ontology edge {} removed.", args.edge_id);
    } else {
        eprintln!("Edge {} not found.", args.edge_id);
        std::process::exit(1);
    }
    Ok(())
}

async fn run_edges(args: OntologyEdgesArgs, pool: &SqlitePool, json: bool) -> Result<()> {
    // Try to resolve as concept first, then as memory
    let full_id = match ontology::resolve_concept_id(pool, &args.id).await {
        Ok(id) => id,
        Err(_) => voidm_core::resolve_id(pool, &args.id).await?,
    };
    let edges = ontology::list_ontology_edges(pool, &full_id).await?;
    if json {
        println!("{}", serde_json::to_string_pretty(&edges)?);
    } else {
        if edges.is_empty() {
            println!("No ontology edges for '{}'.", args.id);
        } else {
            for e in &edges {
                println!(
                    "[{}] {} ({}) --[{}]--> {} ({})",
                    e.id,
                    &e.from_id[..8.min(e.from_id.len())],
                    e.from_type,
                    e.rel_type,
                    &e.to_id[..8.min(e.to_id.len())],
                    e.to_type,
                );
            }
            println!("{} edge(s)", edges.len());
        }
    }
    Ok(())
}

// ─── Hierarchy handler ────────────────────────────────────────────────────────

async fn run_hierarchy(args: HierarchyArgs, pool: &SqlitePool, json: bool) -> Result<()> {
    let concept = ontology::get_concept(pool, &args.id).await?;
    let nodes = ontology::concept_hierarchy(pool, &concept.id).await?;

    if json {
        println!("{}", serde_json::to_string_pretty(&nodes)?);
        return Ok(());
    }

    let ancestors: Vec<_> = nodes
        .iter()
        .filter(|n| matches!(n.direction, HierarchyDirection::Ancestor))
        .collect();
    let descendants: Vec<_> = nodes
        .iter()
        .filter(|n| matches!(n.direction, HierarchyDirection::Descendant))
        .collect();

    if ancestors.is_empty() && descendants.is_empty() {
        println!("'{}' has no IS_A connections yet.", concept.name);
        println!("Use 'voidm ontology link <id> IS_A <parent-id>' to build the hierarchy.");
        return Ok(());
    }

    if !ancestors.is_empty() {
        println!("Ancestors (IS_A chain upward):");
        for n in &ancestors {
            println!(
                "  {:indent$}{} [{}]",
                "",
                n.name,
                &n.id[..8],
                indent = (n.depth as usize - 1) * 2
            );
        }
    }

    println!("  → {} (self)", concept.name);

    if !descendants.is_empty() {
        println!("Descendants (subclasses):");
        for n in &descendants {
            println!(
                "  {:indent$}{} [{}]",
                "",
                n.name,
                &n.id[..8],
                indent = (n.depth as usize - 1) * 2
            );
        }
    }

    Ok(())
}

// ─── Instances handler ────────────────────────────────────────────────────────

async fn run_instances(args: InstancesArgs, pool: &SqlitePool, json: bool) -> Result<()> {
    let concept = ontology::get_concept(pool, &args.id).await?;
    let instances = ontology::concept_instances(pool, &concept.id).await?;

    if json {
        println!("{}", serde_json::to_string_pretty(&instances)?);
        return Ok(());
    }

    if instances.is_empty() {
        println!(
            "No instances of '{}' (including subclasses) found.",
            concept.name
        );
        println!(
            "Use 'voidm ontology link <id> --from-kind memory INSTANCE_OF {}' to link a memory.",
            &concept.id[..8]
        );
    } else {
        println!("Instances of '{}' (including subclasses):", concept.name);
        for inst in &instances {
            let via = if inst.concept_id != concept.id {
                format!(" (via subclass {})", &inst.concept_id[..8])
            } else {
                String::new()
            };
            println!(
                "  [{}] {} {}{}",
                &inst.instance_id[..8.min(inst.instance_id.len())],
                inst.instance_kind,
                inst.note
                    .as_deref()
                    .map(|n| format!("— {}", n))
                    .unwrap_or_default(),
                via
            );
        }
        println!("{} instance(s)", instances.len());
    }
    Ok(())
}

// ─── ID resolution helpers ────────────────────────────────────────────────────

async fn resolve_node_id(pool: &SqlitePool, id: &str, kind: &NodeKind) -> Result<String> {
    match kind {
        NodeKind::Concept => ontology::resolve_concept_id(pool, id).await,
        NodeKind::Memory => voidm_core::resolve_id(pool, id).await,
    }
}

// ─── NLI enrichment ──────────────────────────────────────────────────────────

/// Build a text representation for NLI scoring from a concept.

/// Run NLI enrichment for a single concept against all other concepts.
/// Returns relation suggestions sorted by confidence.
async fn run_enrichment_for_concept(
    concept_id: &str,
    concept_text: &str,
    pool: &SqlitePool,
    config: &Config,
    top_k: usize,
) -> Vec<voidm_core::nli::RelationSuggestion> {
    // Ensure model is loaded
    if let Err(e) = voidm_core::nli::ensure_nli_model().await {
        eprintln!(
            "Warning: NLI model load failed: {}. Skipping enrichment.",
            e
        );
        return vec![];
    }

    // Get all other concepts
    let candidates = match ontology::list_concepts(pool, None, 500).await {
        Ok(cs) => cs,
        Err(e) => {
            tracing::warn!("Failed to list concepts for enrichment: {}", e);
            return vec![];
        }
    };

    // Build candidate list: (id, text, similarity)
    // Use embedding similarity if available, else default to 0.5
    let mut scored_candidates: Vec<(String, String, f32)> = candidates
        .into_iter()
        .filter(|c| c.id != concept_id)
        .map(|c| {
            let text = concept_text_from(&c);
            (c.id, text, 0.5_f32) // similarity placeholder — real cosine would require embeddings
        })
        .collect();

    // If embeddings available, compute actual cosine similarity
    if config.embeddings.enabled {
        if let Ok(query_emb) =
            voidm_core::embeddings::embed_text(&config.embeddings.model, concept_text)
        {
            for (_id, text, sim) in &mut scored_candidates {
                if let Ok(emb) = voidm_core::embeddings::embed_text(&config.embeddings.model, text)
                {
                    *sim = cosine_similarity(&query_emb, &emb);
                }
            }
        }
    }

    // Sort by similarity, take top_k
    scored_candidates.sort_by(|a, b| b.2.partial_cmp(&a.2).unwrap_or(std::cmp::Ordering::Equal));
    scored_candidates.truncate(top_k);

    voidm_core::nli::suggest_relations(concept_text, &scored_candidates)
}

fn concept_text_from(c: &ontology::Concept) -> String {
    match &c.description {
        Some(d) => format!("{}: {}", c.name, d),
        None => c.name.clone(),
    }
}

fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() {
        return 0.0;
    }
    let dot: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
    let norm_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let norm_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm_a == 0.0 || norm_b == 0.0 {
        0.0
    } else {
        dot / (norm_a * norm_b)
    }
}

async fn run_enrich(
    args: EnrichArgs,
    pool: &SqlitePool,
    config: &Config,
    json: bool,
) -> Result<()> {
    let concepts = ontology::list_concepts(pool, None, 1000).await?;
    if concepts.is_empty() {
        if json {
            println!(
                "{}",
                serde_json::json!({ "enriched": 0, "message": "No concepts to enrich." })
            );
        } else {
            println!("No concepts to enrich.");
        }
        return Ok(());
    }

    println!(
        "Enriching {} concept(s) with NLI relation suggestions …",
        concepts.len()
    );

    let mut all_suggestions: Vec<serde_json::Value> = vec![];
    for concept in &concepts {
        let text = concept_text_from(concept);
        let suggestions =
            run_enrichment_for_concept(&concept.id, &text, pool, config, args.top_k).await;

        if !suggestions.is_empty() {
            if json {
                all_suggestions.push(serde_json::json!({
                    "concept_id": concept.id,
                    "concept_name": concept.name,
                    "suggestions": serde_json::to_value(&suggestions)?
                }));
            } else {
                println!("\n[{}] {}:", &concept.id[..8], concept.name);
                for s in &suggestions {
                    println!(
                        "  [{:.2}] --[{}]--> {} ({}) \"{}\"",
                        s.confidence,
                        s.suggested_rel,
                        &s.candidate_id[..8.min(s.candidate_id.len())],
                        s.suggested_rel,
                        &s.candidate_text[..60.min(s.candidate_text.len())]
                    );
                }
            }
        }
    }

    if json {
        println!("{}", serde_json::to_string_pretty(&all_suggestions)?);
    } else {
        println!("\nDone. Use 'voidm ontology link' to confirm suggested relations.");
    }
    Ok(())
}

async fn run_benchmark(json: bool) -> Result<()> {
    println!("Loading NLI model …");
    voidm_core::nli::ensure_nli_model().await?;

    let avg_ms = voidm_core::nli::benchmark_latency(10)?;
    if json {
        println!("{}", serde_json::json!({ "avg_ms": avg_ms, "runs": 10 }));
    } else {
        println!("NLI inference latency: {:.1}ms avg (10 runs)", avg_ms);
        if avg_ms < 200.0 {
            println!("✓ Fast enough for synchronous enrichment on insert.");
        } else {
            println!("⚠ Latency > 200ms — recommend using --enrich flag explicitly.");
        }
    }
    Ok(())
}

async fn run_extract(args: ExtractArgs, pool: &SqlitePool, json: bool) -> Result<()> {
    // Ensure model is loaded
    voidm_core::ner::ensure_ner_model().await?;

    // Extract entities
    let entities = voidm_core::ner::extract_entities(&args.text)?;

    // Filter by min_score
    let filtered: Vec<_> = entities
        .iter()
        .filter(|e| e.score >= args.min_score)
        .collect();

    if filtered.is_empty() {
        if json {
            println!(
                "{}",
                serde_json::json!({ "candidates": [], "message": "No entities found above threshold." })
            );
        } else {
            println!(
                "No entities found above score threshold {:.2}.",
                args.min_score
            );
            println!("Try lowering --min-score or providing more descriptive text.");
        }
        return Ok(());
    }

    // Check against existing concepts
    let entities_owned: Vec<voidm_core::ner::NamedEntity> =
        filtered.iter().map(|e| (*e).clone()).collect();
    let candidates = voidm_core::ner::entities_to_candidates(&entities_owned, pool).await?;

    if json {
        println!("{}", serde_json::to_string_pretty(&candidates)?);
        if !args.add {
            return Ok(());
        }
    } else {
        println!("Extracted {} candidate(s):", candidates.len());
        for c in &candidates {
            let status = if c.already_exists {
                format!(
                    " [exists: {}]",
                    c.existing_id
                        .as_deref()
                        .unwrap_or("?")
                        .get(..8)
                        .unwrap_or("?")
                )
            } else {
                String::new()
            };
            println!(
                "  [{:.2}] {:5} {}{}",
                c.score, c.entity_type, c.name, status
            );
        }
    }

    // Auto-add if --add flag given
    if args.add {
        let new_candidates: Vec<_> = candidates.iter().filter(|c| !c.already_exists).collect();
        if new_candidates.is_empty() {
            if !json {
                println!("All candidates already exist as concepts.");
            }
            return Ok(());
        }

        if !json {
            println!("\nAdding {} new concept(s):", new_candidates.len());
        }

        let mut added = Vec::new();
        for c in &new_candidates {
            match ontology::add_concept(pool, &c.name, None, args.scope.as_deref()).await {
                Ok(concept) => {
                    if !json {
                        println!(
                            "  ✓ {} [{}] ({})",
                            concept.name,
                            &concept.id[..8],
                            c.entity_type
                        );
                    }
                    added.push(concept);
                }
                Err(e) => {
                    if !json {
                        eprintln!("  ✗ {}: {}", c.name, e);
                    }
                }
            }
        }

        if json {
            println!("{}", serde_json::to_string_pretty(&added)?);
        } else {
            println!(
                "\n{} concept(s) added. Use 'voidm ontology link' to build the hierarchy.",
                added.len()
            );
        }
    } else if !json {
        println!(
            "\nUse 'voidm ontology extract \"...\" --add' to automatically add new candidates."
        );
        println!("Or 'voidm ontology concept add \"<name>\"' to add individually.");
    }

    Ok(())
}

// ─── enrich-memories ──────────────────────────────────────────────────────────

async fn run_enrich_memories(
    args: EnrichMemoriesArgs,
    pool: &SqlitePool,
    json: bool,
) -> Result<()> {
    // Ensure NER model is loaded (downloads ~103MB on first use)
    if !json {
        if !voidm_core::ner::ner_model_downloaded() {
            eprintln!("Downloading NER model (~103MB, first use only) …");
        }
    }
    voidm_core::ner::ensure_ner_model().await?;

    let opts = voidm_core::ontology::EnrichMemoriesOpts {
        scope: args.scope.as_deref(),
        min_score: args.min_score,
        add: true, // ALWAYS add new concepts (default behavior changed)
        force: args.force,
        dry_run: args.dry_run,
        limit: args.limit,
    };

    let results = voidm_core::ontology::enrich_memories(pool, &opts).await?;

    if json {
        println!("{}", serde_json::to_string_pretty(&results)?);
        return Ok(());
    }

    // Human-readable output
    let total = results.len();
    let skipped = results.iter().filter(|r| r.skipped).count();
    let processed = results.iter().filter(|r| !r.skipped).count();
    let total_links: usize = results.iter().map(|r| r.links_created).sum();
    let total_created: usize = results.iter().map(|r| r.concepts_created.len()).sum();

    if args.dry_run {
        println!("DRY RUN — no changes written.\n");
    }

    for (i, r) in results.iter().filter(|r| !r.skipped).enumerate() {
        let status = if r.entities_found == 0 {
            "no entities".to_string()
        } else {
            let mut parts = Vec::new();
            if !r.concepts_linked.is_empty() {
                parts.push(format!("linked: {}", r.concepts_linked.join(", ")));
            }
            if !r.concepts_created.is_empty() {
                parts.push(format!("created: {}", r.concepts_created.join(", ")));
            }
            if parts.is_empty() {
                format!(
                    "{} entities, 0 links (no matching concepts)",
                    r.entities_found
                )
            } else {
                parts.join(" | ")
            }
        };
        println!("[{}/{}] {} → {}", i + 1, processed, r.preview, status,);
    }

    if skipped > 0 {
        println!("\n{} already processed (use --force to re-run).", skipped);
    }

    println!(
        "\nDone: {}/{} memories processed, {} link(s) created, {} concept(s) created.",
        processed, total, total_links, total_created,
    );

    // Auto-dedup newly created concepts (if not dry-run)
    if !args.dry_run && total_created > 0 {
        if !json {
            println!("\nAuto-deduplicating newly created concepts...");
        }
        let candidates = ontology::find_merge_candidates(pool, 0.90).await?;
        if candidates.len() > 0 {
            let plan = voidm_core::models::MergePlan {
                merges: candidates
                    .iter()
                    .map(|c| voidm_core::models::MergePair {
                        source: c.source_id.clone(),
                        target: c.target_id.clone(),
                    })
                    .collect(),
            };
            let batch_id = Uuid::new_v4().to_string();
            if let Ok(result) = ontology::execute_merge_batch(pool, &batch_id, &plan).await {
                if !json {
                    println!("✓ Deduplicated {} concept pairs", result.succeeded);
                }
            }
        } else {
            if !json {
                println!("✓ No duplicates found");
            }
        }
    }

    Ok(())
}

// ─── Batch merge handlers ─────────────────────────────────────────────────────

async fn handle_merge_batch(
    path: &str,
    execute: bool,
    pool: &SqlitePool,
    json: bool,
) -> Result<()> {
    use std::fs;
    use voidm_core::models::MergePlan;

    // Load and parse merge plan JSON
    let plan_str = fs::read_to_string(path)?;

    // Try to parse as MergePlan first; if that fails, try MergeCandidate array and convert
    let plan: MergePlan = match serde_json::from_str(&plan_str) {
        Ok(p) => p,
        Err(_) => {
            // Try parsing as array of MergeCandidate (from find-merge-candidates output)
            let candidates: Vec<voidm_core::ontology::MergeCandidate> =
                serde_json::from_str(&plan_str)?;

            // Convert to MergePlan
            let merges = candidates
                .into_iter()
                .map(|c| voidm_core::models::MergePair {
                    source: c.source_id,
                    target: c.target_id,
                })
                .collect();

            MergePlan { merges }
        }
    };

    if execute {
        // Execute the merge batch in a transaction
        let batch_id = Uuid::new_v4().to_string();
        let result = ontology::execute_merge_batch(pool, &batch_id, &plan).await?;

        if json {
            println!("{}", serde_json::to_string_pretty(&result)?);
        } else {
            println!("Batch Merge Execution Complete:");
            println!("────────────────────────────────");
            println!("Batch ID: {}", result.batch_id);
            println!("Total merges: {}", result.total);
            println!("✓ Succeeded: {}", result.succeeded);
            if result.failed > 0 {
                println!("✗ Failed: {}", result.failed);
                for (source, target, reason) in &result.errors {
                    println!("  - {} → {}: {}", &source[..8], &target[..8], reason);
                }
            }
            if result.conflicts > 0 {
                println!("⚠ Conflicts kept: {}", result.conflicts);
            }
            println!("Edges retargeted: {}", result.edges_retargeted);
        }
    } else {
        // Dry-run preview only
        let result = ontology::analyze_merge_plan(pool, &plan).await?;

        if json {
            println!("{}", serde_json::to_string_pretty(&result)?);
        } else {
            println!("Merge Plan Preview (Dry-run):");
            println!("────────────────────────────────");
            println!("Batch ID: {}", result.batch_id);
            println!("Total merges: {}", result.total);
            println!("✓ Will succeed: {}", result.succeeded);
            if result.failed > 0 {
                println!("✗ Will fail: {}", result.failed);
                for (source, target, reason) in &result.errors {
                    println!("  - {} → {}: {}", &source[..8], &target[..8], reason);
                }
            }
            if result.conflicts > 0 {
                println!(
                    "⚠ Conflicts detected: {} (both have CONTRADICTS)",
                    result.conflicts
                );
                println!("  → Will keep both CONTRADICTS edges on target");
            }
            println!("Edges to retarget: {}", result.edges_retargeted);
            println!("\nStatus: This is a dry-run preview. To execute, use --execute flag:");
            println!(
                "voidm ontology concept merge-batch --from {} --execute",
                path
            );
        }
    }

    Ok(())
}

async fn handle_merge_batch_apply(_batch_id: &str, _pool: &SqlitePool, _json: bool) -> Result<()> {
    // Load the batch ID from cache (in real implementation, would load from DB or file)
    // For now, we'll create a minimal plan structure

    // This is a placeholder - in production, batch_id would be tied to a saved plan
    eprintln!("Error: merge-batch-apply requires a saved batch context");
    eprintln!("In current implementation, please run merge-batch --from plan.json again");
    eprintln!("(Full batch execution workflow coming in Phase 5.2)");

    Ok(())
}

async fn handle_rollback_merge(merge_id: &str, pool: &SqlitePool, json: bool) -> Result<()> {
    ontology::rollback_merge(pool, merge_id).await?;

    if json {
        println!(
            "{{\"status\": \"rolled_back\", \"merge_id\": \"{}\"}}",
            merge_id
        );
    } else {
        println!("✓ Rolled back merge: {}", merge_id);
    }

    Ok(())
}

async fn handle_merge_history(
    batch_id: Option<&str>,
    status: Option<&str>,
    pool: &SqlitePool,
    json: bool,
) -> Result<()> {
    let entries = ontology::list_merge_history(pool, batch_id, status).await?;

    if json {
        println!("{}", serde_json::to_string_pretty(&entries)?);
    } else {
        if entries.is_empty() {
            println!("No merge history found");
            return Ok(());
        }

        println!("Merge History:");
        println!("──────────────────────────────────────────────────────────────");
        for entry in entries {
            println!(
                "[{}] {} {}/{} | {} edges | status: {}",
                &entry.id[..8],
                &entry.batch_id[..8],
                &entry.source_id[..8],
                &entry.target_id[..8],
                entry.edges_retargeted,
                entry.status
            );
            if entry.conflicts_kept > 0 {
                println!("      ⚠ {} CONTRADICTS edges kept", entry.conflicts_kept);
            }
            if let Some(reason) = entry.reason {
                println!("      Reason: {}", reason);
            }
            println!("      At: {}", entry.created_at);
        }
    }

    Ok(())
}

async fn handle_automerge(
    threshold: f32,
    use_semantic: bool,
    dry_run: bool,
    force: bool,
    pool: &SqlitePool,
    json: bool,
) -> Result<()> {
    use voidm_core::models::MergePlan;

    // Find merge candidates, optionally using semantic dedup
    let candidates = if use_semantic {
        // Load config to get semantic dedup settings
        let config = voidm_core::Config::load();
        ontology::find_merge_candidates_with_semantic(pool, threshold, &config).await?
    } else {
        ontology::find_merge_candidates(pool, threshold).await?
    };

    if candidates.is_empty() {
        if !json {
            println!(
                "✓ Database is clean: no duplicate concepts found above {:.0}% similarity",
                threshold * 100.0
            );
        } else {
            println!("{{\"status\": \"clean\", \"candidates\": 0}}");
        }
        return Ok(());
    }

    // Convert to merge plan
    let plan = MergePlan {
        merges: candidates
            .iter()
            .map(|c| voidm_core::models::MergePair {
                source: c.source_id.clone(),
                target: c.target_id.clone(),
            })
            .collect(),
    };

    // Show preview unless --force is set
    if !force && !dry_run {
        if !json {
            println!("Auto-Merge Preview:");
            println!("───────────────────────────────────────────────────────────");
            let dedup_info = if use_semantic {
                " (semantic dedup enabled)"
            } else {
                ""
            };
            println!(
                "Found {} duplicate concept pairs above {:.0}% similarity{}",
                candidates.len(),
                threshold * 100.0,
                dedup_info
            );
            println!();
            for (idx, candidate) in candidates.iter().enumerate() {
                println!(
                    "{}. [{}] {} ({} edges) → [{}] {} ({} edges)",
                    idx + 1,
                    candidate.source_id.chars().take(8).collect::<String>(),
                    candidate.source_name,
                    candidate.source_edges,
                    candidate.target_id.chars().take(8).collect::<String>(),
                    candidate.target_name,
                    candidate.target_edges
                );
                println!("   Similarity: {:.1}%\n", candidate.similarity * 100.0);
            }
            println!(
                "Execute merge with: voidm ontology concept automerge --threshold {} --force",
                threshold
            );
        } else {
            println!(
                "{{\"status\": \"preview\", \"candidates\": {}, \"threshold\": {}}}",
                candidates.len(),
                threshold
            );
        }
        return Ok(());
    }

    // Execute merge
    let batch_id = Uuid::new_v4().to_string();
    let result = ontology::execute_merge_batch(pool, &batch_id, &plan).await?;

    if json {
        println!("{}", serde_json::to_string_pretty(&result)?);
    } else {
        println!("Auto-Merge Complete:");
        println!("───────────────────────────────────────────────────────────");
        println!("✓ Batch ID: {}", result.batch_id);
        println!(
            "✓ Merged: {}/{} concept pairs",
            result.succeeded, result.total
        );
        if result.failed > 0 {
            println!("⚠ Failed: {}", result.failed);
            for (source, target, reason) in &result.errors {
                println!("  - {} → {}: {}", &source[..8], &target[..8], reason);
            }
        }
        if result.conflicts > 0 {
            println!(
                "⚠ Conflicts kept: {} (both CONTRADICTS edges preserved)",
                result.conflicts
            );
        }
        println!("Edges retargeted: {}", result.edges_retargeted);
        println!("\nDatabase improved. Run again to check for more duplicates.");
    }

    Ok(())
}

// ─── Auto-improve handler ──────────────────────────────────────────────────────

async fn run_auto_improve(args: AutoImproveArgs, pool: &SqlitePool, json: bool) -> Result<()> {
    if !json && !args.merge_only {
        println!("Auto-Improve: Enriching memories + Auto-merging duplicates");
        println!("═══════════════════════════════════════════════════════════\n");
    } else if !json && args.merge_only {
        println!("Auto-Improve: Merging duplicate concepts");
        println!("═══════════════════════════════════════════════════════════\n");
    }

    // Step 1: Enrich memories with auto-add (skip if --merge-only)
    if !args.merge_only {
        if !json {
            println!("Step 1: Enriching memories...");
        }
        let enrich_args = EnrichMemoriesArgs {
            scope: args.scope.clone(),
            min_score: args.min_score,
            add: true,         // Always auto-add new concepts
            force: args.force, // Pass through the force flag
            dry_run: args.dry_run,
            limit: 0, // Process all
        };

        if args.dry_run {
            if !json {
                println!("(dry-run mode: no changes will be written)\n");
            }
            return Ok(());
        }

        // Run enrich_memories
        match run_enrich_memories(enrich_args, pool, json).await {
            Ok(_) => {
                if !json {
                    println!("\n✓ Memory enrichment complete\n");
                }
            }
            Err(e) => {
                if !json {
                    println!("\n⚠ Enrichment had warnings (continuing): {}\n", e);
                }
            }
        }
    } else if args.dry_run {
        if !json {
            println!("(dry-run mode: no changes will be written)\n");
        }
        return Ok(());
    }

    // Step 2: Auto-merge duplicates (always run this)
    if !json {
        println!(
            "Step {}: Auto-merging similar concepts...",
            if args.merge_only { 1 } else { 2 }
        );
    }

    let candidates = ontology::find_merge_candidates(pool, args.threshold).await?;

    if candidates.is_empty() {
        if !json {
            println!(
                "✓ No duplicates found above {:.0}% similarity\n",
                args.threshold * 100.0
            );
            println!("═══════════════════════════════════════════════════════════");
            println!("Database is clean and optimized.");
        } else {
            println!("{{\"status\":\"ok\",\"duplicates\":0}}");
        }
        return Ok(());
    }

    // Convert to merge plan and execute
    let plan = voidm_core::models::MergePlan {
        merges: candidates
            .iter()
            .map(|c| voidm_core::models::MergePair {
                source: c.source_id.clone(),
                target: c.target_id.clone(),
            })
            .collect(),
    };

    // For auto-improve, auto-execute unless --dry-run
    if args.dry_run {
        // Show preview in dry-run mode
        if !json {
            println!(
                "Found {} duplicate concept pairs above {:.0}% similarity\n",
                candidates.len(),
                args.threshold * 100.0
            );
            for (idx, candidate) in candidates.iter().take(5).enumerate() {
                println!(
                    "{}. [{}] {} → [{}] {} ({}% similar)",
                    idx + 1,
                    candidate.source_id.chars().take(8).collect::<String>(),
                    candidate.source_name,
                    candidate.target_id.chars().take(8).collect::<String>(),
                    candidate.target_name,
                    (candidate.similarity * 100.0) as i32
                );
            }
            if candidates.len() > 5 {
                println!("... and {} more", candidates.len() - 5);
            }
            println!("\n(dry-run: no changes will be made)");
        } else {
            println!(
                "{{\"status\":\"preview\",\"duplicates\":{}}}",
                candidates.len()
            );
        }
        return Ok(());
    }

    // Execute merge
    let batch_id = Uuid::new_v4().to_string();
    let result = ontology::execute_merge_batch(pool, &batch_id, &plan).await?;

    if json {
        // Agent-friendly JSON: minimal, single line
        println!(
            "{{\"status\":\"ok\",\"merged\":{},\"conflicts\":{}}}",
            result.succeeded, result.conflicts
        );
    } else {
        println!("✓ Merged {} concept pairs\n", result.succeeded);
        if result.failed > 0 {
            println!("⚠ Failed: {}", result.failed);
        }
        if result.conflicts > 0 {
            println!(
                "⚠ Conflicts: {} CONTRADICTS edges preserved",
                result.conflicts
            );
        }

        println!("\n═══════════════════════════════════════════════════════════");
        println!("Auto-Improve Complete:");
        if !args.merge_only {
            println!("✓ Memories enriched with new concepts");
        }
        println!("✓ Database deduplicated");
        println!("\nRun again to check for more opportunities.");
    }

    Ok(())
}
