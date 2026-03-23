// PostgreSQL Query Translator - Cypher to SQL Conversion
//
// Translates Cypher patterns to PostgreSQL SQL queries.
// PostgreSQL advantages:
// - pgvector for efficient vector search
// - Full-text search with tsvector/tsquery
// - pg_trgm for fuzzy matching
// - Native JSON/JSONB support
// - Array types for tags and scopes

use super::translator::QueryTranslator;
use super::QueryParams;
use serde_json::Value;
use std::collections::HashMap;

pub struct PostgresTranslator;

impl QueryTranslator for PostgresTranslator {
    fn backend_name(&self) -> &'static str {
        "postgres"
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
        embedding: Option<&[f32]>,
        metadata: Option<&str>,
    ) -> Result<(String, QueryParams), String> {
        let params = QueryParams::new()
            .with_param("id", id)
            .with_param("type", memory_type)
            .with_param("content", content)
            .with_param("importance", importance)
            .with_param("tags", tags)
            .with_param("scopes", scopes)
            .with_param("created_at", created_at)
            .with_param("embedding", embedding.map(|e| e.to_vec()))
            .with_param("metadata", metadata);

        let sql = r#"
            INSERT INTO memories (id, type, content, importance, tags, scopes, created_at, embedding, metadata)
            VALUES ($id, $type, $content, $importance, $tags, $scopes, $created_at, $embedding::vector, $metadata)
            RETURNING *
        "#.to_string();

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
        let sql = "UPDATE memories SET content = $content, updated_at = $updated_at WHERE id = $id RETURNING *".to_string();
        Ok((sql, params))
    }

    fn translate_memory_resolve_id(&self, prefix: &str) -> Result<(String, QueryParams), String> {
        let params = QueryParams::new().with_param("prefix", format!("{}%", prefix));
        let sql = "SELECT id FROM memories WHERE id LIKE $prefix LIMIT 1".to_string();
        Ok((sql, params))
    }

    fn translate_list_scopes(&self) -> Result<(String, QueryParams), String> {
        let params = QueryParams::new();
        // PostgreSQL UNNEST for array handling
        let sql = r#"
            SELECT DISTINCT scope
            FROM memories, UNNEST(scopes) as scope
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
            RETURNING *
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
            RETURNING *
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
            .with_param("query", query)
            .with_param("scope", scope)
            .with_param("limit", limit as i32);

        // PostgreSQL full-text search with tsvector
        let sql = r#"
            SELECT * FROM ontology_concepts
            WHERE (name ILIKE $query OR description ILIKE $query)
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
        // PostgreSQL JSON aggregation
        let sql = r#"
            SELECT c.*, 
                   json_agg(json_build_object('rel_type', oe.rel_type, 'related_id', oe.to_id)) as relations
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
            RETURNING *
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
        scopes: &[String],
        embedding: Option<&[f32]>,
    ) -> Result<(String, QueryParams), String> {
        let params = QueryParams::new()
            .with_param("query", query)
            .with_param("limit", limit as i32)
            .with_param("min_score", min_score)
            .with_param("scopes", scopes)
            .with_param("embedding", embedding.map(|e| e.to_vec()));

        // PostgreSQL hybrid search with pgvector and tsvector
        // Vector + FTS + fuzzy combination
        let sql = r#"
            WITH vector_search AS (
              SELECT m.id, m.*, 
                     1 - (m.embedding <-> $embedding::vector) as vec_score
              FROM memories m
              WHERE m.embedding IS NOT NULL
                AND 1 - (m.embedding <-> $embedding::vector) > 0.0
            ),
            fts_search AS (
              SELECT m.id, m.*,
                     ts_rank(to_tsvector('english', m.content), plainto_tsquery('english', $query)) as fts_score
              FROM memories m
              WHERE to_tsvector('english', m.content) @@ plainto_tsquery('english', $query)
            ),
            fuzzy_search AS (
              SELECT m.id, m.*,
                     similarity(m.content, $query) as fuzzy_score
              FROM memories m
              WHERE m.content % $query
            ),
            combined AS (
              SELECT id, m.*,
                     (COALESCE(vec_score, 0) * 0.5 + COALESCE(fts_score, 0) * 0.3 + COALESCE(fuzzy_score, 0) * 0.2) as combined_score
              FROM (
                SELECT DISTINCT COALESCE(vs.id, fs.id, fzy.id) as id,
                       COALESCE(vs.*, fs.*, fzy.*) as m,
                       vs.vec_score, fs.fts_score, fzy.fuzzy_score
                FROM vector_search vs
                FULL OUTER JOIN fts_search fs ON vs.id = fs.id
                FULL OUTER JOIN fuzzy_search fzy ON vs.id = fzy.id
              )
            )
            SELECT * FROM combined
            WHERE combined_score >= $min_score
            ORDER BY combined_score DESC
            LIMIT $limit
        "#.to_string();

        Ok((sql, params))
    }

    fn translate_search_hybrid_rrf(
        &self,
        query: &str,
        limit: usize,
        min_score: f32,
        scopes: &[String],
        embedding: Option<&[f32]>,
    ) -> Result<(String, QueryParams), String> {
        let params = QueryParams::new()
            .with_param("query", query)
            .with_param("limit", limit as i32)
            .with_param("min_score", min_score)
            .with_param("scopes", scopes)
            .with_param("embedding", embedding.map(|e| e.to_vec()));

        // PostgreSQL RRF (Reciprocal Rank Fusion) hybrid search
        // Combines vector, FTS, and fuzzy using RRF formula: Σ 1/(k + rank)
        let sql = r#"
            WITH vector_search AS (
              SELECT m.id,
                     ROW_NUMBER() OVER (ORDER BY 1 - (m.embedding <-> $embedding::vector) DESC) as rank
              FROM memories m
              WHERE m.embedding IS NOT NULL
                AND 1 - (m.embedding <-> $embedding::vector) > 0.0
            ),
            fts_search AS (
              SELECT m.id,
                     ROW_NUMBER() OVER (ORDER BY ts_rank(to_tsvector('english', m.content), plainto_tsquery('english', $query)) DESC) as rank
              FROM memories m
              WHERE to_tsvector('english', m.content) @@ plainto_tsquery('english', $query)
            ),
            fuzzy_search AS (
              SELECT m.id,
                     ROW_NUMBER() OVER (ORDER BY similarity(m.content, $query) DESC) as rank
              FROM memories m
              WHERE m.content % $query
            ),
            rrf_scores AS (
              SELECT COALESCE(vs.id, fs.id, fzy.id) as id,
                     COALESCE(1.0/(60 + vs.rank), 0) +
                     COALESCE(1.0/(60 + fs.rank), 0) +
                     COALESCE(1.0/(60 + fzy.rank), 0) as rrf_score
              FROM vector_search vs
              FULL OUTER JOIN fts_search fs ON vs.id = fs.id
              FULL OUTER JOIN fuzzy_search fzy ON vs.id = fzy.id
            )
            SELECT m.*, rrf.rrf_score
            FROM rrf_scores rrf
            JOIN memories m ON rrf.id = m.id
            WHERE rrf.rrf_score >= $min_score
            ORDER BY rrf.rrf_score DESC
            LIMIT $limit
        "#.to_string();

        Ok((sql, params))
    }

    // Graph Operations

    fn translate_query_cypher(
        &self,
        _query: &str,
        _params: &HashMap<String, Value>,
    ) -> Result<(String, QueryParams), String> {
        Err("Cypher queries not supported on PostgreSQL backend".to_string())
    }

    fn translate_get_neighbors(
        &self,
        id: &str,
        depth: usize,
    ) -> Result<(String, QueryParams), String> {
        let params = QueryParams::new()
            .with_param("id", id)
            .with_param("depth", depth as i32);

        // PostgreSQL WITH RECURSIVE for graph traversal
        let sql = format!(
            r#"
            WITH RECURSIVE neighbors(node_id, current_depth) AS (
              SELECT to_id, 1 FROM memory_edges WHERE from_id = $id
              UNION ALL
              SELECT me.to_id, neighbors.current_depth + 1 
              FROM memory_edges me
              JOIN neighbors ON me.from_id = neighbors.node_id 
              WHERE neighbors.current_depth < {}
            )
            SELECT DISTINCT node_id FROM neighbors
        "#,
            depth
        );

        Ok((sql, params))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_postgres_translator_memory_create() {
        let translator = PostgresTranslator;
        let (query, params) = translator
            .translate_memory_create(
                "id1",
                "semantic",
                "test content",
                5,
                &[],
                &[],
                "2026-03-15",
                None,
                None,
            )
            .unwrap();
        assert!(query.contains("INSERT"));
        assert!(query.contains("RETURNING"));
        assert!(params.get("id").is_some());
    }

    #[test]
    fn test_postgres_translator_memory_get() {
        let translator = PostgresTranslator;
        let (query, params) = translator.translate_memory_get("test-id").unwrap();
        assert!(query.contains("SELECT"));
        assert!(params.get("id").is_some());
    }

    #[test]
    fn test_postgres_translator_list_scopes() {
        let translator = PostgresTranslator;
        let (query, _params) = translator.translate_list_scopes().unwrap();
        assert!(query.contains("UNNEST"));
    }
}
