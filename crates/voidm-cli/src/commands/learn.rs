use anyhow::{bail, Context, Result};
use clap::{Args, Subcommand};
use sqlx::SqlitePool;
use std::sync::Arc;
use voidm_core::{
    crud,
    db::Database,
    learning::{
        apply_learning_consolidation, consolidate_learning_tips, default_memory_type_for_learning,
        extract_learning_candidates, memory_to_learning_record, parse_learning_trajectories,
        search_learning_tips, LearningConsolidationApplied, LearningConsolidationCluster,
        LearningConsolidationRequest, LearningMemoryRecord, LearningSearchFilters,
        LearningSearchRequest, LearningSearchResponse, LearningSourceOutcome, LearningTip,
        LearningTipCategory, LEARNING_TIP_VERSION,
    },
    models::{AddMemoryRequest, AddMemoryResponse, EdgeType, LinkSpec, MemoryType},
    resolve_id,
    search::SearchMode,
    Config,
};

#[derive(Subcommand)]
pub enum LearnCommands {
    /// Add a structured learning tip backed by a memory record
    Add(LearnAddArgs),
    /// Ingest coding-agent trajectories and extract learning tips
    Ingest(LearnIngestArgs),
    /// Consolidate overlapping learning tips into canonical records
    Consolidate(LearnConsolidateArgs),
    /// Search only structured learning tips
    Search(LearnSearchArgs),
    /// Get a learning tip by memory id or prefix
    Get(LearnGetArgs),
}

#[derive(Args)]
pub struct LearnAddArgs {
    /// Generalized learning tip content
    pub content: String,

    /// Learning category: strategy, recovery, optimization
    #[arg(long)]
    pub category: String,

    /// Trigger condition for applying the learning
    #[arg(long)]
    pub trigger: String,

    /// Application context where the learning applies
    #[arg(long = "application-context")]
    pub application_context: String,

    /// Task category such as authentication, deployment, retrieval, parsing
    #[arg(long = "task-category")]
    pub task_category: String,

    /// Optional subtask label for finer-grained retrieval
    #[arg(long)]
    pub subtask: Option<String>,

    /// Outcome that produced the learning: success, recovered_failure, failure, inefficient
    #[arg(long = "source-outcome")]
    pub source_outcome: String,

    /// Source trajectory id. May be repeated.
    #[arg(long = "trajectory")]
    pub trajectory_ids: Vec<String>,

    /// Optional negative example to avoid repeating a failed pattern
    #[arg(long = "negative-example")]
    pub negative_example: Option<String>,

    /// Learning priority from 1 to 10
    #[arg(long, default_value = "5")]
    pub priority: u8,

    /// Underlying memory type override. Defaults from category.
    #[arg(long, short = 't')]
    pub memory_type: Option<String>,

    /// Memory importance 1 to 10. Defaults to learning priority.
    #[arg(long)]
    pub importance: Option<i64>,

    /// Scopes (may be repeated)
    #[arg(long, short = 's')]
    pub scope: Vec<String>,

    /// Tags (comma-separated)
    #[arg(long)]
    pub tags: Option<String>,

    /// Link to existing memory: <id>:<EDGE_TYPE> or <id>:<EDGE_TYPE>:<note>
    #[arg(long = "link")]
    pub links: Vec<String>,
}

#[derive(Args)]
pub struct LearnSearchArgs {
    /// Search query
    pub query: String,

    /// Search mode: hybrid, semantic, keyword, fuzzy, bm25, hybrid-rrf
    #[arg(long, default_value = "hybrid")]
    pub mode: String,

    /// Maximum results
    #[arg(long, default_value = "10")]
    pub limit: usize,

    /// Filter by scope prefix
    #[arg(long)]
    pub scope: Option<String>,

    /// Minimum score threshold. Use 0 to disable filtering.
    #[arg(long)]
    pub min_score: Option<f32>,

    /// Minimum quality score for results
    #[arg(long)]
    pub min_quality: Option<f32>,

    /// Filter by learning category
    #[arg(long)]
    pub category: Option<String>,

    /// Filter by trigger text
    #[arg(long)]
    pub trigger: Option<String>,

    /// Filter by application context text
    #[arg(long = "application-context")]
    pub application_context: Option<String>,

    /// Filter by task category text
    #[arg(long = "task-category")]
    pub task_category: Option<String>,

    /// Filter by subtask text
    #[arg(long)]
    pub subtask: Option<String>,

    /// Filter by source outcome
    #[arg(long = "source-outcome")]
    pub source_outcome: Option<String>,

    /// Filter by source trajectory id
    #[arg(long = "trajectory")]
    pub trajectory_id: Option<String>,

    /// Filter by minimum learning priority
    #[arg(long = "priority-min")]
    pub priority_min: Option<u8>,
}

#[derive(Args)]
pub struct LearnIngestArgs {
    /// Path to a trajectory file. Supports JSON object, JSON array, or JSONL.
    #[arg(long = "from", conflicts_with = "stdin")]
    pub from: Option<String>,

    /// Read trajectory JSON from stdin instead of a file.
    #[arg(long, conflicts_with = "from")]
    pub stdin: bool,

    /// Preview extracted candidates without writing them.
    #[arg(long, conflicts_with = "write")]
    pub dry_run: bool,

    /// Persist extracted candidates as structured learning tips.
    #[arg(long, conflicts_with = "dry_run")]
    pub write: bool,

    /// Override scopes for stored candidates. Replaces scopes from the trajectory input.
    #[arg(long, short = 's')]
    pub scope: Vec<String>,
}

#[derive(Args)]
pub struct LearnConsolidateArgs {
    /// Filter learning tips by scope prefix before consolidation.
    #[arg(long)]
    pub scope: Option<String>,

    /// Filter by learning category before clustering.
    #[arg(long)]
    pub category: Option<String>,

    /// Filter by task category before clustering.
    #[arg(long = "task-category")]
    pub task_category: Option<String>,

    /// Similarity threshold for grouping tips.
    #[arg(long, default_value = "0.82")]
    pub threshold: f32,

    /// Maximum learning tips to inspect.
    #[arg(long, default_value = "500")]
    pub limit: usize,

    /// Preview consolidation clusters without writing canonical records.
    #[arg(long, conflicts_with = "write")]
    pub dry_run: bool,

    /// Persist canonical records and invalidate clustered members.
    #[arg(long, conflicts_with = "dry_run")]
    pub write: bool,
}

#[derive(Args)]
pub struct LearnGetArgs {
    /// Memory id or short prefix
    pub id: String,
}

#[derive(serde::Serialize)]
struct LearnIngestItem {
    pub trajectory_id: String,
    pub task: String,
    pub reason: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_step_index: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_step_title: Option<String>,
    pub content: String,
    pub memory_type: String,
    pub scopes: Vec<String>,
    pub tags: Vec<String>,
    pub learning_tip: LearningTip,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub memory: Option<AddMemoryResponse>,
}

#[derive(serde::Serialize)]
struct LearnIngestResponse {
    pub trajectories_processed: usize,
    pub candidate_count: usize,
    pub stored_count: usize,
    pub dry_run: bool,
    pub trajectories_without_candidates: Vec<String>,
    pub results: Vec<LearnIngestItem>,
}

#[derive(serde::Serialize)]
struct LearnConsolidateResult {
    pub cluster: LearningConsolidationCluster,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub applied: Option<LearningConsolidationApplied>,
}

#[derive(serde::Serialize)]
struct LearnConsolidateResponse {
    pub cluster_count: usize,
    pub member_count: usize,
    pub dry_run: bool,
    pub results: Vec<LearnConsolidateResult>,
}

pub async fn run(
    cmd: LearnCommands,
    db: &Arc<dyn Database>,
    config: &Config,
    json: bool,
) -> Result<()> {
    let pool = db.sqlite_pool().expect("SQLite backend required");
    match cmd {
        LearnCommands::Add(args) => run_add(args, pool, config, json).await,
        LearnCommands::Ingest(args) => run_ingest(args, pool, config, json).await,
        LearnCommands::Consolidate(args) => run_consolidate(args, pool, config, json).await,
        LearnCommands::Search(args) => run_search(args, pool, config, json).await,
        LearnCommands::Get(args) => run_get(args, pool, json).await,
    }
}

async fn run_add(args: LearnAddArgs, pool: &SqlitePool, config: &Config, json: bool) -> Result<()> {
    let category: LearningTipCategory = args.category.parse()?;
    let source_outcome: LearningSourceOutcome = args.source_outcome.parse()?;

    let memory_type = if let Some(memory_type) = args.memory_type {
        memory_type.parse::<MemoryType>()?
    } else {
        default_memory_type_for_learning(&category)
    };

    let importance = args.importance.unwrap_or(args.priority as i64);
    if !(1..=10).contains(&importance) {
        bail!(
            "Invalid importance value '{}'. Must be an integer between 1 and 10.",
            importance
        );
    }

    let learning_tip = LearningTip {
        version: LEARNING_TIP_VERSION,
        category: category.clone(),
        trigger: args.trigger,
        application_context: args.application_context,
        task_category: args.task_category,
        subtask: args.subtask,
        priority: args.priority,
        source_outcome,
        source_trajectory_ids: args.trajectory_ids,
        negative_example: args.negative_example,
        created_by: Some("voidm.learn.add".to_string()),
    };
    learning_tip.validate()?;

    let links = parse_links(&args.links)?;
    let request = build_learning_request(
        args.content,
        &learning_tip,
        memory_type,
        args.scope,
        parse_tags(args.tags),
        importance,
        links,
    )?;

    let response = crud::add_memory(pool, request, config).await?;
    let payload = serde_json::json!({
        "memory": response,
        "learning_tip": learning_tip,
    });

    if json {
        println!("{}", serde_json::to_string_pretty(&payload)?);
    } else {
        println!(
            "Added learning tip: {}",
            payload["memory"]["id"].as_str().unwrap_or_default()
        );
        println!(
            "Category: {}  Memory Type: {}  Priority: {}  Outcome: {}",
            category,
            payload["memory"]["type"].as_str().unwrap_or_default(),
            learning_tip.priority,
            learning_tip.source_outcome
        );
        println!("Task: {}", learning_tip.task_category);
        println!("Trigger: {}", learning_tip.trigger);
        println!("Context: {}", learning_tip.application_context);
        if let Some(subtask) = &learning_tip.subtask {
            println!("Subtask: {}", subtask);
        }
        println!(
            "Trajectories: {}",
            learning_tip.source_trajectory_ids.join(", ")
        );
        if let Some(quality_score) = payload["memory"]["quality_score"].as_f64() {
            println!("Quality: {:.2}", quality_score);
        }
        if let Some(negative_example) = &learning_tip.negative_example {
            println!("Avoid: {}", negative_example);
        }
    }

    Ok(())
}

async fn run_ingest(
    args: LearnIngestArgs,
    pool: &SqlitePool,
    config: &Config,
    json: bool,
) -> Result<()> {
    let raw = if args.stdin {
        use std::io::Read;
        let mut buf = String::new();
        std::io::stdin()
            .read_to_string(&mut buf)
            .context("Failed to read trajectory JSON from stdin")?;
        buf
    } else {
        let path = args
            .from
            .as_deref()
            .ok_or_else(|| anyhow::anyhow!("Provide either --from <file> or --stdin"))?;
        std::fs::read_to_string(path)
            .with_context(|| format!("Failed to read trajectory file '{}'", path))?
    };
    let trajectories = parse_learning_trajectories(&raw)?;

    let mut results = Vec::new();
    let mut trajectories_without_candidates = Vec::new();
    let mut stored_count = 0usize;

    for trajectory in trajectories.iter() {
        let candidates = extract_learning_candidates(trajectory)?;
        if candidates.is_empty() {
            trajectories_without_candidates.push(trajectory.trajectory_id.clone());
            continue;
        }

        for candidate in candidates {
            let scopes = if args.scope.is_empty() {
                candidate.scopes.clone()
            } else {
                dedupe_tags(args.scope.clone())
            };

            let memory = if args.write {
                let request = build_learning_request(
                    candidate.content.clone(),
                    &candidate.learning_tip,
                    candidate.memory_type.clone(),
                    scopes.clone(),
                    candidate.tags.clone(),
                    candidate.learning_tip.priority as i64,
                    vec![],
                )?;
                let stored = crud::add_memory(pool, request, config).await?;
                stored_count += 1;
                Some(stored)
            } else {
                None
            };

            results.push(LearnIngestItem {
                trajectory_id: candidate.trajectory_id,
                task: candidate.task,
                reason: candidate.reason,
                source_step_index: candidate.source_step_index,
                source_step_title: candidate.source_step_title,
                content: candidate.content,
                memory_type: candidate.memory_type.to_string(),
                scopes,
                tags: candidate.tags,
                learning_tip: candidate.learning_tip,
                memory,
            });
        }
    }

    let response = LearnIngestResponse {
        trajectories_processed: trajectories.len(),
        candidate_count: results.len(),
        stored_count,
        dry_run: !args.write,
        trajectories_without_candidates,
        results,
    };

    emit_ingest_response(&response, json)?;
    Ok(())
}

async fn run_consolidate(
    args: LearnConsolidateArgs,
    pool: &SqlitePool,
    config: &Config,
    json: bool,
) -> Result<()> {
    let request = LearningConsolidationRequest {
        scope_filter: args.scope,
        category: parse_optional_category(args.category)?,
        task_category: args.task_category,
        threshold: args.threshold,
        limit: args.limit,
    };

    let clusters = consolidate_learning_tips(pool, &request).await?;
    let member_count = clusters.iter().map(|cluster| cluster.members.len()).sum();
    let mut results = Vec::new();

    for cluster in clusters {
        let applied = if args.write {
            Some(apply_learning_consolidation(pool, &cluster, config).await?)
        } else {
            None
        };
        results.push(LearnConsolidateResult { cluster, applied });
    }

    let response = LearnConsolidateResponse {
        cluster_count: results.len(),
        member_count,
        dry_run: !args.write,
        results,
    };

    emit_consolidate_response(&response, json)?;
    Ok(())
}

async fn run_search(
    args: LearnSearchArgs,
    pool: &SqlitePool,
    config: &Config,
    json: bool,
) -> Result<()> {
    let mode: SearchMode = args.mode.parse()?;
    let filters = LearningSearchFilters {
        category: parse_optional_category(args.category)?,
        trigger: args.trigger,
        application_context: args.application_context,
        task_category: args.task_category,
        subtask: args.subtask,
        source_outcome: parse_optional_source_outcome(args.source_outcome)?,
        trajectory_id: args.trajectory_id,
        priority_min: args.priority_min,
    };

    let response = search_learning_tips(
        pool,
        &LearningSearchRequest {
            query: args.query,
            mode,
            limit: args.limit,
            scope_filter: args.scope,
            min_score: args.min_score,
            min_quality: args.min_quality,
            filters,
        },
        &config.embeddings.model,
        config.embeddings.enabled,
        config.search.min_score,
        &config.search,
    )
    .await?;

    emit_search_response(&response, json)?;
    Ok(())
}

async fn run_get(args: LearnGetArgs, pool: &SqlitePool, json: bool) -> Result<()> {
    let id = resolve_id(pool, &args.id).await?;
    let memory = crud::get_memory(pool, &id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("Memory '{}' not found", id))?;
    let record = memory_to_learning_record(memory)
        .ok_or_else(|| anyhow::anyhow!("Memory '{}' is not a structured learning tip", id))?;

    if json {
        println!("{}", serde_json::to_string_pretty(&record)?);
    } else {
        emit_learning_record(&record);
    }

    Ok(())
}

fn emit_search_response(response: &LearningSearchResponse, json: bool) -> Result<()> {
    if json {
        println!("{}", serde_json::to_string_pretty(response)?);
        return Ok(());
    }

    if response.results.is_empty() {
        println!("No learning tips found.");
        return Ok(());
    }

    for result in &response.results {
        println!(
            "[{:.2}] {} | {} | {} | {}",
            result.score,
            result.learning_tip.category,
            result.learning_tip.task_category,
            result.learning_tip.source_outcome,
            result.id
        );
        println!("Trigger: {}", result.learning_tip.trigger);
        println!("Tip: {}", result.content);
        println!("Context: {}", result.learning_tip.application_context);
        println!(
            "Priority: {}  Trajectories: {}",
            result.learning_tip.priority,
            result.learning_tip.source_trajectory_ids.join(", ")
        );
        if let Some(subtask) = &result.learning_tip.subtask {
            println!("Subtask: {}", subtask);
        }
        println!();
    }

    Ok(())
}

fn emit_ingest_response(response: &LearnIngestResponse, json: bool) -> Result<()> {
    if json {
        println!("{}", serde_json::to_string_pretty(response)?);
        return Ok(());
    }

    if response.results.is_empty() {
        println!(
            "Parsed {} trajectory record(s) but extracted no learning tips.",
            response.trajectories_processed
        );
        if !response.trajectories_without_candidates.is_empty() {
            println!(
                "No candidates: {}",
                response.trajectories_without_candidates.join(", ")
            );
        }
        return Ok(());
    }

    println!(
        "Parsed {} trajectory record(s); extracted {} learning tip(s).",
        response.trajectories_processed, response.candidate_count
    );
    if response.dry_run {
        println!("Preview only. Use --write to persist these candidates.");
    } else {
        println!("Stored {} learning tip(s).", response.stored_count);
    }
    println!();

    for item in &response.results {
        println!(
            "{} | {} | {} | {}",
            item.trajectory_id,
            item.learning_tip.category,
            item.learning_tip.task_category,
            item.reason
        );
        if let Some(step_index) = item.source_step_index {
            println!("Source Step: {}", step_index + 1);
        }
        if let Some(step_title) = &item.source_step_title {
            println!("Source Title: {}", step_title);
        }
        println!("Trigger: {}", item.learning_tip.trigger);
        println!("Tip: {}", item.content);
        println!("Context: {}", item.learning_tip.application_context);
        println!(
            "Priority: {}  Outcome: {}",
            item.learning_tip.priority, item.learning_tip.source_outcome
        );
        if !item.scopes.is_empty() {
            println!("Scopes: {}", item.scopes.join(", "));
        }
        if let Some(memory) = &item.memory {
            println!("Stored As: {}", memory.id);
            if let Some(quality_score) = memory.quality_score {
                println!("Quality: {:.2}", quality_score);
            }
        }
        println!();
    }

    Ok(())
}

fn emit_consolidate_response(response: &LearnConsolidateResponse, json: bool) -> Result<()> {
    if json {
        println!("{}", serde_json::to_string_pretty(response)?);
        return Ok(());
    }

    if response.results.is_empty() {
        println!("No overlapping learning tips met the consolidation threshold.");
        return Ok(());
    }

    println!(
        "Found {} consolidation cluster(s) covering {} learning tip(s).",
        response.cluster_count, response.member_count
    );
    if response.dry_run {
        println!("Preview only. Use --write to create canonical records.");
    } else {
        println!("Canonical records created and clustered members invalidated.");
    }
    println!();

    for (index, result) in response.results.iter().enumerate() {
        let cluster = &result.cluster;
        println!(
            "Cluster {} | {:.2} similarity | {} members | {}",
            index + 1,
            cluster.similarity_score,
            cluster.members.len(),
            cluster.canonical_learning_tip.category
        );
        println!(
            "Canonical Trigger: {}",
            cluster.canonical_learning_tip.trigger
        );
        println!("Canonical Tip: {}", cluster.canonical_content);
        println!(
            "Task: {}  Outcome: {}  Priority: {}",
            cluster.canonical_learning_tip.task_category,
            cluster.canonical_learning_tip.source_outcome,
            cluster.canonical_learning_tip.priority
        );
        println!("Members: {}", cluster.member_ids.join(", "));
        if let Some(applied) = &result.applied {
            println!("Stored As: {}", applied.canonical_memory.id);
        }
        println!();
    }

    Ok(())
}

fn emit_learning_record(record: &LearningMemoryRecord) {
    println!("ID: {}", record.memory.id);
    println!(
        "Category: {}  Memory Type: {}  Priority: {}  Outcome: {}",
        record.learning_tip.category,
        record.memory.memory_type,
        record.learning_tip.priority,
        record.learning_tip.source_outcome
    );
    println!("Task: {}", record.learning_tip.task_category);
    println!("Trigger: {}", record.learning_tip.trigger);
    println!("Context: {}", record.learning_tip.application_context);
    if let Some(subtask) = &record.learning_tip.subtask {
        println!("Subtask: {}", subtask);
    }
    if let Some(quality_score) = record.memory.quality_score {
        println!("Quality: {:.2}", quality_score);
    }
    if !record.memory.scopes.is_empty() {
        println!("Scopes: {}", record.memory.scopes.join(", "));
    }
    println!(
        "Trajectories: {}",
        record.learning_tip.source_trajectory_ids.join(", ")
    );
    if let Some(negative_example) = &record.learning_tip.negative_example {
        println!("Avoid: {}", negative_example);
    }
    println!();
    println!("{}", record.memory.content);
}

fn build_learning_request(
    content: String,
    learning_tip: &LearningTip,
    memory_type: MemoryType,
    scopes: Vec<String>,
    tags: Vec<String>,
    importance: i64,
    links: Vec<LinkSpec>,
) -> Result<AddMemoryRequest> {
    if !(1..=10).contains(&importance) {
        bail!(
            "Invalid importance value '{}'. Must be an integer between 1 and 10.",
            importance
        );
    }

    let mut tags = tags;
    tags.push("learning-tip".to_string());
    tags.push(learning_tip.category.to_string());
    tags = dedupe_tags(tags);

    let metadata =
        voidm_core::learning::attach_learning_tip_metadata(serde_json::json!({}), learning_tip)?;

    Ok(AddMemoryRequest {
        id: None,
        content,
        memory_type,
        scopes,
        tags,
        importance,
        metadata,
        links,
        title: None,
        context: None,
    })
}

fn parse_tags(tags: Option<String>) -> Vec<String> {
    tags.unwrap_or_default()
        .split(',')
        .map(|tag| tag.trim().to_string())
        .filter(|tag| !tag.is_empty())
        .collect()
}

fn dedupe_tags(tags: Vec<String>) -> Vec<String> {
    let mut seen = std::collections::HashSet::new();
    let mut deduped = Vec::new();
    for tag in tags {
        let normalized = tag.to_lowercase();
        if seen.insert(normalized) {
            deduped.push(tag);
        }
    }
    deduped
}

fn parse_links(links: &[String]) -> Result<Vec<LinkSpec>> {
    let mut parsed = Vec::new();
    for link in links {
        parsed.push(parse_link_spec(link)?);
    }
    Ok(parsed)
}

fn parse_link_spec(value: &str) -> Result<LinkSpec> {
    let parts: Vec<&str> = value.splitn(3, ':').collect();
    if parts.len() < 2 {
        bail!(
            "Invalid --link format: '{}'. Expected <id>:<EDGE_TYPE> or <id>:<EDGE_TYPE>:<note>",
            value
        );
    }

    let target_id = parts[0].to_string();
    let edge_type: EdgeType = parts[1].parse()?;
    let note = if parts.len() >= 3 && !parts[2].is_empty() {
        Some(parts[2].to_string())
    } else {
        None
    };

    if edge_type.requires_note() && note.is_none() {
        bail!("RELATES_TO requires a note explaining why no stronger relationship applies.");
    }

    Ok(LinkSpec {
        target_id,
        edge_type,
        note,
    })
}

fn parse_optional_category(value: Option<String>) -> Result<Option<LearningTipCategory>> {
    value
        .map(|category| category.parse::<LearningTipCategory>())
        .transpose()
}

fn parse_optional_source_outcome(value: Option<String>) -> Result<Option<LearningSourceOutcome>> {
    value
        .map(|outcome| outcome.parse::<LearningSourceOutcome>())
        .transpose()
}
