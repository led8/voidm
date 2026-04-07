use anyhow::{Context, Result};
use neo4rs::Graph;
use std::future::Future;
use std::pin::Pin;

use crate::models::{AddMemoryRequest, AddMemoryResponse, EdgeType, LinkResponse, Memory};
use crate::ontology::{
    Concept, ConceptSearchResult, ConceptWithInstances, ConceptWithSimilarityWarning, OntologyEdge,
};
use crate::search::{SearchOptions, SearchResponse};

/// Neo4j implementation of the Database trait.
/// Uses the neo4rs async driver with Bolt protocol.
#[derive(Clone)]
pub struct Neo4jDatabase {
    pub graph: Graph,
}

impl Neo4jDatabase {
    /// Connect to a Neo4j instance
    pub async fn connect(uri: &str, username: &str, password: &str) -> Result<Self> {
        let graph = Graph::new(uri, username, password)
            .await
            .with_context(|| format!("Failed to connect to Neo4j at {}", uri))?;

        // Initialize schema
        let db = Self { graph };
        db.init_schema().await?;

        Ok(db)
    }

    /// Initialize Neo4j schema with constraints and indices
    async fn init_schema(&self) -> Result<()> {
        // Create constraints for Memory nodes
        self.graph
            .run(neo4rs::query(
                "CREATE CONSTRAINT memory_id IF NOT EXISTS FOR (m:Memory) REQUIRE m.id IS UNIQUE",
            ))
            .await
            .ok(); // Ignore errors if constraint already exists

        // Create constraint for Concept nodes
        self.graph
            .run(neo4rs::query(
                "CREATE CONSTRAINT concept_id IF NOT EXISTS FOR (c:Concept) REQUIRE c.id IS UNIQUE",
            ))
            .await
            .ok();

        Ok(())
    }
}

// Trait implementation
impl crate::db::Database for Neo4jDatabase {
    fn health_check(&self) -> Pin<Box<dyn Future<Output = Result<()>> + Send + '_>> {
        let graph = self.graph.clone();
        Box::pin(async move {
            graph
                .run(neo4rs::query("RETURN 1 as ping"))
                .await
                .map(|_| ())
                .context("Neo4j health check failed")
        })
    }

    fn close(&self) -> Pin<Box<dyn Future<Output = Result<()>> + Send + '_>> {
        // neo4rs::Graph doesn't have an explicit close method
        // Connection is closed when graph is dropped
        Box::pin(async move { Ok(()) })
    }

    fn ensure_schema(&self) -> Pin<Box<dyn Future<Output = Result<()>> + Send + '_>> {
        let db = self.clone();
        Box::pin(async move { db.init_schema().await })
    }

    fn add_memory(
        &self,
        req: AddMemoryRequest,
        config: &crate::Config,
    ) -> Pin<Box<dyn Future<Output = Result<AddMemoryResponse>> + Send + '_>> {
        let graph = self.graph.clone();
        let config_model = config.embeddings.model.clone();

        Box::pin(async move {
            let id = req.id.unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
            let created_at = chrono::Utc::now().to_rfc3339();
            let memory_type = req.memory_type.to_string();

            let query = neo4rs::query(
                "CREATE (m:Memory {
                    id: $id,
                    type: $type,
                    content: $content,
                    importance: $importance,
                    tags: $tags,
                    metadata: $metadata,
                    scopes: $scopes,
                    created_at: $created_at,
                    updated_at: $created_at,
                    embedding_model: $model,
                    title: $title,
                    context: $context
                }) RETURN m",
            )
            .param("id", id.clone())
            .param("type", memory_type.clone())
            .param("content", req.content.clone())
            .param("importance", req.importance)
            .param("tags", req.tags.clone())
            .param("metadata", req.metadata.to_string())
            .param("scopes", req.scopes.clone())
            .param("created_at", created_at.clone())
            .param("model", config_model)
            .param("title", req.title.clone().unwrap_or_default())
            .param("context", req.context.clone().unwrap_or_default());

            graph
                .run(query)
                .await
                .context("Failed to create memory in Neo4j")?;

            Ok(AddMemoryResponse {
                id,
                memory_type,
                content: req.content,
                scopes: req.scopes,
                tags: req.tags,
                importance: req.importance,
                created_at,
                quality_score: None,
                suggested_links: vec![],
                duplicate_warning: None,
                title: req.title,
                context: req.context,
            })
        })
    }

    fn get_memory(
        &self,
        id: &str,
    ) -> Pin<Box<dyn Future<Output = Result<Option<Memory>>> + Send + '_>> {
        let graph = self.graph.clone();
        let id = id.to_string();

        Box::pin(async move {
            let mut result = graph
                .execute(neo4rs::query("MATCH (m:Memory {id: $id}) RETURN m").param("id", id))
                .await
                .context("Failed to get memory from Neo4j")?;

            if let Ok(Some(row)) = result.next().await {
                let node: neo4rs::Node = row.get("m").context("Failed to extract memory node")?;

                let memory = Memory {
                    id: node.get("id").context("Missing id")?,
                    content: node.get("content").context("Missing content")?,
                    memory_type: node.get::<String>("type").context("Missing type")?,
                    importance: node.get("importance").unwrap_or(0),
                    tags: node.get("tags").unwrap_or_default(),
                    metadata: serde_json::Value::Object(Default::default()),
                    scopes: node.get("scopes").unwrap_or_default(),
                    created_at: node.get("created_at").context("Missing created_at")?,
                    updated_at: node.get("updated_at").context("Missing updated_at")?,
                    quality_score: None,
                    title: node.get("title").ok().filter(|s: &String| !s.is_empty()),
                    context: node.get("context").ok().filter(|s: &String| !s.is_empty()),
                };

                Ok(Some(memory))
            } else {
                Ok(None)
            }
        })
    }

    fn list_memories(
        &self,
        limit: Option<usize>,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<Memory>>> + Send + '_>> {
        let graph = self.graph.clone();
        let limit = limit.unwrap_or(100);

        Box::pin(async move {
            let mut result = graph
                .execute(
                    neo4rs::query(
                        "MATCH (m:Memory) RETURN m ORDER BY m.created_at DESC LIMIT $limit",
                    )
                    .param("limit", limit as i64),
                )
                .await
                .context("Failed to list memories from Neo4j")?;

            let mut memories = Vec::new();
            while let Ok(Some(row)) = result.next().await {
                let node: neo4rs::Node = row.get("m").context("Failed to extract memory node")?;

                let memory = Memory {
                    id: node.get("id").context("Missing id")?,
                    content: node.get("content").context("Missing content")?,
                    memory_type: node.get::<String>("type").context("Missing type")?,
                    importance: node.get("importance").unwrap_or(0),
                    tags: node.get("tags").unwrap_or_default(),
                    metadata: serde_json::Value::Object(Default::default()),
                    scopes: node.get("scopes").unwrap_or_default(),
                    created_at: node.get("created_at").context("Missing created_at")?,
                    updated_at: node.get("updated_at").unwrap_or_default(),
                    quality_score: None,
                    title: node.get("title").ok().filter(|s: &String| !s.is_empty()),
                    context: node.get("context").ok().filter(|s: &String| !s.is_empty()),
                };

                memories.push(memory);
            }

            Ok(memories)
        })
    }

    fn delete_memory(&self, id: &str) -> Pin<Box<dyn Future<Output = Result<bool>> + Send + '_>> {
        let graph = self.graph.clone();
        let id = id.to_string();

        Box::pin(async move {
            let mut result = graph
                .execute(
                    neo4rs::query("MATCH (m:Memory {id: $id}) DELETE m RETURN count(m) as deleted")
                        .param("id", id),
                )
                .await
                .context("Failed to delete memory from Neo4j")?;

            if let Ok(Some(row)) = result.next().await {
                let deleted: i64 = row.get("deleted").unwrap_or(0);
                Ok(deleted > 0)
            } else {
                Ok(false)
            }
        })
    }

    fn update_memory(
        &self,
        id: &str,
        content: &str,
    ) -> Pin<Box<dyn Future<Output = Result<()>> + Send + '_>> {
        let graph = self.graph.clone();
        let id = id.to_string();
        let content = content.to_string();

        Box::pin(async move {
            let updated_at = chrono::Utc::now().to_rfc3339();
            graph
                .run(
                    neo4rs::query("MATCH (m:Memory {id: $id}) SET m.content = $content, m.updated_at = $updated_at")
                        .param("id", id)
                        .param("content", content)
                        .param("updated_at", updated_at),
                )
                .await
                .context("Failed to update memory in Neo4j")?;

            Ok(())
        })
    }

    fn resolve_memory_id(
        &self,
        id: &str,
    ) -> Pin<Box<dyn Future<Output = Result<String>> + Send + '_>> {
        let id = id.to_string();
        Box::pin(async move { Ok(id) })
    }

    fn list_scopes(&self) -> Pin<Box<dyn Future<Output = Result<Vec<String>>> + Send + '_>> {
        let graph = self.graph.clone();

        Box::pin(async move {
            let mut result = graph
                .execute(neo4rs::query("MATCH (m:Memory) WHERE m.scopes IS NOT NULL UNWIND m.scopes as scope RETURN DISTINCT scope"))
                .await
                .context("Failed to list scopes from Neo4j")?;

            let mut scopes = Vec::new();
            while let Ok(Some(row)) = result.next().await {
                if let Ok(scope) = row.get::<String>("scope") {
                    scopes.push(scope);
                }
            }

            Ok(scopes)
        })
    }

    fn link_memories(
        &self,
        from_id: &str,
        rel: &EdgeType,
        to_id: &str,
        note: Option<&str>,
    ) -> Pin<Box<dyn Future<Output = Result<LinkResponse>> + Send + '_>> {
        let graph = self.graph.clone();
        let from_id = from_id.to_string();
        let to_id = to_id.to_string();
        let rel_type = format!("{:?}", rel);
        let note = note.map(|s| s.to_string());

        Box::pin(async move {
            let query = if let Some(note_text) = &note {
                neo4rs::query(
                    "MATCH (from:Memory {id: $from_id}), (to:Memory {id: $to_id})
                     CREATE (from)-[:RELATES {type: $rel_type, note: $note}]->(to)
                     RETURN true as created",
                )
                .param("from_id", from_id.clone())
                .param("to_id", to_id.clone())
                .param("rel_type", rel_type.clone())
                .param("note", note_text.clone())
            } else {
                neo4rs::query(
                    "MATCH (from:Memory {id: $from_id}), (to:Memory {id: $to_id})
                     CREATE (from)-[:RELATES {type: $rel_type}]->(to)
                     RETURN true as created",
                )
                .param("from_id", from_id.clone())
                .param("to_id", to_id.clone())
                .param("rel_type", rel_type.clone())
            };

            let mut result = graph
                .execute(query)
                .await
                .context("Failed to link memories in Neo4j")?;

            let created = if let Ok(Some(_row)) = result.next().await {
                true
            } else {
                false
            };

            Ok(LinkResponse {
                created,
                from: from_id,
                rel: rel_type,
                to: to_id,
                conflict_warning: None,
            })
        })
    }

    fn unlink_memories(
        &self,
        from_id: &str,
        rel: &EdgeType,
        to_id: &str,
    ) -> Pin<Box<dyn Future<Output = Result<bool>> + Send + '_>> {
        let graph = self.graph.clone();
        let from_id = from_id.to_string();
        let to_id = to_id.to_string();
        let rel_type = format!("{:?}", rel);

        Box::pin(async move {
            let mut result = graph
                .execute(
                    neo4rs::query(
                        "MATCH (from:Memory {id: $from_id})-[r:RELATES {type: $rel_type}]->(to:Memory {id: $to_id})
                         DELETE r RETURN count(r) as deleted"
                    )
                    .param("from_id", from_id)
                    .param("rel_type", rel_type)
                    .param("to_id", to_id),
                )
                .await
                .context("Failed to unlink memories in Neo4j")?;

            if let Ok(Some(row)) = result.next().await {
                let deleted: i64 = row.get("deleted").unwrap_or(0);
                Ok(deleted > 0)
            } else {
                Ok(false)
            }
        })
    }

    fn list_edges(
        &self,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<crate::models::MemoryEdge>>> + Send + '_>> {
        let graph = self.graph.clone();

        Box::pin(async move {
            let mut result = graph
                .execute(
                    neo4rs::query("MATCH (from:Memory)-[r:RELATES]->(to:Memory) RETURN from.id as from_id, to.id as to_id, r.type as rel_type, r.note as note")
                )
                .await
                .context("Failed to list edges from Neo4j")?;

            let mut edges = Vec::new();
            while let Ok(Some(row)) = result.next().await {
                let edge = crate::models::MemoryEdge {
                    from_id: row.get("from_id").context("Missing from_id")?,
                    to_id: row.get("to_id").context("Missing to_id")?,
                    rel_type: row.get("rel_type").context("Missing rel_type")?,
                    note: row.get("note").ok(),
                };
                edges.push(edge);
            }

            Ok(edges)
        })
    }

    fn list_ontology_edges(
        &self,
    ) -> Pin<
        Box<dyn Future<Output = Result<Vec<crate::models::OntologyEdgeForMigration>>> + Send + '_>,
    > {
        let graph = self.graph.clone();

        Box::pin(async move {
            // Query all relationships with their properties
            let cypher = r#"
                MATCH (from)-[r]->(to)
                WHERE r.from_id IS NOT NULL AND r.rel_type IS NOT NULL
                RETURN r.from_id AS from_id, r.from_type AS from_type, 
                       r.to_id AS to_id, r.to_type AS to_type, 
                       r.rel_type AS rel_type, r.note AS note
            "#;

            let mut result = graph
                .execute(neo4rs::query(cypher))
                .await
                .context("Failed to list ontology edges from Neo4j")?;

            let mut edges = Vec::new();
            while let Ok(Some(row)) = result.next().await {
                if let (Ok(from_id), Ok(from_type), Ok(to_id), Ok(to_type), Ok(rel_type)) = (
                    row.get::<String>("from_id"),
                    row.get::<String>("from_type"),
                    row.get::<String>("to_id"),
                    row.get::<String>("to_type"),
                    row.get::<String>("rel_type"),
                ) {
                    let note = row.get::<Option<String>>("note").ok().flatten();
                    edges.push(crate::models::OntologyEdgeForMigration {
                        from_id,
                        from_type,
                        to_id,
                        to_type,
                        rel_type,
                        note,
                    });
                }
            }

            Ok(edges)
        })
    }

    /// Link a memory or concept to another memory or concept (for ontology edges)
    fn create_ontology_edge(
        &self,
        from_id: &str,
        from_type: &str,
        rel_type: &str,
        to_id: &str,
        to_type: &str,
    ) -> Pin<Box<dyn Future<Output = Result<bool>> + Send + '_>> {
        let graph = self.graph.clone();
        let from_id = from_id.to_string();
        let to_id = to_id.to_string();
        let from_label = if from_type == "memory" {
            "Memory"
        } else {
            "Concept"
        };
        let to_label = if to_type == "memory" {
            "Memory"
        } else {
            "Concept"
        };
        let rel_type_str = rel_type.to_string();
        let from_type_str = from_type.to_string();
        let to_type_str = to_type.to_string();

        Box::pin(async move {
            // Create relationship with properties for later querying/deletion
            let query_str = format!(
                "MATCH (from:{} {{id: $from_id}}), (to:{} {{id: $to_id}})
                 CREATE (from)-[r:{}{{from_id: $from_id, from_type: $from_type, to_id: $to_id, to_type: $to_type, rel_type: $rel_type}}]->(to)
                 RETURN true as created",
                from_label, to_label, rel_type_str
            );

            let mut result = graph
                .execute(
                    neo4rs::query(&query_str)
                        .param("from_id", from_id.clone())
                        .param("from_type", from_type_str)
                        .param("to_id", to_id.clone())
                        .param("to_type", to_type_str)
                        .param("rel_type", rel_type_str),
                )
                .await
                .with_context(|| format!("Failed to link {} -> {} in Neo4j", from_id, to_id))?;

            if let Ok(Some(_row)) = result.next().await {
                Ok(true)
            } else {
                Ok(false)
            }
        })
    }

    fn search_hybrid(
        &self,
        opts: &SearchOptions,
        model_name: &str,
        embeddings_enabled: bool,
        config_min_score: f32,
        _config_search: &crate::config::SearchConfig,
    ) -> Pin<Box<dyn Future<Output = Result<SearchResponse>> + Send + '_>> {
        let graph = self.graph.clone();
        let opts_owned = opts.clone();
        let model_name_owned = model_name.to_string();

        Box::pin(async move {
            use std::collections::HashMap;

            let query_text = &opts_owned.query;
            let limit = opts_owned.limit;
            let fetch_limit = limit * 3; // over-fetch for merging
            let mut scores: HashMap<String, f32> = HashMap::new();

            // --- Vector search via embeddings ---
            let use_vector = embeddings_enabled
                && matches!(
                    opts_owned.mode,
                    crate::search::SearchMode::Hybrid | crate::search::SearchMode::Semantic
                );

            if use_vector {
                match crate::embeddings::embed_text(&model_name_owned, query_text) {
                    Ok(query_embedding) => {
                        // Vector search in Neo4j: calculate cosine similarity with all memories
                        let cypher_query = r#"
                            MATCH (m:Memory)
                            WHERE m.embedding IS NOT NULL
                            WITH m, 
                                 [x IN m.embedding | x] AS mem_emb,
                                 $query_emb AS query_emb,
                                 reduce(dot=0.0, i IN range(0, size(m.embedding)-1) | dot + m.embedding[i] * $query_emb[i]) AS dot_product,
                                 sqrt(reduce(sum=0.0, x IN m.embedding | sum + x*x)) AS mem_norm,
                                 sqrt(reduce(sum=0.0, x IN $query_emb | sum + x*x)) AS query_norm
                            WITH m, 
                                 CASE 
                                    WHEN mem_norm = 0.0 OR query_norm = 0.0 THEN 0.0
                                    ELSE dot_product / (mem_norm * query_norm)
                                 END AS similarity
                            WHERE similarity > 0.0
                            RETURN m.id AS id, similarity AS score
                            ORDER BY similarity DESC
                            LIMIT $limit
                        "#;

                        match graph
                            .execute(
                                neo4rs::query(cypher_query)
                                    .param("query_emb", query_embedding.clone())
                                    .param("limit", fetch_limit as i64),
                            )
                            .await
                        {
                            Ok(mut result) => {
                                while let Ok(Some(row)) = result.next().await {
                                    if let Ok(id) = row.get::<String>("id") {
                                        if let Ok(score) = row.get::<f32>("score") {
                                            // Normalize cosine similarity [0,1] to [0,1]
                                            let normalized = (score + 1.0) / 2.0; // Convert [-1,1] to [0,1]
                                            *scores.entry(id).or_default() += normalized * 0.5;
                                        }
                                    }
                                }
                            }
                            Err(e) => tracing::warn!("Neo4j vector search failed: {}", e),
                        }
                    }
                    Err(e) => tracing::warn!("Embedding failed: {}", e),
                }
            }

            // --- Full-text search via content matching ---
            let use_fts = matches!(
                opts_owned.mode,
                crate::search::SearchMode::Hybrid
                    | crate::search::SearchMode::Bm25
                    | crate::search::SearchMode::Keyword
            );

            if use_fts {
                // Simple substring/content search in Neo4j
                let fts_cypher = r#"
                    MATCH (m:Memory)
                    WHERE toLower(m.content) CONTAINS toLower($query)
                    WITH m, 
                         // Simple scoring: penalize by position (earlier matches score higher)
                         1.0 / (1.0 + toFloat(apoc.text.indexOf(toLower(m.content), toLower($query)))) AS relevance
                    RETURN m.id AS id, relevance AS score
                    ORDER BY relevance DESC
                    LIMIT $limit
                "#;

                match graph
                    .execute(
                        neo4rs::query(fts_cypher)
                            .param("query", query_text.clone())
                            .param("limit", fetch_limit as i64),
                    )
                    .await
                {
                    Ok(mut result) => {
                        while let Ok(Some(row)) = result.next().await {
                            if let Ok(id) = row.get::<String>("id") {
                                if let Ok(score) = row.get::<f32>("score") {
                                    // Normalize to [0,1]
                                    let normalized = score.clamp(0.0, 1.0);
                                    *scores.entry(id).or_default() += normalized * 0.3;
                                }
                            }
                        }
                    }
                    Err(e) => tracing::warn!("Neo4j FTS search failed: {}", e),
                }
            }

            // --- Fuzzy search via Levenshtein distance (if available) ---
            let use_fuzzy = matches!(opts_owned.mode, crate::search::SearchMode::Hybrid);

            if use_fuzzy {
                // Fuzzy search using apoc.text.levenshteinDistance if available
                let fuzzy_cypher = r#"
                    MATCH (m:Memory)
                    WITH m,
                         apoc.text.levenshteinDistance(toLower(m.content), toLower($query)) AS distance,
                         toFloat(length(m.content)) AS content_len
                    WITH m,
                         // Similarity = 1 - (distance / max_possible_distance)
                         CASE 
                            WHEN content_len = 0.0 THEN 0.0
                            ELSE 1.0 - (toFloat(distance) / toFloat(length($query) + content_len))
                         END AS similarity
                    WHERE similarity > 0.3
                    RETURN m.id AS id, similarity AS score
                    ORDER BY similarity DESC
                    LIMIT $limit
                "#;

                match graph
                    .execute(
                        neo4rs::query(fuzzy_cypher)
                            .param("query", query_text.clone())
                            .param("limit", fetch_limit as i64),
                    )
                    .await
                {
                    Ok(mut result) => {
                        while let Ok(Some(row)) = result.next().await {
                            if let Ok(id) = row.get::<String>("id") {
                                if let Ok(score) = row.get::<f32>("score") {
                                    let normalized = score.clamp(0.0, 1.0);
                                    *scores.entry(id).or_default() += normalized * 0.2;
                                }
                            }
                        }
                    }
                    Err(_) => {
                        // apoc functions may not be available, skip fuzzy search
                        tracing::debug!("Neo4j fuzzy search unavailable (apoc not installed)");
                    }
                }
            }

            // --- Merge and rank results ---
            let mut results: Vec<(String, f32)> = scores.into_iter().collect();
            results.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

            // Filter by min_score
            let min_score = config_min_score.max(0.0);
            results.retain(|(_id, score)| *score >= min_score);
            let threshold_applied = if min_score > 0.0 {
                Some(min_score)
            } else {
                None
            };

            // Fetch full memory objects and convert to SearchResult
            let mut response_results = Vec::new();
            let mut best_score: Option<f32> = None;

            for (id, combined_score) in results.iter().take(limit) {
                match self.get_memory(id).await {
                    Ok(Some(memory)) => {
                        best_score = Some(combined_score.max(best_score.unwrap_or(0.0)));

                        let age = crate::search::compute_age_days(&memory.created_at);
                        response_results.push(crate::search::SearchResult {
                            id: memory.id,
                            score: *combined_score,
                            memory_type: memory.memory_type,
                            content: memory.content,
                            scopes: memory.scopes,
                            tags: memory.tags,
                            importance: memory.importance,
                            created_at: memory.created_at,
                            source: "search".to_string(),
                            rel_type: None,
                            direction: None,
                            hop_depth: None,
                            parent_id: None,
                            quality_score: memory.quality_score,
                            age_days: age,
                            title: memory.title,
                            context: memory.context,
                            context_chunks: vec![],
                            content_source: None,
                        });
                    }
                    Ok(None) => {
                        tracing::warn!("Memory {} found in search but not retrievable", id);
                    }
                    Err(e) => {
                        tracing::warn!("Error retrieving memory {}: {}", id, e);
                    }
                }
            }

            Ok(crate::search::SearchResponse {
                results: response_results,
                threshold_applied,
                best_score,
            })
        })
    }

    fn add_concept(
        &self,
        name: &str,
        description: Option<&str>,
        scope: Option<&str>,
        id: Option<&str>,
    ) -> Pin<Box<dyn Future<Output = Result<ConceptWithSimilarityWarning>> + Send + '_>> {
        let graph = self.graph.clone();
        let name = name.to_string();
        let description = description.map(|s| s.to_string());
        let scope = scope.map(|s| s.to_string());
        let id_owned = id.map(|s| s.to_string());

        Box::pin(async move {
            let id = id_owned.unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
            let created_at = chrono::Utc::now().to_rfc3339();

            let query = neo4rs::query(
                "CREATE (c:Concept {
                    id: $id,
                    name: $name,
                    description: $description,
                    scope: $scope,
                    created_at: $created_at
                }) RETURN c",
            )
            .param("id", id.clone())
            .param("name", name.clone())
            .param("description", description.clone() as Option<String>)
            .param("scope", scope.clone() as Option<String>)
            .param("created_at", created_at.clone());

            graph
                .run(query)
                .await
                .context("Failed to create concept in Neo4j")?;

            Ok(ConceptWithSimilarityWarning {
                id,
                name,
                description,
                scope,
                created_at,
                similar_concepts: vec![],
            })
        })
    }

    fn get_concept(&self, id: &str) -> Pin<Box<dyn Future<Output = Result<Concept>> + Send + '_>> {
        let graph = self.graph.clone();
        let id = id.to_string();

        Box::pin(async move {
            let mut result = graph
                .execute(
                    neo4rs::query("MATCH (c:Concept {id: $id}) RETURN c").param("id", id.clone()),
                )
                .await
                .context("Failed to get concept from Neo4j")?;

            if let Ok(Some(row)) = result.next().await {
                let node: neo4rs::Node = row.get("c").context("Failed to extract concept node")?;

                let concept = Concept {
                    id: node.get("id").context("Missing id")?,
                    name: node.get("name").context("Missing name")?,
                    description: node.get("description").ok(),
                    scope: node.get("scope").ok(),
                    created_at: node.get("created_at").context("Missing created_at")?,
                };

                Ok(concept)
            } else {
                anyhow::bail!("Concept not found: {}", id)
            }
        })
    }

    fn get_concept_with_instances(
        &self,
        id: &str,
    ) -> Pin<Box<dyn Future<Output = Result<ConceptWithInstances>> + Send + '_>> {
        let graph = self.graph.clone();
        let id = id.to_string();

        Box::pin(async move {
            let mut result = graph
                .execute(
                    neo4rs::query("MATCH (c:Concept {id: $id}) RETURN c").param("id", id.clone()),
                )
                .await
                .context("Failed to get concept from Neo4j")?;

            if let Ok(Some(row)) = result.next().await {
                let node: neo4rs::Node = row.get("c").context("Failed to extract concept node")?;

                let concept = ConceptWithInstances {
                    id: node.get("id").context("Missing id")?,
                    name: node.get("name").context("Missing name")?,
                    description: node.get("description").ok(),
                    scope: node.get("scope").ok(),
                    created_at: node.get("created_at").context("Missing created_at")?,
                    instances: vec![],
                    subclasses: vec![],
                    superclasses: vec![],
                };

                Ok(concept)
            } else {
                anyhow::bail!("Concept not found: {}", id)
            }
        })
    }

    fn list_concepts(
        &self,
        scope: Option<&str>,
        limit: usize,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<Concept>>> + Send + '_>> {
        let graph = self.graph.clone();
        let scope = scope.map(|s| s.to_string());

        Box::pin(async move {
            let query = if let Some(scope_filter) = scope {
                neo4rs::query("MATCH (c:Concept {scope: $scope}) RETURN c LIMIT $limit")
                    .param("scope", scope_filter)
                    .param("limit", limit as i64)
            } else {
                neo4rs::query("MATCH (c:Concept) RETURN c LIMIT $limit")
                    .param("limit", limit as i64)
            };

            let mut result = graph
                .execute(query)
                .await
                .context("Failed to list concepts from Neo4j")?;

            let mut concepts = Vec::new();
            while let Ok(Some(row)) = result.next().await {
                let node: neo4rs::Node = row.get("c").context("Failed to extract concept node")?;

                let concept = Concept {
                    id: node.get("id").context("Missing id")?,
                    name: node.get("name").context("Missing name")?,
                    description: node.get("description").ok(),
                    scope: node.get("scope").ok(),
                    created_at: node.get("created_at").context("Missing created_at")?,
                };

                concepts.push(concept);
            }

            Ok(concepts)
        })
    }

    fn delete_concept(&self, id: &str) -> Pin<Box<dyn Future<Output = Result<bool>> + Send + '_>> {
        let graph = self.graph.clone();
        let id = id.to_string();

        Box::pin(async move {
            let mut result = graph
                .execute(
                    neo4rs::query(
                        "MATCH (c:Concept {id: $id}) DELETE c RETURN count(c) as deleted",
                    )
                    .param("id", id),
                )
                .await
                .context("Failed to delete concept from Neo4j")?;

            if let Ok(Some(row)) = result.next().await {
                let deleted: i64 = row.get("deleted").unwrap_or(0);
                Ok(deleted > 0)
            } else {
                Ok(false)
            }
        })
    }

    fn resolve_concept_id(
        &self,
        id: &str,
    ) -> Pin<Box<dyn Future<Output = Result<String>> + Send + '_>> {
        let id = id.to_string();
        Box::pin(async move { Ok(id) })
    }

    fn search_concepts(
        &self,
        query: &str,
        scope: Option<&str>,
        limit: usize,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<ConceptSearchResult>>> + Send + '_>> {
        let graph = self.graph.clone();
        let query_str = query.to_string();
        let scope = scope.map(|s| s.to_string());

        Box::pin(async move {
            let cypher_query = if let Some(scope_filter) = scope {
                neo4rs::query(
                    "MATCH (c:Concept {scope: $scope}) WHERE c.name =~ ('(?i).*' + $query + '.*') RETURN c LIMIT $limit"
                )
                .param("scope", scope_filter)
                .param("query", query_str)
                .param("limit", limit as i64)
            } else {
                neo4rs::query(
                    "MATCH (c:Concept) WHERE c.name =~ ('(?i).*' + $query + '.*') RETURN c LIMIT $limit"
                )
                .param("query", query_str)
                .param("limit", limit as i64)
            };

            let mut result = graph
                .execute(cypher_query)
                .await
                .context("Failed to search concepts in Neo4j")?;

            let mut results = Vec::new();
            while let Ok(Some(row)) = result.next().await {
                let node: neo4rs::Node = row.get("c").context("Failed to extract concept node")?;

                let search_result = ConceptSearchResult {
                    id: node.get("id").context("Missing id")?,
                    name: node.get("name").context("Missing name")?,
                    description: node.get("description").ok(),
                    scope: node.get("scope").ok(),
                    score: 0.5,
                };

                results.push(search_result);
            }

            Ok(results)
        })
    }

    fn add_ontology_edge(
        &self,
        from_id: &str,
        from_kind: crate::ontology::NodeKind,
        rel: &crate::ontology::OntologyRelType,
        to_id: &str,
        to_kind: crate::ontology::NodeKind,
        note: Option<&str>,
    ) -> Pin<Box<dyn Future<Output = Result<OntologyEdge>> + Send + '_>> {
        let graph = self.graph.clone();
        let from_id = from_id.to_string();
        let to_id = to_id.to_string();
        let rel_type = format!("{:?}", rel);
        let note = note.map(|s| s.to_string());

        Box::pin(async move {
            let from_label = match from_kind {
                crate::ontology::NodeKind::Concept => "Concept",
                crate::ontology::NodeKind::Memory => "Memory",
            };
            let to_label = match to_kind {
                crate::ontology::NodeKind::Concept => "Concept",
                crate::ontology::NodeKind::Memory => "Memory",
            };

            let query = format!(
                "MATCH (from:{} {{id: $from_id}}), (to:{} {{id: $to_id}})
                 CREATE (from)-[r:ONTOLOGY {{type: $rel_type, note: $note}}]->(to)
                 RETURN id(r) as edge_id",
                from_label, to_label
            );

            let q = neo4rs::query(&query)
                .param("from_id", from_id.clone())
                .param("rel_type", rel_type.clone())
                .param("to_id", to_id.clone())
                .param("note", note.clone() as Option<String>);

            let mut result = graph
                .execute(q)
                .await
                .context("Failed to create ontology edge in Neo4j")?;

            let edge_id = if let Ok(Some(row)) = result.next().await {
                row.get::<i64>("edge_id").unwrap_or(1)
            } else {
                1
            };

            let created_at = chrono::Utc::now().to_rfc3339();

            Ok(OntologyEdge {
                id: edge_id,
                from_id,
                from_type: from_kind,
                rel_type,
                to_id,
                to_type: to_kind,
                note,
                created_at,
            })
        })
    }

    fn delete_ontology_edge(
        &self,
        edge_id: i64,
    ) -> Pin<Box<dyn Future<Output = Result<bool>> + Send + '_>> {
        let graph = self.graph.clone();

        Box::pin(async move {
            // Neo4j internal relationship IDs are not directly accessible in parameterized queries
            // For now, we'll query to find the Nth relationship (by ordinal position)
            // and delete it. This is not ideal but works for the current interface.

            let cypher = r#"
                MATCH (from)-[r]->(to)
                WHERE r.from_id IS NOT NULL AND r.rel_type IS NOT NULL
                WITH r, row_number() OVER () AS rn
                WHERE rn = $edge_id
                DELETE r
                RETURN true as deleted
            "#;

            let mut result = graph
                .execute(neo4rs::query(cypher).param("edge_id", edge_id))
                .await
                .context("Failed to delete ontology edge from Neo4j")?;

            if let Ok(Some(_row)) = result.next().await {
                Ok(true)
            } else {
                Ok(false)
            }
        })
    }

    fn query_cypher(
        &self,
        query: &str,
        params: &serde_json::Value,
    ) -> Pin<Box<dyn Future<Output = Result<serde_json::Value>> + Send + '_>> {
        let graph = self.graph.clone();
        let query = query.to_string();
        let params = params.clone();

        Box::pin(async move {
            let mut q = neo4rs::query(&query);

            if let Some(obj) = params.as_object() {
                for (key, value) in obj {
                    match value {
                        serde_json::Value::String(s) => q = q.param(key, s.clone()),
                        serde_json::Value::Number(n) => {
                            if let Some(i) = n.as_i64() {
                                q = q.param(key, i);
                            }
                        }
                        serde_json::Value::Bool(b) => q = q.param(key, *b),
                        _ => {}
                    }
                }
            }

            let mut result = graph
                .execute(q)
                .await
                .context("Failed to execute Cypher query")?;

            let mut rows = Vec::new();
            while let Ok(Some(_row)) = result.next().await {
                // TODO: Convert row to JSON
                rows.push(serde_json::json!({}));
            }

            Ok(serde_json::json!(rows))
        })
    }

    fn get_neighbors(
        &self,
        id: &str,
        depth: usize,
    ) -> Pin<Box<dyn Future<Output = Result<serde_json::Value>> + Send + '_>> {
        let graph = self.graph.clone();
        let id = id.to_string();

        Box::pin(async move {
            let depth = std::cmp::min(depth, 3);

            let mut result = graph
                .execute(
                    neo4rs::query(
                        &format!("MATCH (m:Memory {{id: $id}})-[*1..{}]-(neighbor) RETURN DISTINCT neighbor.id as neighbor_id", depth)
                    )
                    .param("id", id),
                )
                .await
                .context("Failed to get neighbors from Neo4j")?;

            let mut neighbors = Vec::new();
            while let Ok(Some(row)) = result.next().await {
                if let Ok(neighbor_id) = row.get::<String>("neighbor_id") {
                    neighbors.push(neighbor_id);
                }
            }

            Ok(serde_json::json!({ "neighbors": neighbors }))
        })
    }

    fn check_model_mismatch(
        &self,
        configured_model: &str,
    ) -> Pin<Box<dyn Future<Output = Result<Option<(String, String)>>> + Send + '_>> {
        let graph = self.graph.clone();
        let configured_model = configured_model.to_string();

        Box::pin(async move {
            let mut result = graph
                .execute(
                    neo4rs::query("MATCH (m:Memory) WHERE m.embedding_model IS NOT NULL RETURN DISTINCT m.embedding_model LIMIT 1")
                )
                .await
                .context("Failed to check model mismatch")?;

            if let Ok(Some(row)) = result.next().await {
                if let Ok(stored_model) = row.get::<String>("m.embedding_model") {
                    if stored_model != configured_model && !stored_model.is_empty() {
                        return Ok(Some((stored_model, configured_model)));
                    }
                }
            }

            Ok(None)
        })
    }

    fn update_memory_full(
        &self,
        _id: &str,
        _patch: crate::crud::UpdateMemoryPatch,
        _config: &crate::Config,
    ) -> Pin<Box<dyn Future<Output = Result<crate::models::Memory>> + Send + '_>> {
        Box::pin(async move { anyhow::bail!("update_memory_full not yet implemented for Neo4j") })
    }

    fn list_memories_filtered(
        &self,
        scope_filter: Option<String>,
        type_filter: Option<String>,
        limit: usize,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<crate::models::Memory>>> + Send + '_>> {
        // Best-effort: ignore scope/type filters, delegate to list_memories
        let _ = (scope_filter, type_filter);
        self.list_memories(Some(limit))
    }
}

#[cfg(test)]
mod neo4j_integration_tests {
    use super::*;
    use crate::db::Database;
    use crate::models::{AddMemoryRequest, MemoryType};

    /// Test Neo4j connection with local instance
    /// Requires: docker run --publish=7474:7474 --publish=7687:7687 neo4j
    /// Run with: cargo test -- --ignored --test-threads=1
    #[tokio::test]
    #[ignore] // Run manually with local Neo4j instance
    async fn test_neo4j_health_check() {
        let db = Neo4jDatabase::connect("bolt://localhost:7687", "neo4j", "neo4jneo4j")
            .await
            .expect("Failed to connect to Neo4j - is it running?");

        db.health_check().await.expect("Health check failed");
        println!("✓ Neo4j health check passed");
    }

    #[tokio::test]
    #[ignore]
    async fn test_neo4j_memory_crud() {
        let db = Neo4jDatabase::connect("bolt://localhost:7687", "neo4j", "neo4jneo4j")
            .await
            .expect("Failed to connect to Neo4j");

        let config = crate::Config::default();

        // Create memory
        let req = AddMemoryRequest {
            id: None,
            content: "Integration test memory".to_string(),
            memory_type: MemoryType::Semantic,
            scopes: vec!["integration_test".to_string()],
            tags: vec!["test".to_string()],
            importance: 5,
            metadata: serde_json::json!({}),
            links: vec![],
        };

        let response = db
            .add_memory(req, &config)
            .await
            .expect("Failed to add memory");
        println!("✓ Created memory: {}", response.id);

        // Get memory
        let mem = db
            .get_memory(&response.id)
            .await
            .expect("Failed to get memory");
        assert!(mem.is_some(), "Memory should exist");
        let mem = mem.unwrap();
        assert_eq!(mem.content, "Integration test memory");
        assert_eq!(mem.memory_type.to_lowercase(), "semantic");
        println!("✓ Retrieved memory: {}", mem.id);

        // Update memory
        db.update_memory(&response.id, "Updated content")
            .await
            .expect("Failed to update memory");
        println!("✓ Updated memory");

        // Delete memory
        let deleted = db
            .delete_memory(&response.id)
            .await
            .expect("Failed to delete memory");
        assert!(deleted, "Memory should be deleted");
        println!("✓ Deleted memory");
    }

    #[tokio::test]
    #[ignore]
    async fn test_neo4j_relationships() {
        let db = Neo4jDatabase::connect("bolt://localhost:7687", "neo4j", "neo4jneo4j")
            .await
            .expect("Failed to connect to Neo4j");

        let config = crate::Config::default();

        // Create two memories
        let req1 = AddMemoryRequest {
            id: None,
            content: "First memory".to_string(),
            memory_type: MemoryType::Semantic,
            scopes: vec!["rel_test".to_string()],
            tags: vec![],
            importance: 5,
            metadata: serde_json::json!({}),
            links: vec![],
        };

        let id1 = db
            .add_memory(req1, &config)
            .await
            .expect("Failed to create memory 1")
            .id;

        let req2 = AddMemoryRequest {
            id: None,
            content: "Second memory".to_string(),
            memory_type: MemoryType::Semantic,
            scopes: vec!["rel_test".to_string()],
            tags: vec![],
            importance: 5,
            metadata: serde_json::json!({}),
            links: vec![],
        };

        let id2 = db
            .add_memory(req2, &config)
            .await
            .expect("Failed to create memory 2")
            .id;

        // Link them
        let link = db
            .link_memories(
                &id1,
                &crate::models::EdgeType::RelatesTo,
                &id2,
                Some("test link"),
            )
            .await
            .expect("Failed to link memories");
        assert!(link.created, "Link should be created");
        println!("✓ Created relationship: {} -> {} ({})", id1, id2, link.rel);

        // Unlink them
        let unlinked = db
            .unlink_memories(&id1, &crate::models::EdgeType::RelatesTo, &id2)
            .await
            .expect("Failed to unlink");
        assert!(unlinked, "Relationship should be deleted");
        println!("✓ Deleted relationship");

        // Cleanup
        let _ = db.delete_memory(&id1).await;
        let _ = db.delete_memory(&id2).await;
    }

    #[tokio::test]
    #[ignore]
    async fn test_neo4j_concepts() {
        let db = Neo4jDatabase::connect("bolt://localhost:7687", "neo4j", "neo4jneo4j")
            .await
            .expect("Failed to connect to Neo4j");

        // Create concept
        let concept_resp = db
            .add_concept("TestConcept", Some("A test concept"), Some("testing"), None)
            .await
            .expect("Failed to add concept");
        println!("✓ Created concept: {}", concept_resp.id);

        // Get concept
        let concept = db
            .get_concept(&concept_resp.id)
            .await
            .expect("Failed to get concept");
        assert_eq!(concept.name, "TestConcept");
        println!("✓ Retrieved concept: {}", concept.name);

        // List concepts
        let concepts = db
            .list_concepts(Some("testing"), 10)
            .await
            .expect("Failed to list concepts");
        assert!(!concepts.is_empty(), "Should have at least one concept");
        println!("✓ Listed {} concepts", concepts.len());

        // Search concepts
        let search_results = db
            .search_concepts("Test", None, 10)
            .await
            .expect("Failed to search concepts");
        assert!(!search_results.is_empty(), "Should find test concept");
        println!("✓ Found {} concepts in search", search_results.len());

        // Delete concept
        let deleted = db
            .delete_concept(&concept_resp.id)
            .await
            .expect("Failed to delete concept");
        assert!(deleted, "Concept should be deleted");
        println!("✓ Deleted concept");
    }

    #[tokio::test]
    #[ignore]
    async fn test_verify_migrated_memories() {
        use crate::db::Database;

        let db = Neo4jDatabase::connect("bolt://localhost:7687", "neo4j", "neo4jneo4j")
            .await
            .expect("Failed to connect to Neo4j");

        let memories = db
            .list_memories(None)
            .await
            .expect("Failed to list memories");

        println!("Found {} memories in Neo4j:", memories.len());
        for mem in &memories {
            println!("  - {} ({}): {}", mem.id, mem.memory_type, mem.content);
        }

        assert!(
            memories.len() >= 2,
            "Should have at least 2 memories from migration"
        );
    }
}
