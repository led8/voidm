// Query Translation Layer - Cypher-to-Backend SQL Translation
//
// This module provides a translation layer that converts Cypher query patterns
// to backend-specific SQL implementations. All database operations are represented
// as Cypher patterns internally, which are then translated to the target backend.
//
// Architecture:
// - Neo4j: Pass-through (uses Cypher directly)
// - SQLite: Translates Cypher to SQL
// - PostgreSQL: Translates Cypher to SQL with pgvector/FTS support

use serde_json::{json, Value};
use std::collections::HashMap;

pub mod cypher;
pub mod postgres;
pub mod sqlite;
pub mod translator;

pub use cypher::CypherOperation;
pub use postgres::PostgresTranslator;
pub use sqlite::SqliteTranslator;
pub use translator::{Neo4jTranslator, QueryTranslator};

/// Parameters for query execution
#[derive(Debug, Clone)]
pub struct QueryParams {
    pub params: HashMap<String, Value>,
}

impl QueryParams {
    pub fn new() -> Self {
        Self {
            params: HashMap::new(),
        }
    }

    pub fn with_param(mut self, key: impl Into<String>, value: impl Into<Value>) -> Self {
        self.params.insert(key.into(), value.into());
        self
    }

    pub fn get(&self, key: &str) -> Option<&Value> {
        self.params.get(key)
    }

    pub fn to_json(&self) -> Value {
        json!(self.params)
    }
}

impl Default for QueryParams {
    fn default() -> Self {
        Self::new()
    }
}

/// Query result abstraction
#[derive(Debug, Clone)]
pub enum QueryResult {
    Empty,
    Single(Value),
    Multiple(Vec<Value>),
    Affected(usize),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_query_params() {
        let params = QueryParams::new()
            .with_param("id", "test-id")
            .with_param("count", 42);

        assert_eq!(
            params.get("id").map(|v| v.as_str()).flatten(),
            Some("test-id")
        );
        assert_eq!(params.get("count").map(|v| v.as_i64()).flatten(), Some(42));
    }
}
