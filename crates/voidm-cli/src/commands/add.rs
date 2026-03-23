use anyhow::Result;
use clap::Args;
use sqlx::SqlitePool;
use voidm_core::{
    crud,
    models::{AddMemoryRequest, EdgeType, LinkSpec, MemoryType},
    Config,
};

#[derive(Args)]
pub struct AddArgs {
    /// Memory content
    pub content: String,

    /// Memory type: episodic, semantic, procedural, conceptual, contextual
    #[arg(long, short = 't')]
    pub r#type: String,

    /// Scopes (may be repeated): e.g. --scope work/acme/backend
    #[arg(long, short = 's')]
    pub scope: Vec<String>,

    /// Tags (comma-separated): e.g. --tags rust,performance
    #[arg(long)]
    pub tags: Option<String>,

    /// Importance 1–10 (default: 5)
    #[arg(long, default_value = "5")]
    pub importance: i64,

    /// Link to existing memory: <id>:<EDGE_TYPE> or <id>:<EDGE_TYPE>:<note>
    /// RELATES_TO requires a note: <id>:RELATES_TO:<reason>
    #[arg(long = "link")]
    pub links: Vec<String>,
}

pub async fn run(args: AddArgs, pool: &SqlitePool, config: &Config, json: bool) -> Result<()> {
    let memory_type: MemoryType = args.r#type.parse()?;

    // Validate importance before touching the DB
    if !(1..=10).contains(&args.importance) {
        anyhow::bail!(
            "Invalid importance value '{}'. Must be an integer between 1 and 10.",
            args.importance
        );
    }

    let tags: Vec<String> = args
        .tags
        .unwrap_or_default()
        .split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();

    let mut link_specs = Vec::new();
    for link_str in &args.links {
        let spec = parse_link_spec(link_str)?;
        link_specs.push(spec);
    }

    let req = AddMemoryRequest {
        id: None,
        content: args.content,
        memory_type,
        scopes: args.scope,
        tags,
        importance: args.importance,
        metadata: serde_json::Value::Object(Default::default()),
        links: link_specs,
    };

    let resp = crud::add_memory(pool, req, config).await?;

    if json {
        println!("{}", serde_json::to_string_pretty(&resp)?);
    } else {
        println!("Added memory: {}", resp.id);
        println!(
            "Type: {}  Importance: {}",
            resp.memory_type, resp.importance
        );
        if let Some(qs) = resp.quality_score {
            println!("Quality: {:.2}", qs);
        }
        if !resp.scopes.is_empty() {
            println!("Scopes: {}", resp.scopes.join(", "));
        }
        if !resp.suggested_links.is_empty() {
            eprintln!("\nSuggested links:");
            for s in &resp.suggested_links {
                eprintln!(
                    "  [{:.2}] {} ({}): {}",
                    s.score, s.id, s.memory_type, s.hint
                );
            }
        }
        if let Some(ref dup) = resp.duplicate_warning {
            eprintln!("\nWarning: {}", dup.message);
            eprintln!("  Existing: {} [{:.2}]", dup.id, dup.score);
        }
    }

    Ok(())
}

fn parse_link_spec(s: &str) -> Result<LinkSpec> {
    // Format: <id>:<EDGE_TYPE> or <id>:<EDGE_TYPE>:<note>
    let parts: Vec<&str> = s.splitn(3, ':').collect();
    if parts.len() < 2 {
        anyhow::bail!(
            "Invalid --link format: '{}'\n\
             Expected: --link <id>:<EDGE_TYPE> or --link <id>:<EDGE_TYPE>:<note>\n\
             Valid edge types: RELATES_TO, SUPPORTS, CONTRADICTS, DERIVED_FROM, PRECEDES, PART_OF, EXEMPLIFIES, INVALIDATES\n\
             Example: --link abc123:SUPPORTS",
            s
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
        anyhow::bail!(
            "RELATES_TO requires a note explaining why no stronger relationship applies.\n\
             Use: --link {}:RELATES_TO:<your reason>\n\
             Example: --link {}:RELATES_TO:\"both concern API design\"\n\
             Consider a stronger type if applicable: SUPPORTS, CONTRADICTS, DERIVED_FROM, PRECEDES, PART_OF, EXEMPLIFIES, INVALIDATES",
            target_id, target_id
        );
    }

    Ok(LinkSpec {
        target_id,
        edge_type,
        note,
    })
}
