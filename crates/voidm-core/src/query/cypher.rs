// Cypher Query Operations - Canonical Form
//
// This module defines all database operations as Cypher-like patterns.
// Each operation represents a semantic action independent of the backend.

use serde_json::Value;
use std::collections::HashMap;

/// Represents all database operations in canonical Cypher form
#[derive(Debug, Clone)]
pub enum CypherOperation {
    // Memory CRUD
    MemoryCreate {
        id: String,
        memory_type: String,
        content: String,
        importance: i32,
        tags: Vec<String>,
        scopes: Vec<String>,
        created_at: String,
        embedding: Option<Vec<f32>>,
        metadata: Option<String>,
    },
    MemoryGet {
        id: String,
    },
    MemoryList {
        limit: Option<usize>,
    },
    MemoryDelete {
        id: String,
    },
    MemoryUpdate {
        id: String,
        content: String,
        updated_at: String,
    },
    MemoryResolveId {
        prefix: String,
    },
    MemoryListScopes,

    // Memory Edges/Links
    LinkMemories {
        from_id: String,
        rel_type: String,
        to_id: String,
        note: Option<String>,
        created_at: String,
    },
    UnlinkMemories {
        from_id: String,
        rel_type: String,
        to_id: String,
    },
    ListMemoryEdges,

    // Ontology Concepts
    ConceptCreate {
        id: String,
        name: String,
        description: Option<String>,
        scope: Option<String>,
        created_at: String,
    },
    ConceptGet {
        id: String,
    },
    ConceptList {
        scope: Option<String>,
        limit: usize,
    },
    ConceptDelete {
        id: String,
    },
    ConceptResolveId {
        prefix: String,
    },
    ConceptSearch {
        query: String,
        scope: Option<String>,
        limit: usize,
    },
    ConceptGetWithInstances {
        id: String,
    },

    // Ontology Edges
    OntologyEdgeCreate {
        from_id: String,
        from_type: String,
        rel_type: String,
        to_id: String,
        to_type: String,
        note: Option<String>,
    },
    OntologyEdgeDelete {
        from_id: String,
        rel_type: String,
        to_id: String,
    },
    ListOntologyEdges,

    // Search
    SearchHybrid {
        query: String,
        limit: usize,
        min_score: f32,
        scopes: Vec<String>,
        embedding: Option<Vec<f32>>,
    },

    /// Hybrid search with Reciprocal Rank Fusion (RRF)
    /// Combines vector, BM25, and fuzzy signals using RRF instead of weighted averaging
    SearchHybridRRF {
        query: String,
        limit: usize,
        min_score: f32,
        scopes: Vec<String>,
        embedding: Option<Vec<f32>>,
    },

    // Graph Operations
    QueryCypher {
        query: String,
        params: HashMap<String, Value>,
    },
    GetNeighbors {
        id: String,
        depth: usize,
    },
}

impl CypherOperation {
    /// Get the Cypher pattern for this operation (canonical form)
    pub fn cypher_pattern(&self) -> String {
        match self {
            Self::MemoryCreate { .. } => {
                r#"
                CREATE (m:Memory {
                  id: $id,
                  type: $type,
                  content: $content,
                  importance: $importance,
                  tags: $tags,
                  scopes: $scopes,
                  created_at: $created_at,
                  embedding: $embedding,
                  metadata: $metadata
                })
                RETURN m
                "#.to_string()
            }
            Self::MemoryGet { .. } => {
                r#"
                MATCH (m:Memory {id: $id})
                RETURN m
                "#.to_string()
            }
            Self::MemoryList { .. } => {
                r#"
                MATCH (m:Memory)
                RETURN m
                LIMIT $limit
                "#.to_string()
            }
            Self::MemoryDelete { .. } => {
                r#"
                MATCH (m:Memory {id: $id})
                DELETE m
                RETURN true as deleted
                "#.to_string()
            }
            Self::MemoryUpdate { .. } => {
                r#"
                MATCH (m:Memory {id: $id})
                SET m.content = $content, m.updated_at = $updated_at
                RETURN m
                "#.to_string()
            }
            Self::MemoryResolveId { .. } => {
                r#"
                MATCH (m:Memory)
                WHERE m.id STARTS WITH $prefix
                RETURN m.id
                LIMIT 1
                "#.to_string()
            }
            Self::MemoryListScopes => {
                r#"
                MATCH (m:Memory)
                UNWIND m.scopes as scope
                RETURN DISTINCT scope
                ORDER BY scope
                "#.to_string()
            }
            Self::LinkMemories { .. } => {
                r#"
                MATCH (from:Memory {id: $from_id}), (to:Memory {id: $to_id})
                CREATE (from)-[r {
                  rel_type: $rel_type,
                  note: $note,
                  created_at: $created_at
                }]->(to)
                RETURN r, from.id, to.id
                "#.to_string()
            }
            Self::UnlinkMemories { .. } => {
                r#"
                MATCH (from:Memory {id: $from_id})-[r {rel_type: $rel_type}]->(to:Memory {id: $to_id})
                DELETE r
                RETURN true as deleted
                "#.to_string()
            }
            Self::ListMemoryEdges => {
                r#"
                MATCH (from:Memory)-[r]->(to:Memory)
                RETURN from.id, r.rel_type, to.id, r.note, r.created_at
                "#.to_string()
            }
            Self::ConceptCreate { .. } => {
                r#"
                CREATE (c:Concept {
                  id: $id,
                  name: $name,
                  description: $description,
                  scope: $scope,
                  created_at: $created_at
                })
                RETURN c
                "#.to_string()
            }
            Self::ConceptGet { .. } => {
                r#"
                MATCH (c:Concept {id: $id})
                RETURN c
                "#.to_string()
            }
            Self::ConceptList { .. } => {
                r#"
                MATCH (c:Concept)
                WHERE c.scope = $scope OR $scope IS NULL
                RETURN c
                LIMIT $limit
                "#.to_string()
            }
            Self::ConceptDelete { .. } => {
                r#"
                MATCH (c:Concept {id: $id})
                DELETE c
                RETURN true as deleted
                "#.to_string()
            }
            Self::ConceptResolveId { .. } => {
                r#"
                MATCH (c:Concept)
                WHERE c.id STARTS WITH $prefix
                RETURN c.id
                LIMIT 1
                "#.to_string()
            }
            Self::ConceptSearch { .. } => {
                r#"
                MATCH (c:Concept)
                WHERE (c.name CONTAINS $query OR c.description CONTAINS $query)
                  AND (c.scope = $scope OR $scope IS NULL)
                RETURN c
                LIMIT $limit
                "#.to_string()
            }
            Self::ConceptGetWithInstances { .. } => {
                r#"
                MATCH (c:Concept {id: $id})
                OPTIONAL MATCH (c)-[r]->(related)
                RETURN c, collect({rel_type: type(r), node: related}) as relations
                "#.to_string()
            }
            Self::OntologyEdgeCreate { .. } => {
                r#"
                MATCH (from {id: $from_id}), (to {id: $to_id})
                CREATE (from)-[r {
                  rel_type: $rel_type,
                  from_type: $from_type,
                  to_type: $to_type,
                  note: $note
                }]->(to)
                RETURN r
                "#.to_string()
            }
            Self::OntologyEdgeDelete { .. } => {
                r#"
                MATCH ()-[r]-()
                WHERE r.from_id = $from_id AND r.rel_type = $rel_type AND r.to_id = $to_id
                DELETE r
                RETURN true as deleted
                "#.to_string()
            }
            Self::ListOntologyEdges => {
                r#"
                MATCH (from)-[r]->(to)
                WHERE r.from_type IS NOT NULL
                RETURN r.from_id, r.from_type, r.to_id, r.to_type, r.rel_type, r.note
                "#.to_string()
            }
            Self::SearchHybrid { .. } => {
                r#"
                // Vector search
                MATCH (m:Memory)
                WHERE m.embedding IS NOT NULL
                WITH m, cosine_similarity($embedding, m.embedding) AS vec_score
                WHERE vec_score > 0.0

                UNION

                // Full-text search
                MATCH (m:Memory)
                WHERE m.content CONTAINS $query
                WITH m, 1.0 / (1.0 + COALESCE(position($query in m.content), 999)) AS fts_score

                UNION

                // Fuzzy search
                MATCH (m:Memory)
                WHERE levenshtein(m.content, $query) / COALESCE(length($query), 1) > 0.3
                WITH m, 1.0 - (levenshtein(m.content, $query) / length($query)) AS fuzzy_score

                WITH m,
                     (COALESCE(vec_score, 0.0) * 0.5 +
                      COALESCE(fts_score, 0.0) * 0.3 +
                      COALESCE(fuzzy_score, 0.0) * 0.2) AS combined_score
                "#.to_string()
            }
            Self::SearchHybridRRF { .. } => {
                r#"
                // Vector search
                MATCH (m:Memory)
                WHERE m.embedding IS NOT NULL
                WITH m, cosine_similarity($embedding, m.embedding) AS vec_score
                WHERE vec_score > 0.0
                WITH COLLECT({id: m.id, rank: 1}) AS vec_ranks

                UNION

                // Full-text search
                MATCH (m:Memory)
                WHERE m.content CONTAINS $query
                WITH m, 1.0 / (1.0 + COALESCE(position($query in m.content), 999)) AS fts_score
                WITH COLLECT({id: m.id, rank: 1}) AS fts_ranks

                UNION

                // Fuzzy search
                MATCH (m:Memory)
                WHERE levenshtein(m.content, $query) / COALESCE(length($query), 1) > 0.3
                WITH m, 1.0 - (levenshtein(m.content, $query) / length($query)) AS fuzzy_score
                WITH COLLECT({id: m.id, rank: 1}) AS fuzzy_ranks

                // RRF Fusion: score = Σ 1/(k + rank), k=60
                WITH vec_ranks, fts_ranks, fuzzy_ranks
                UNWIND (vec_ranks + fts_ranks + fuzzy_ranks) AS item
                WITH item.id AS id, SUM(1.0 / (60 + item.rank)) AS rrf_score
                ORDER BY rrf_score DESC
                LIMIT $limit
                MATCH (m:Memory {id: id})
                RETURN m, rrf_score
                "#.to_string()
            }
            Self::QueryCypher { query, .. } => query.clone(),
            Self::GetNeighbors { .. } => {
                r#"
                MATCH path = (n {id: $id})-[*0..$depth]-(neighbor)
                RETURN collect(neighbor) as neighbors
                "#.to_string()
            }
        }
    }

    /// Get the operation name for logging/debugging
    pub fn operation_name(&self) -> &'static str {
        match self {
            Self::MemoryCreate { .. } => "MemoryCreate",
            Self::MemoryGet { .. } => "MemoryGet",
            Self::MemoryList { .. } => "MemoryList",
            Self::MemoryDelete { .. } => "MemoryDelete",
            Self::MemoryUpdate { .. } => "MemoryUpdate",
            Self::MemoryResolveId { .. } => "MemoryResolveId",
            Self::MemoryListScopes => "MemoryListScopes",
            Self::LinkMemories { .. } => "LinkMemories",
            Self::UnlinkMemories { .. } => "UnlinkMemories",
            Self::ListMemoryEdges => "ListMemoryEdges",
            Self::ConceptCreate { .. } => "ConceptCreate",
            Self::ConceptGet { .. } => "ConceptGet",
            Self::ConceptList { .. } => "ConceptList",
            Self::ConceptDelete { .. } => "ConceptDelete",
            Self::ConceptResolveId { .. } => "ConceptResolveId",
            Self::ConceptSearch { .. } => "ConceptSearch",
            Self::ConceptGetWithInstances { .. } => "ConceptGetWithInstances",
            Self::OntologyEdgeCreate { .. } => "OntologyEdgeCreate",
            Self::OntologyEdgeDelete { .. } => "OntologyEdgeDelete",
            Self::ListOntologyEdges => "ListOntologyEdges",
            Self::SearchHybrid { .. } => "SearchHybrid",
            Self::SearchHybridRRF { .. } => "SearchHybridRRF",
            Self::QueryCypher { .. } => "QueryCypher",
            Self::GetNeighbors { .. } => "GetNeighbors",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_memory_get_cypher() {
        let op = CypherOperation::MemoryGet {
            id: "test-id".to_string(),
        };
        let cypher = op.cypher_pattern();
        assert!(cypher.contains("MATCH"));
        assert!(cypher.contains("Memory"));
        assert!(cypher.contains("$id"));
    }

    #[test]
    fn test_operation_name() {
        let op = CypherOperation::MemoryCreate {
            id: "1".to_string(),
            memory_type: "semantic".to_string(),
            content: "test".to_string(),
            importance: 5,
            tags: vec![],
            scopes: vec![],
            created_at: "2026-03-15".to_string(),
            embedding: None,
            metadata: None,
        };
        assert_eq!(op.operation_name(), "MemoryCreate");
    }
}
