pub mod ast;
pub mod lexer;
pub mod parser;
pub mod translator;

use anyhow::{bail, Result};
use sqlx::SqlitePool;
use std::collections::HashMap;

pub use ast::CypherAst;

/// Execute a read-only Cypher query. Rejects write clauses before parsing.
pub async fn execute_read(
    pool: &SqlitePool,
    query: &str,
) -> Result<Vec<HashMap<String, serde_json::Value>>> {
    // Step 1: Strip comments
    let stripped = lexer::strip_comments(query);

    // Step 2: Reject write clauses (token-level, not substring)
    reject_write_clauses(&stripped)?;

    // Step 3: Parse — wrap errors with usage hint
    let ast = parser::parse(&stripped).map_err(|e| {
        anyhow::anyhow!(
            "Cypher parse error: {}\n\
             Supported syntax:\n\
             \x20 MATCH (a:Memory)-[:SUPPORTS]->(b:Memory) RETURN a.memory_id, b.memory_id LIMIT 10\n\
             \x20 MATCH (a)-[:RELATES_TO]-(b) WHERE a.memory_id = '<id>' RETURN b.memory_id\n\
             \x20 MATCH (a)-[*1..3]->(b) RETURN a.memory_id, b.memory_id\n\
             Clauses: MATCH, WHERE, RETURN, ORDER BY, LIMIT, WITH\n\
             Write operations (CREATE, MERGE, SET, DELETE, REMOVE, DROP) are not allowed.",
            e
        )
    })?;

    // Step 4: Translate to SQL
    let (sql, params) = translator::translate(&ast)
        .map_err(|e| anyhow::anyhow!("Cypher translation error: {}", e))?;

    // Step 5: Execute
    let rows = run_query(pool, &sql, &params).await?;
    Ok(rows)
}

const WRITE_KEYWORDS: &[&str] = &["CREATE", "MERGE", "SET", "DELETE", "REMOVE", "DROP"];

fn reject_write_clauses(query: &str) -> Result<()> {
    let tokens = lexer::tokenize(query);
    for token in &tokens {
        if let lexer::Token::Keyword(kw) = token {
            let upper = kw.to_uppercase();
            if WRITE_KEYWORDS.contains(&upper.as_str()) {
                bail!(
                    "'{}' is a write operation and is not allowed via 'voidm graph cypher'.\n\
                     Use 'voidm link' / 'voidm unlink' to modify the graph.\n\
                     Allowed clauses: MATCH, WHERE, RETURN, ORDER BY, LIMIT, WITH.",
                    upper
                );
            }
        }
    }
    Ok(())
}

async fn run_query(
    pool: &SqlitePool,
    sql: &str,
    params: &[serde_json::Value],
) -> Result<Vec<HashMap<String, serde_json::Value>>> {
    use sqlx::Column;
    use sqlx::Row;

    // Build query with dynamic binding
    let mut q = sqlx::query(sql);
    for param in params {
        match param {
            serde_json::Value::String(s) => q = q.bind(s.clone()),
            serde_json::Value::Number(n) => {
                if let Some(i) = n.as_i64() {
                    q = q.bind(i);
                } else if let Some(f) = n.as_f64() {
                    q = q.bind(f);
                }
            }
            serde_json::Value::Null => q = q.bind(Option::<String>::None),
            other => q = q.bind(other.to_string()),
        }
    }

    let rows = q.fetch_all(pool).await?;
    let mut results = Vec::new();

    for row in rows {
        let mut map = HashMap::new();
        for (i, col) in row.columns().iter().enumerate() {
            let val: serde_json::Value = match row.try_get::<String, _>(i) {
                Ok(s) => serde_json::Value::String(s),
                Err(_) => match row.try_get::<i64, _>(i) {
                    Ok(n) => serde_json::Value::Number(n.into()),
                    Err(_) => match row.try_get::<f64, _>(i) {
                        Ok(f) => serde_json::json!(f),
                        Err(_) => serde_json::Value::Null,
                    },
                },
            };
            map.insert(col.name().to_string(), val);
        }
        results.push(map);
    }

    Ok(results)
}
