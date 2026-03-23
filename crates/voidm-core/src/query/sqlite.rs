// SQLite Query Translator - Cypher to SQL Conversion
//
// Translates Cypher patterns to SQLite SQL queries.
// SQLite considerations:
// - JSON functions for storing tags, metadata
// - FTS5 for full-text search
// - Custom functions for vector operations
// - LIKE for substring/fuzzy matching

use super::translator::QueryTranslator;
use super::QueryParams;
use serde_json::Value;
use std::collections::HashMap;

pub struct SqliteTranslator;

impl QueryTranslator for SqliteTranslator {
    fn backend_name(&self) -> &'static str {
        "sqlite"
    }

    fn translate(
        &self,
        op: &super::cypher::CypherOperation,
    ) -> Result<(String, QueryParams), String> {
        match op {
            super::cypher::CypherOperation::MemoryCreate {
                id,
                memory_type,
                content,
                importance,
                tags,
                scopes,
                created_at,
                embedding,
                metadata,
            } => self.translate_memory_create(
                id,
                memory_type,
                content,
                *importance,
                tags,
                scopes,
                created_at,
                embedding.as_deref(),
                metadata.as_deref(),
            ),
            super::cypher::CypherOperation::MemoryGet { id } => self.translate_memory_get(id),
            super::cypher::CypherOperation::MemoryList { limit } => {
                self.translate_memory_list(*limit)
            }
            super::cypher::CypherOperation::MemoryDelete { id } => self.translate_memory_delete(id),
            super::cypher::CypherOperation::MemoryUpdate {
                id,
                content,
                updated_at,
            } => self.translate_memory_update(id, content, updated_at),
            super::cypher::CypherOperation::MemoryResolveId { prefix } => {
                self.translate_memory_resolve_id(prefix)
            }
            super::cypher::CypherOperation::MemoryListScopes => self.translate_list_scopes(),
            super::cypher::CypherOperation::LinkMemories {
                from_id,
                rel_type,
                to_id,
                note,
                created_at,
            } => {
                self.translate_link_memories(from_id, rel_type, to_id, note.as_deref(), created_at)
            }
            super::cypher::CypherOperation::UnlinkMemories {
                from_id,
                rel_type,
                to_id,
            } => self.translate_unlink_memories(from_id, rel_type, to_id),
            super::cypher::CypherOperation::ListMemoryEdges => self.translate_list_memory_edges(),
            super::cypher::CypherOperation::ConceptCreate {
                id,
                name,
                description,
                scope,
                created_at,
            } => self.translate_concept_create(
                id,
                name,
                description.as_deref(),
                scope.as_deref(),
                created_at,
            ),
            super::cypher::CypherOperation::ConceptGet { id } => self.translate_concept_get(id),
            super::cypher::CypherOperation::ConceptList { scope, limit } => {
                self.translate_concept_list(scope.as_deref(), *limit)
            }
            super::cypher::CypherOperation::ConceptDelete { id } => {
                self.translate_concept_delete(id)
            }
            super::cypher::CypherOperation::ConceptResolveId { prefix } => {
                self.translate_concept_resolve_id(prefix)
            }
            super::cypher::CypherOperation::ConceptSearch {
                query,
                scope,
                limit,
            } => self.translate_concept_search(query, scope.as_deref(), *limit),
            super::cypher::CypherOperation::ConceptGetWithInstances { id } => {
                self.translate_concept_get_with_instances(id)
            }
            super::cypher::CypherOperation::OntologyEdgeCreate {
                from_id,
                from_type,
                rel_type,
                to_id,
                to_type,
                note,
            } => self.translate_ontology_edge_create(
                from_id,
                from_type,
                rel_type,
                to_id,
                to_type,
                note.as_deref(),
            ),
            super::cypher::CypherOperation::OntologyEdgeDelete {
                from_id,
                rel_type,
                to_id,
            } => self.translate_ontology_edge_delete(from_id, rel_type, to_id),
            super::cypher::CypherOperation::ListOntologyEdges => {
                self.translate_list_ontology_edges()
            }
            super::cypher::CypherOperation::SearchHybrid {
                query,
                limit,
                min_score,
                scopes,
                embedding,
            } => self.translate_search_hybrid(
                query,
                *limit,
                *min_score,
                scopes,
                embedding.as_deref(),
            ),
            super::cypher::CypherOperation::SearchHybridRRF {
                query,
                limit,
                min_score,
                scopes,
                embedding,
            } => self.translate_search_hybrid_rrf(
                query,
                *limit,
                *min_score,
                scopes,
                embedding.as_deref(),
            ),
            super::cypher::CypherOperation::QueryCypher { query, params } => {
                self.translate_query_cypher(query, params)
            }
            super::cypher::CypherOperation::GetNeighbors { id, depth } => {
                self.translate_get_neighbors(id, *depth)
            }
        }
    }

    // Memory CRUD Operations

    fn translate_memory_create(
        &self,
        id: &str,
        memory_type: &str,
        content: &str,
        importance: i32,
        tags: &[String],
        scopes: &[String],
        created_at: &str,
        _embedding: Option<&[f32]>,
        metadata: Option<&str>,
    ) -> Result<(String, QueryParams), String> {
        let params = QueryParams::new()
            .with_param("id", id)
            .with_param("type", memory_type)
            .with_param("content", content)
            .with_param("importance", importance)
            .with_param("tags", serde_json::to_string(tags).unwrap_or_default())
            .with_param("scopes", serde_json::to_string(scopes).unwrap_or_default())
            .with_param("created_at", created_at)
            .with_param("metadata", metadata);

        let sql = r#"
            INSERT INTO memories (id, type, content, importance, tags, scopes, created_at, metadata)
            VALUES ($id, $type, $content, $importance, $tags, $scopes, $created_at, $metadata)
        "#
        .to_string();

        Ok((sql, params))
    }

    fn translate_memory_get(&self, id: &str) -> Result<(String, QueryParams), String> {
        let params = QueryParams::new().with_param("id", id);
        let sql = "SELECT * FROM memories WHERE id = $id".to_string();
        Ok((sql, params))
    }

    fn translate_memory_list(&self, limit: Option<usize>) -> Result<(String, QueryParams), String> {
        let limit_val = limit.unwrap_or(1000);
        let params = QueryParams::new().with_param("limit", limit_val as i32);
        let sql = "SELECT * FROM memories LIMIT $limit".to_string();
        Ok((sql, params))
    }

    fn translate_memory_delete(&self, id: &str) -> Result<(String, QueryParams), String> {
        let params = QueryParams::new().with_param("id", id);
        let sql = "DELETE FROM memories WHERE id = $id".to_string();
        Ok((sql, params))
    }

    fn translate_memory_update(
        &self,
        id: &str,
        content: &str,
        updated_at: &str,
    ) -> Result<(String, QueryParams), String> {
        let params = QueryParams::new()
            .with_param("id", id)
            .with_param("content", content)
            .with_param("updated_at", updated_at);
        let sql = "UPDATE memories SET content = $content, updated_at = $updated_at WHERE id = $id"
            .to_string();
        Ok((sql, params))
    }

    fn translate_memory_resolve_id(&self, prefix: &str) -> Result<(String, QueryParams), String> {
        let params = QueryParams::new().with_param("prefix", format!("{}%", prefix));
        let sql = "SELECT id FROM memories WHERE id LIKE $prefix LIMIT 1".to_string();
        Ok((sql, params))
    }

    fn translate_list_scopes(&self) -> Result<(String, QueryParams), String> {
        let params = QueryParams::new();
        // SQLite doesn't have UNNEST; we need to use JSON extraction
        let sql = r#"
            SELECT DISTINCT json_extract(scopes_json, '$[' || number || ']') as scope
            FROM memories,
            json_each(memories.scopes) as scopes_json
            WHERE scope IS NOT NULL
            ORDER BY scope
        "#
        .to_string();
        Ok((sql, params))
    }

    // Memory Edges/Links

    fn translate_link_memories(
        &self,
        from_id: &str,
        rel_type: &str,
        to_id: &str,
        note: Option<&str>,
        created_at: &str,
    ) -> Result<(String, QueryParams), String> {
        let params = QueryParams::new()
            .with_param("from_id", from_id)
            .with_param("rel_type", rel_type)
            .with_param("to_id", to_id)
            .with_param("note", note)
            .with_param("created_at", created_at);
        let sql = r#"
            INSERT INTO memory_edges (from_id, to_id, rel_type, note, created_at)
            VALUES ($from_id, $to_id, $rel_type, $note, $created_at)
        "#
        .to_string();
        Ok((sql, params))
    }

    fn translate_unlink_memories(
        &self,
        from_id: &str,
        rel_type: &str,
        to_id: &str,
    ) -> Result<(String, QueryParams), String> {
        let params = QueryParams::new()
            .with_param("from_id", from_id)
            .with_param("rel_type", rel_type)
            .with_param("to_id", to_id);
        let sql = r#"
            DELETE FROM memory_edges
            WHERE from_id = $from_id AND to_id = $to_id AND rel_type = $rel_type
        "#
        .to_string();
        Ok((sql, params))
    }

    fn translate_list_memory_edges(&self) -> Result<(String, QueryParams), String> {
        let params = QueryParams::new();
        let sql = "SELECT from_id, rel_type, to_id, note, created_at FROM memory_edges".to_string();
        Ok((sql, params))
    }

    // Ontology Concepts

    fn translate_concept_create(
        &self,
        id: &str,
        name: &str,
        description: Option<&str>,
        scope: Option<&str>,
        created_at: &str,
    ) -> Result<(String, QueryParams), String> {
        let params = QueryParams::new()
            .with_param("id", id)
            .with_param("name", name)
            .with_param("description", description)
            .with_param("scope", scope)
            .with_param("created_at", created_at);
        let sql = r#"
            INSERT INTO ontology_concepts (id, name, description, scope, created_at)
            VALUES ($id, $name, $description, $scope, $created_at)
        "#
        .to_string();
        Ok((sql, params))
    }

    fn translate_concept_get(&self, id: &str) -> Result<(String, QueryParams), String> {
        let params = QueryParams::new().with_param("id", id);
        let sql = "SELECT * FROM ontology_concepts WHERE id = $id".to_string();
        Ok((sql, params))
    }

    fn translate_concept_list(
        &self,
        scope: Option<&str>,
        limit: usize,
    ) -> Result<(String, QueryParams), String> {
        let params = QueryParams::new()
            .with_param("scope", scope)
            .with_param("limit", limit as i32);
        let sql = r#"
            SELECT * FROM ontology_concepts
            WHERE scope IS NULL OR scope = $scope
            LIMIT $limit
        "#
        .to_string();
        Ok((sql, params))
    }

    fn translate_concept_delete(&self, id: &str) -> Result<(String, QueryParams), String> {
        let params = QueryParams::new().with_param("id", id);
        let sql = "DELETE FROM ontology_concepts WHERE id = $id".to_string();
        Ok((sql, params))
    }

    fn translate_concept_resolve_id(&self, prefix: &str) -> Result<(String, QueryParams), String> {
        let params = QueryParams::new().with_param("prefix", format!("{}%", prefix));
        let sql = "SELECT id FROM ontology_concepts WHERE id LIKE $prefix LIMIT 1".to_string();
        Ok((sql, params))
    }

    fn translate_concept_search(
        &self,
        query: &str,
        scope: Option<&str>,
        limit: usize,
    ) -> Result<(String, QueryParams), String> {
        let params = QueryParams::new()
            .with_param("query", format!("%{}%", query))
            .with_param("scope", scope)
            .with_param("limit", limit as i32);
        let sql = r#"
            SELECT * FROM ontology_concepts
            WHERE (name LIKE $query OR description LIKE $query)
              AND (scope IS NULL OR scope = $scope)
            LIMIT $limit
        "#
        .to_string();
        Ok((sql, params))
    }

    fn translate_concept_get_with_instances(
        &self,
        id: &str,
    ) -> Result<(String, QueryParams), String> {
        let params = QueryParams::new().with_param("id", id);
        // This is a complex operation requiring JOINs
        let sql = r#"
            SELECT c.*, 
                   json_group_array(json_object('rel_type', oe.rel_type, 'related_id', oe.to_id)) as relations
            FROM ontology_concepts c
            LEFT JOIN ontology_edges oe ON c.id = oe.from_id
            WHERE c.id = $id
            GROUP BY c.id
        "#.to_string();
        Ok((sql, params))
    }

    // Ontology Edges

    fn translate_ontology_edge_create(
        &self,
        from_id: &str,
        from_type: &str,
        rel_type: &str,
        to_id: &str,
        to_type: &str,
        note: Option<&str>,
    ) -> Result<(String, QueryParams), String> {
        let params = QueryParams::new()
            .with_param("from_id", from_id)
            .with_param("from_type", from_type)
            .with_param("rel_type", rel_type)
            .with_param("to_id", to_id)
            .with_param("to_type", to_type)
            .with_param("note", note);
        let sql = r#"
            INSERT INTO ontology_edges (from_id, from_type, rel_type, to_id, to_type, note)
            VALUES ($from_id, $from_type, $rel_type, $to_id, $to_type, $note)
        "#
        .to_string();
        Ok((sql, params))
    }

    fn translate_ontology_edge_delete(
        &self,
        from_id: &str,
        rel_type: &str,
        to_id: &str,
    ) -> Result<(String, QueryParams), String> {
        let params = QueryParams::new()
            .with_param("from_id", from_id)
            .with_param("rel_type", rel_type)
            .with_param("to_id", to_id);
        let sql = r#"
            DELETE FROM ontology_edges
            WHERE from_id = $from_id AND rel_type = $rel_type AND to_id = $to_id
        "#
        .to_string();
        Ok((sql, params))
    }

    fn translate_list_ontology_edges(&self) -> Result<(String, QueryParams), String> {
        let params = QueryParams::new();
        let sql = "SELECT from_id, from_type, to_id, to_type, rel_type, note FROM ontology_edges"
            .to_string();
        Ok((sql, params))
    }

    // Search

    fn translate_search_hybrid(
        &self,
        query: &str,
        limit: usize,
        min_score: f32,
        _scopes: &[String],
        _embedding: Option<&[f32]>,
    ) -> Result<(String, QueryParams), String> {
        let params = QueryParams::new()
            .with_param("query", query)
            .with_param("limit", limit as i32)
            .with_param("min_score", min_score);

        // SQLite FTS5 hybrid search
        // Note: This is simplified; actual implementation would need:
        // - Custom vector functions for embeddings
        // - FTS5 virtual table for full-text search
        // - Scoring combination
        let sql = r#"
            SELECT m.* FROM memories m
            WHERE m.content MATCH $query
            LIMIT $limit
        "#
        .to_string();
        Ok((sql, params))
    }

    fn translate_search_hybrid_rrf(
        &self,
        query: &str,
        limit: usize,
        min_score: f32,
        _scopes: &[String],
        _embedding: Option<&[f32]>,
    ) -> Result<(String, QueryParams), String> {
        let params = QueryParams::new()
            .with_param("query", query)
            .with_param("limit", limit as i32)
            .with_param("min_score", min_score);

        // SQLite RRF hybrid search (Reciprocal Rank Fusion)
        // Note: Implementation handled by search_with_rrf() in search.rs
        // This translator stub is for interface compatibility
        let sql = r#"
            SELECT m.* FROM memories m
            WHERE m.content MATCH $query
            LIMIT $limit
        "#
        .to_string();
        Ok((sql, params))
    }

    // Graph Operations

    fn translate_query_cypher(
        &self,
        _query: &str,
        _params: &HashMap<String, Value>,
    ) -> Result<(String, QueryParams), String> {
        Err("Cypher queries not supported on SQLite backend".to_string())
    }

    fn translate_get_neighbors(
        &self,
        id: &str,
        depth: usize,
    ) -> Result<(String, QueryParams), String> {
        let params = QueryParams::new()
            .with_param("id", id)
            .with_param("depth", depth as i32);

        // SQLite doesn't have native recursive WITH support for graphs
        // This is a simplified version; full implementation would need CTE
        let sql = r#"
            WITH RECURSIVE neighbors(node_id, current_depth) AS (
              SELECT to_id, 0 FROM memory_edges WHERE from_id = $id
              UNION ALL
              SELECT to_id, current_depth + 1 FROM memory_edges
              WHERE from_id = neighbors.node_id AND current_depth < $depth
            )
            SELECT DISTINCT node_id FROM neighbors
        "#
        .to_string();
        Ok((sql, params))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sqlite_translator_memory_get() {
        let translator = SqliteTranslator;
        let (query, params) = translator.translate_memory_get("test-id").unwrap();
        assert!(query.contains("SELECT"));
        assert!(query.contains("memories"));
        assert!(params.get("id").is_some());
    }

    #[test]
    fn test_sqlite_translator_concept_create() {
        let translator = SqliteTranslator;
        let (query, params) = translator
            .translate_concept_create("id1", "test", Some("desc"), None, "2026-03-15")
            .unwrap();
        assert!(query.contains("INSERT"));
        assert!(query.contains("ontology_concepts"));
        assert!(params.get("name").is_some());
    }

    #[test]
    fn test_sqlite_translator_resolve_id() {
        let translator = SqliteTranslator;
        let (query, params) = translator.translate_memory_resolve_id("abc").unwrap();
        assert!(query.contains("LIKE"));
        assert!(params.get("prefix").is_some());
    }
}
