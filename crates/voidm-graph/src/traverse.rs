use anyhow::Result;
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use std::collections::{HashMap, HashSet, VecDeque};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NeighborResult {
    pub memory_id: String,
    pub rel_type: String,
    pub direction: String, // "outgoing" | "incoming" | "undirected"
    pub note: Option<String>,
    pub depth: u8,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PathStep {
    pub memory_id: String,
    pub rel_type: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphStats {
    pub node_count: i64,
    pub edge_count: i64,
    pub rel_type_counts: HashMap<String, i64>,
}

/// Get N-hop neighbors of a memory. RELATES_TO is always queried bidirectionally.
pub async fn neighbors(
    pool: &SqlitePool,
    memory_id: &str,
    depth: u8,
    rel_filter: Option<&str>,
) -> Result<Vec<NeighborResult>> {
    let mut visited: HashSet<String> = HashSet::new();
    visited.insert(memory_id.to_string());

    let mut results = Vec::new();
    let mut frontier: Vec<(String, u8)> = vec![(memory_id.to_string(), 0)];

    while let Some((current_id, current_depth)) = frontier.pop() {
        if current_depth >= depth {
            continue;
        }

        let current_node: Option<i64> =
            sqlx::query_scalar("SELECT id FROM graph_nodes WHERE memory_id = ?")
                .bind(&current_id)
                .fetch_optional(pool)
                .await?;

        let node_id = match current_node {
            Some(n) => n,
            None => continue,
        };

        // Outgoing edges
        let outgoing: Vec<(String, String, Option<String>)> = sqlx::query_as(
            "SELECT n.memory_id, e.rel_type, e.note
             FROM graph_edges e
             JOIN graph_nodes n ON n.id = e.target_id
             WHERE e.source_id = ?",
        )
        .bind(node_id)
        .fetch_all(pool)
        .await?;

        for (neighbor_id, rel_type, note) in outgoing {
            if let Some(filter) = rel_filter {
                if rel_type != filter {
                    continue;
                }
            }
            if !visited.contains(&neighbor_id) {
                visited.insert(neighbor_id.clone());
                results.push(NeighborResult {
                    memory_id: neighbor_id.clone(),
                    rel_type: rel_type.clone(),
                    direction: "outgoing".into(),
                    note,
                    depth: current_depth + 1,
                });
                frontier.push((neighbor_id, current_depth + 1));
            }
        }

        // Incoming edges (all types + always include RELATES_TO reverse)
        let incoming: Vec<(String, String, Option<String>)> = sqlx::query_as(
            "SELECT n.memory_id, e.rel_type, e.note
             FROM graph_edges e
             JOIN graph_nodes n ON n.id = e.source_id
             WHERE e.target_id = ?",
        )
        .bind(node_id)
        .fetch_all(pool)
        .await?;

        for (neighbor_id, rel_type, note) in incoming {
            // For directed edges, only traverse incoming if specifically RELATES_TO
            let is_undirected = rel_type == "RELATES_TO";
            // For non-RELATES_TO, incoming means something else links TO us — include it
            if let Some(filter) = rel_filter {
                if rel_type != filter {
                    continue;
                }
            }
            if !visited.contains(&neighbor_id) {
                visited.insert(neighbor_id.clone());
                let direction = if is_undirected {
                    "undirected".into()
                } else {
                    "incoming".into()
                };
                results.push(NeighborResult {
                    memory_id: neighbor_id.clone(),
                    rel_type,
                    direction,
                    note,
                    depth: current_depth + 1,
                });
                frontier.push((neighbor_id, current_depth + 1));
            }
        }
    }

    Ok(results)
}

/// BFS shortest path between two memories. Max 10 hops.
pub async fn shortest_path(
    pool: &SqlitePool,
    from_id: &str,
    to_id: &str,
) -> Result<Option<Vec<PathStep>>> {
    const MAX_DEPTH: u8 = 10;

    let mut visited: HashSet<String> = HashSet::new();
    let mut queue: VecDeque<Vec<PathStep>> = VecDeque::new();

    queue.push_back(vec![PathStep {
        memory_id: from_id.to_string(),
        rel_type: None,
    }]);
    visited.insert(from_id.to_string());

    while let Some(path) = queue.pop_front() {
        let current = &path.last().unwrap().memory_id;
        if current == to_id {
            return Ok(Some(path));
        }
        if path.len() as u8 > MAX_DEPTH {
            continue;
        }

        let node_id: Option<i64> =
            sqlx::query_scalar("SELECT id FROM graph_nodes WHERE memory_id = ?")
                .bind(current)
                .fetch_optional(pool)
                .await?;

        if let Some(nid) = node_id {
            // Both directions
            let edges: Vec<(String, String)> = sqlx::query_as(
                "SELECT n.memory_id, e.rel_type FROM graph_edges e
                 JOIN graph_nodes n ON n.id = e.target_id WHERE e.source_id = ?
                 UNION
                 SELECT n.memory_id, e.rel_type FROM graph_edges e
                 JOIN graph_nodes n ON n.id = e.source_id WHERE e.target_id = ?",
            )
            .bind(nid)
            .bind(nid)
            .fetch_all(pool)
            .await?;

            for (neighbor_id, rel_type) in edges {
                if !visited.contains(&neighbor_id) {
                    visited.insert(neighbor_id.clone());
                    let mut new_path = path.clone();
                    new_path.push(PathStep {
                        memory_id: neighbor_id,
                        rel_type: Some(rel_type),
                    });
                    queue.push_back(new_path);
                }
            }
        }
    }

    Ok(None) // No path found
}

/// Compute PageRank for all graph nodes. Returns (memory_id, score) sorted descending.
/// Includes ontology concept nodes (prefixed "concept::<id>") in the same graph so
/// well-connected concepts rank alongside well-connected memories.
pub async fn pagerank(
    pool: &SqlitePool,
    damping: f64,
    iterations: u32,
) -> Result<Vec<(String, f64)>> {
    // ── Memory nodes + graph_edges ────────────────────────────────────────────
    let mem_edges: Vec<(i64, i64)> = sqlx::query_as("SELECT source_id, target_id FROM graph_edges")
        .fetch_all(pool)
        .await?;

    let mem_nodes: Vec<(i64, String)> = sqlx::query_as("SELECT id, memory_id FROM graph_nodes")
        .fetch_all(pool)
        .await?;

    // ── Concept nodes + ontology_edges ────────────────────────────────────────
    let concept_nodes: Vec<(String,)> = sqlx::query_as("SELECT id FROM ontology_concepts")
        .fetch_all(pool)
        .await?;

    let ont_edges: Vec<(String, String)> =
        sqlx::query_as("SELECT from_id, to_id FROM ontology_edges")
            .fetch_all(pool)
            .await?;

    // ── Build unified node index ───────────────────────────────────────────────
    // Memory nodes use integer graph_nodes.id as key.
    // Concept nodes use a string key "c::<concept_id>".
    let mut labels: Vec<String> = Vec::new(); // index → display label
    let mut mem_graph_id_to_idx: HashMap<i64, usize> = HashMap::new();
    let mut concept_id_to_idx: HashMap<String, usize> = HashMap::new();

    for (gid, mid) in &mem_nodes {
        let idx = labels.len();
        mem_graph_id_to_idx.insert(*gid, idx);
        labels.push(mid.clone());
    }
    for (cid,) in &concept_nodes {
        let idx = labels.len();
        concept_id_to_idx.insert(cid.clone(), idx);
        labels.push(format!("concept::{}", cid));
    }

    let n = labels.len();
    if n == 0 {
        return Ok(vec![]);
    }

    let mut out_neighbors: Vec<Vec<usize>> = vec![vec![]; n];
    let mut in_neighbors: Vec<Vec<usize>> = vec![vec![]; n];

    // Memory ↔ memory edges
    for (src, tgt) in &mem_edges {
        if let (Some(&si), Some(&ti)) = (mem_graph_id_to_idx.get(src), mem_graph_id_to_idx.get(tgt))
        {
            out_neighbors[si].push(ti);
            in_neighbors[ti].push(si);
        }
    }

    // Ontology edges (concept ↔ concept, concept ↔ memory)
    for (from_id, to_id) in &ont_edges {
        // from_id could be a concept id or a memory UUID
        let from_idx = concept_id_to_idx
            .get(from_id.as_str())
            .copied()
            .or_else(|| {
                // It's a memory UUID — find its graph_nodes.id
                mem_nodes
                    .iter()
                    .find(|(_, mid)| mid == from_id)
                    .and_then(|(gid, _)| mem_graph_id_to_idx.get(gid).copied())
            });
        let to_idx = concept_id_to_idx.get(to_id.as_str()).copied().or_else(|| {
            mem_nodes
                .iter()
                .find(|(_, mid)| mid == to_id)
                .and_then(|(gid, _)| mem_graph_id_to_idx.get(gid).copied())
        });

        if let (Some(si), Some(ti)) = (from_idx, to_idx) {
            out_neighbors[si].push(ti);
            in_neighbors[ti].push(si);
        }
    }

    // ── Power iteration ───────────────────────────────────────────────────────
    let mut scores = vec![1.0f64 / n as f64; n];
    for _ in 0..iterations {
        let mut new_scores = vec![(1.0 - damping) / n as f64; n];
        for i in 0..n {
            for &j in &in_neighbors[i] {
                let out_deg = out_neighbors[j].len();
                if out_deg > 0 {
                    new_scores[i] += damping * scores[j] / out_deg as f64;
                }
            }
        }
        scores = new_scores;
    }

    let mut results: Vec<(String, f64)> = labels.into_iter().zip(scores).collect();
    results.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    Ok(results)
}

/// Graph statistics.
pub async fn graph_stats(pool: &SqlitePool) -> Result<GraphStats> {
    let node_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM graph_nodes")
        .fetch_one(pool)
        .await?;
    let edge_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM graph_edges")
        .fetch_one(pool)
        .await?;

    let rel_rows: Vec<(String, i64)> = sqlx::query_as(
        "SELECT rel_type, COUNT(*) as cnt FROM graph_edges GROUP BY rel_type ORDER BY cnt DESC",
    )
    .fetch_all(pool)
    .await?;

    Ok(GraphStats {
        node_count,
        edge_count,
        rel_type_counts: rel_rows.into_iter().collect(),
    })
}
