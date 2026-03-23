use anyhow::Result;
use clap::{Args, Subcommand};
use sqlx::SqlitePool;
use voidm_core::resolve_id;
use voidm_graph;

#[derive(Subcommand)]
pub enum GraphCommands {
    /// Execute a read-only Cypher query
    Cypher(CypherArgs),
    /// Get N-hop neighbors of a memory
    Neighbors(NeighborsArgs),
    /// Find shortest path between two memories
    Path(PathArgs),
    /// Compute PageRank for all memories
    Pagerank(PagerankArgs),
    /// Show graph statistics
    Stats,
    /// Export graph to Graphviz DOT format
    Export(ExportArgs),
}

#[derive(Args)]
pub struct CypherArgs {
    /// Read-only Cypher query (MATCH/WHERE/RETURN/LIMIT). Write clauses are rejected.
    pub query: String,
}

#[derive(Args)]
pub struct NeighborsArgs {
    /// Memory ID to start from
    pub id: String,
    /// Traversal depth (default: 1)
    #[arg(long, default_value = "1")]
    pub depth: u8,
    /// Filter by relationship type: SUPPORTS, CONTRADICTS, DERIVED_FROM, PRECEDES, PART_OF, EXEMPLIFIES, INVALIDATES, RELATES_TO
    #[arg(long)]
    pub rel: Option<String>,
}

#[derive(Args)]
pub struct PathArgs {
    /// Source memory ID
    pub from: String,
    /// Target memory ID
    pub to: String,
}

#[derive(Args)]
pub struct PagerankArgs {
    /// Number of top results to return
    #[arg(long, default_value = "10")]
    pub top: usize,
    #[arg(long, default_value = "0.85")]
    pub damping: f64,
    #[arg(long, default_value = "20")]
    pub iterations: u32,
}

#[derive(Args)]
pub struct ExportArgs {
    /// Export format: dot (Graphviz), json, csv, html (interactive viewer)
    #[arg(long, default_value = "dot")]
    pub format: String,
    /// Filter: include only memories (m), concepts (c), or both (mc)
    #[arg(long)]
    pub nodes: Option<String>,
    /// Minimum edge count to include node (only nodes with >= edges shown)
    #[arg(long, default_value = "0")]
    pub min_edges: usize,
}

pub async fn run(cmd: GraphCommands, pool: &SqlitePool, json: bool) -> Result<()> {
    match cmd {
        GraphCommands::Cypher(args) => run_cypher(args, pool, json).await,
        GraphCommands::Neighbors(args) => run_neighbors(args, pool, json).await,
        GraphCommands::Path(args) => run_path(args, pool, json).await,
        GraphCommands::Pagerank(args) => run_pagerank(args, pool, json).await,
        GraphCommands::Stats => run_stats(pool, json).await,
        GraphCommands::Export(args) => run_export(args, pool, json).await,
    }
}

async fn run_cypher(args: CypherArgs, pool: &SqlitePool, json: bool) -> Result<()> {
    let rows = voidm_graph::cypher_read(pool, &args.query).await?;
    if json {
        println!("{}", serde_json::to_string_pretty(&rows)?);
    } else {
        for row in &rows {
            let line: Vec<String> = row.iter().map(|(k, v)| format!("{}: {}", k, v)).collect();
            println!("{}", line.join("  |  "));
        }
        println!("{} row(s)", rows.len());
    }
    Ok(())
}

async fn run_neighbors(args: NeighborsArgs, pool: &SqlitePool, json: bool) -> Result<()> {
    let id = match resolve_id(pool, &args.id).await {
        Ok(id) => id,
        Err(e) => {
            if json {
                println!(
                    "{}",
                    serde_json::json!({ "error": e.to_string(), "id": args.id })
                );
            } else {
                eprintln!("Error: {}", e);
            }
            std::process::exit(1);
        }
    };
    let results = voidm_graph::neighbors(pool, &id, args.depth, args.rel.as_deref()).await?;
    if json {
        println!("{}", serde_json::to_string_pretty(&results)?);
    } else {
        if results.is_empty() {
            println!("No neighbors found for '{}' at depth {}.", id, args.depth);
            println!(
                "Hint: Use 'voidm link {} <EDGE_TYPE> <target-id>' to create edges.",
                id
            );
        } else {
            for n in &results {
                println!(
                    "[depth {}] {} --[{}]--> {} ({})",
                    n.depth, id, n.rel_type, n.memory_id, n.direction
                );
            }
            println!("{} neighbor(s)", results.len());
        }
    }
    Ok(())
}

async fn run_path(args: PathArgs, pool: &SqlitePool, json: bool) -> Result<()> {
    // Resolve both IDs before same-ID check (so short IDs expand correctly)
    let from = resolve_id(pool, &args.from).await?;
    let to = resolve_id(pool, &args.to).await?;

    if from == to {
        if json {
            println!(
                "{}",
                serde_json::json!({
                    "error": "Source and target IDs are the same. A path requires two distinct memories.",
                    "from": from, "to": to
                })
            );
        } else {
            eprintln!("Error: Source and target IDs are the same ('{}').\nA path requires two distinct memory IDs.", from);
        }
        std::process::exit(2);
    }

    match voidm_graph::shortest_path(pool, &from, &to).await? {
        None => {
            if json {
                println!(
                    "{}",
                    serde_json::json!({
                        "path": null,
                        "message": format!("No path found between '{}' and '{}'", from, to),
                        "hint": "Memories may not be connected. Use 'voidm link' to create edges."
                    })
                );
            } else {
                println!("No path found between '{}' and '{}'.", from, to);
                println!(
                    "Hint: Use 'voidm link {} <EDGE_TYPE> {}' to connect them.",
                    from, to
                );
            }
        }
        Some(path) => {
            if json {
                println!("{}", serde_json::to_string_pretty(&path)?);
            } else {
                let parts: Vec<String> = path
                    .iter()
                    .map(|s| {
                        if let Some(ref r) = s.rel_type {
                            format!("{} -[{}]->", s.memory_id, r)
                        } else {
                            s.memory_id.clone()
                        }
                    })
                    .collect();
                println!("{}", parts.join(" "));
            }
        }
    }
    Ok(())
}

async fn run_pagerank(args: PagerankArgs, pool: &SqlitePool, json: bool) -> Result<()> {
    let mut results = voidm_graph::pagerank(pool, args.damping, args.iterations).await?;
    results.truncate(args.top);
    if json {
        let v: Vec<_> = results
            .iter()
            .map(|(id, score)| serde_json::json!({"id": id, "score": score}))
            .collect();
        println!("{}", serde_json::to_string_pretty(&v)?);
    } else {
        if results.is_empty() {
            println!(
                "No memories in graph yet. Use 'voidm add' and 'voidm link' to build the graph."
            );
        } else {
            for (i, (id, score)) in results.iter().enumerate() {
                println!("#{} [{:.4}] {}", i + 1, score, id);
            }
        }
    }
    Ok(())
}

async fn run_stats(pool: &SqlitePool, json: bool) -> Result<()> {
    let stats = voidm_graph::graph_stats(pool).await?;
    if json {
        println!("{}", serde_json::to_string_pretty(&stats)?);
    } else {
        println!("Nodes: {}", stats.node_count);
        println!("Edges: {}", stats.edge_count);
        if !stats.rel_type_counts.is_empty() {
            println!("Edge types:");
            let mut counts: Vec<_> = stats.rel_type_counts.iter().collect();
            counts.sort_by(|a, b| b.1.cmp(a.1));
            for (rel, cnt) in counts {
                println!("  {:20} {}", rel, cnt);
            }
        } else {
            println!("No edges yet. Use 'voidm link <id> <EDGE_TYPE> <id>' to create edges.");
        }
    }
    Ok(())
}

async fn run_export(args: ExportArgs, pool: &SqlitePool, _json: bool) -> Result<()> {
    match args.format.as_str() {
        "dot" => export_dot(args, pool).await,
        "json" => export_json(args, pool).await,
        "csv" => export_csv(args, pool).await,
        fmt => anyhow::bail!("Unknown format: {}. Supported: dot, json, csv", fmt),
    }
}

async fn export_dot(_args: ExportArgs, pool: &SqlitePool) -> Result<()> {
    // Get all memories
    let memories: Vec<(String, String, String)> = sqlx::query_as(
        "SELECT id, type, SUBSTR(content, 1, 50) as preview FROM memories LIMIT 1000",
    )
    .fetch_all(pool)
    .await?;

    // Get all concepts
    let concepts: Vec<(String, String)> =
        sqlx::query_as("SELECT id, name FROM ontology_concepts LIMIT 500")
            .fetch_all(pool)
            .await?;

    // Get all edges
    let edges: Vec<(String, String, String)> =
        sqlx::query_as("SELECT from_id, to_id, rel_type FROM ontology_edges LIMIT 2000")
            .fetch_all(pool)
            .await?;

    // Start DOT file
    println!("digraph voidm {{");
    println!("  rankdir=LR;");
    println!("  node [shape=box, style=rounded];");

    // Add memory nodes
    for (id, mem_type, preview) in &memories {
        let color = match mem_type.as_str() {
            "semantic" => "lightblue",
            "episodic" => "lightgreen",
            "procedural" => "lightyellow",
            "conceptual" => "lightcyan",
            "contextual" => "lightgray",
            _ => "white",
        };
        let label = preview.replace("\"", "\\\"");
        println!(
            "  \"m:{}\" [label=\"{}\", fillcolor=\"{}\", style=\"rounded,filled\"];",
            &id[..8],
            label,
            color
        );
    }

    // Add concept nodes
    for (id, name) in &concepts {
        let label = name.replace("\"", "\\\"");
        println!("  \"c:{}\" [label=\"{} (concept)\", fillcolor=\"lavender\", style=\"rounded,filled\"];", 
                 &id[..8], label);
    }

    // Add edges
    for (from, to, rel) in &edges {
        let from_node = if from.starts_with("m:") {
            from.clone()
        } else {
            format!("m:{}", &from[..8])
        };
        let to_node = if to.starts_with("c:") {
            to.clone()
        } else {
            format!("c:{}", &to[..8])
        };
        println!(
            "  \"{}\" -> \"{}\" [label=\"{}\"];",
            from_node, to_node, rel
        );
    }

    println!("}}");
    Ok(())
}

async fn export_json(_args: ExportArgs, pool: &SqlitePool) -> Result<()> {
    use serde_json::json;

    let memories: Vec<(String, String)> =
        sqlx::query_as("SELECT id, type FROM memories LIMIT 1000")
            .fetch_all(pool)
            .await?;

    let concepts: Vec<(String, String)> =
        sqlx::query_as("SELECT id, name FROM ontology_concepts LIMIT 500")
            .fetch_all(pool)
            .await?;

    let edges: Vec<(String, String, String)> =
        sqlx::query_as("SELECT from_id, to_id, rel_type FROM ontology_edges LIMIT 2000")
            .fetch_all(pool)
            .await?;

    let result = json!({
        "memories": memories.iter().map(|(id, t)| json!({"id": id, "type": t})).collect::<Vec<_>>(),
        "concepts": concepts.iter().map(|(id, name)| json!({"id": id, "name": name})).collect::<Vec<_>>(),
        "edges": edges.iter().map(|(f, t, r)| json!({"from": f, "to": t, "type": r})).collect::<Vec<_>>(),
    });

    println!("{}", serde_json::to_string_pretty(&result)?);
    Ok(())
}

async fn export_csv(_args: ExportArgs, pool: &SqlitePool) -> Result<()> {
    let edges: Vec<(String, String, String)> =
        sqlx::query_as("SELECT from_id, to_id, rel_type FROM ontology_edges LIMIT 2000")
            .fetch_all(pool)
            .await?;

    println!("from_id,to_id,relationship_type");
    for (from, to, rel) in edges {
        println!("{},{},{}", from, to, rel);
    }
    Ok(())
}
