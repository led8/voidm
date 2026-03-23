# Phase 2: Cypher-First Architecture & Query Translation Design

**Date**: 2026-03-15  
**Status**: DESIGN DOCUMENT  
**Target**: PostgreSQL Backend Implementation (Phase 3)

---

## Executive Summary

This phase designs a **Cypher-first** architecture where:
1. All database operations are represented as **Cypher query patterns** (canonical form)
2. Each backend implements a **QueryTranslator** that converts Cypher → backend-specific SQL/Cypher
3. Neo4j uses Cypher directly (no translation needed)
4. SQLite & PostgreSQL translate Cypher patterns to SQL equivalents

**Why Cypher-First?**
- **Consistency**: Same logical operations across all backends
- **Maintainability**: Single source of truth for query semantics
- **Extensibility**: Easy to add new backends (just write a translator)
- **User-Facing**: Internal only (CLI remains unchanged)

---

## Architecture Overview

```
┌─────────────────────────────────────────────────────────────────┐
│                     Voidm Core API                              │
│  (Memory CRUD, Search, Ontology, Links - no changes)            │
└────────────────────────┬────────────────────────────────────────┘
                         │
                         ▼
         ┌──────────────────────────────┐
         │   Database Trait (Generic)    │
         │  (Still accepts domain models)│
         └────────────┬─────────────────┘
                      │
                      ▼
         ┌──────────────────────────────┐
         │   QueryTranslator Layer       │
         │   (NEW - Converts Cypher)     │
         └─┬────────────────┬────────────┘
           │                │
    ┌──────▼──────┐  ┌──────▼──────────┐
    │  Neo4j:     │  │  SQLite/Postgres:│
    │  PASS-THRU  │  │  TRANSLATE       │
    │  (Cypher)   │  │  (to SQL)        │
    └──────┬──────┘  └──────┬──────────┘
           │                │
    ┌──────▼──────┐  ┌──────▼──────────┐
    │ Neo4j DB    │  │  SQLx (sqlite)   │
    │ (neo4rs)    │  │  OR sqlx(pg)     │
    └─────────────┘  └──────────────────┘
```

---

## Cypher Query Categories

All operations mapped to **Cypher patterns** that backend translators convert:

### 1. MEMORY CRUD OPERATIONS

#### 1.1: Add Memory
**Cypher Pattern**:
```cypher
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
```

**Translation to SQL**:
- SQLite: `INSERT INTO memories (...) VALUES (...)`
- PostgreSQL: `INSERT INTO memories (...) VALUES (...) RETURNING *`

---

#### 1.2: Get Memory
**Cypher Pattern**:
```cypher
MATCH (m:Memory {id: $id})
RETURN m
```

**Translation**:
- SQLite: `SELECT * FROM memories WHERE id = $id`
- PostgreSQL: `SELECT * FROM memories WHERE id = $id`

---

#### 1.3: List Memories
**Cypher Pattern**:
```cypher
MATCH (m:Memory)
RETURN m
LIMIT $limit
```

**Translation**:
- SQLite: `SELECT * FROM memories LIMIT $limit`
- PostgreSQL: `SELECT * FROM memories LIMIT $limit`

---

#### 1.4: Delete Memory
**Cypher Pattern**:
```cypher
MATCH (m:Memory {id: $id})
DELETE m
RETURN count(m) > 0 as deleted
```

**Translation**:
- SQLite: `DELETE FROM memories WHERE id = $id` then check rows affected
- PostgreSQL: Similar with RETURNING

---

#### 1.5: Update Memory
**Cypher Pattern**:
```cypher
MATCH (m:Memory {id: $id})
SET m.content = $content, m.updated_at = $updated_at
RETURN m
```

**Translation**:
- SQLite: `UPDATE memories SET content = $content, updated_at = $updated_at WHERE id = $id`
- PostgreSQL: Same

---

#### 1.6: Resolve Memory ID (prefix → full UUID)
**Cypher Pattern**:
```cypher
MATCH (m:Memory)
WHERE m.id STARTS WITH $prefix
RETURN m.id LIMIT 1
```

**Translation**:
- SQLite: `SELECT id FROM memories WHERE id LIKE ($prefix || '%') LIMIT 1`
- PostgreSQL: `SELECT id FROM memories WHERE id LIKE ($prefix || '%') LIMIT 1`

---

#### 1.7: List Scopes
**Cypher Pattern**:
```cypher
MATCH (m:Memory)
UNWIND m.scopes as scope
RETURN DISTINCT scope
```

**Translation**:
- SQLite: `SELECT DISTINCT scope FROM memory_scopes ORDER BY scope`
- PostgreSQL: `SELECT DISTINCT UNNEST(scopes) as scope FROM memories ORDER BY scope`

---

### 2. MEMORY EDGES/LINKS

#### 2.1: Link Memories
**Cypher Pattern**:
```cypher
MATCH (from:Memory {id: $from_id}), (to:Memory {id: $to_id})
CREATE (from)-[r:RELATES_TO {
  rel_type: $rel_type,
  note: $note,
  created_at: $created_at
}]->(to)
RETURN r, from.id, to.id
```

**Translation**:
- SQLite: `INSERT INTO memory_edges (from_id, to_id, rel_type, note, created_at) VALUES (...)`
- PostgreSQL: Similar

---

#### 2.2: Unlink Memories
**Cypher Pattern**:
```cypher
MATCH (from:Memory {id: $from_id})-[r {rel_type: $rel_type}]->(to:Memory {id: $to_id})
DELETE r
RETURN count(r) > 0 as deleted
```

**Translation**:
- SQLite: `DELETE FROM memory_edges WHERE from_id = $from_id AND to_id = $to_id AND rel_type = $rel_type`
- PostgreSQL: Same

---

#### 2.3: List Memory Edges
**Cypher Pattern**:
```cypher
MATCH (from:Memory)-[r]->(to:Memory)
RETURN from.id, r.rel_type, to.id, r.note, r.created_at
```

**Translation**:
- SQLite: `SELECT from_id, rel_type, to_id, note, created_at FROM memory_edges`
- PostgreSQL: Same

---

### 3. ONTOLOGY CONCEPTS

#### 3.1: Add Concept
**Cypher Pattern**:
```cypher
CREATE (c:Concept {
  id: $id,
  name: $name,
  description: $description,
  scope: $scope,
  created_at: $created_at
})
RETURN c
```

**Translation**:
- SQLite: `INSERT INTO ontology_concepts (...) VALUES (...)`
- PostgreSQL: Same

---

#### 3.2: Get Concept
**Cypher Pattern**:
```cypher
MATCH (c:Concept {id: $id})
RETURN c
```

**Translation**:
- SQLite: `SELECT * FROM ontology_concepts WHERE id = $id`
- PostgreSQL: Same

---

#### 3.3: Get Concept with Instances/Relations
**Cypher Pattern**:
```cypher
MATCH (c:Concept {id: $id})
OPTIONAL MATCH (c)-[r]->(related)
RETURN c, collect({type: type(r), node: related}) as relations
```

**Translation**: More complex SQL JOIN pattern (see detailed design below)

---

#### 3.4: List Concepts
**Cypher Pattern**:
```cypher
MATCH (c:Concept)
WHERE c.scope = $scope OR $scope IS NULL
RETURN c
LIMIT $limit
```

**Translation**:
- SQLite: `SELECT * FROM ontology_concepts WHERE scope IS NULL OR scope = $scope LIMIT $limit`
- PostgreSQL: Same

---

#### 3.5: Delete Concept
**Cypher Pattern**:
```cypher
MATCH (c:Concept {id: $id})
DELETE c
RETURN count(c) > 0 as deleted
```

**Translation**:
- SQLite: `DELETE FROM ontology_concepts WHERE id = $id`
- PostgreSQL: Same

---

#### 3.6: Search Concepts
**Cypher Pattern**:
```cypher
MATCH (c:Concept)
WHERE c.name CONTAINS $query OR c.description CONTAINS $query
AND (c.scope = $scope OR $scope IS NULL)
RETURN c
LIMIT $limit
```

**Translation**:
- SQLite: Use FTS5 if available, or LIKE fallback
- PostgreSQL: Use full-text search or ILIKE

---

### 4. ONTOLOGY EDGES

#### 4.1: Add Ontology Edge
**Cypher Pattern**:
```cypher
MATCH (from {id: $from_id}), (to {id: $to_id})
CREATE (from)-[r {
  rel_type: $rel_type,
  from_type: $from_type,
  to_type: $to_type,
  note: $note
}]->(to)
RETURN r
```

**Translation**:
- SQLite: `INSERT INTO ontology_edges (...) VALUES (...)`
- PostgreSQL: Same

---

#### 4.2: Delete Ontology Edge
**Cypher Pattern**:
```cypher
MATCH ()-[r]-()
WHERE r.from_id = $from_id AND r.rel_type = $rel_type AND r.to_id = $to_id
DELETE r
RETURN count(r) > 0 as deleted
```

**Translation**:
- SQLite: `DELETE FROM ontology_edges WHERE ...`
- PostgreSQL: Same

---

#### 4.3: List Ontology Edges
**Cypher Pattern**:
```cypher
MATCH (from)-[r]->(to)
WHERE r.from_type IS NOT NULL
RETURN r.from_id, r.from_type, r.to_id, r.to_type, r.rel_type, r.note
```

**Translation**:
- SQLite: `SELECT * FROM ontology_edges`
- PostgreSQL: Same

---

### 5. SEARCH (HYBRID)

#### 5.1: Hybrid Search (Cypher Pattern)
**Cypher Pattern**:
```cypher
// Vector search
MATCH (m:Memory)
WHERE m.embedding IS NOT NULL
WITH m, dot_product(...) AS vec_score
WHERE vec_score > 0.0

// Full-text search
MATCH (m:Memory)
WHERE m.content CONTAINS $query
WITH m, position($query in m.content) AS fts_score

// Fuzzy search
MATCH (m:Memory)
WHERE levenshtein(m.content, $query) / length($query) > 0.3
WITH m, 1.0 - (levenshtein(...) / length(...)) AS fuzzy_score

// Merge and score
WITH m,
     (vec_score * 0.5 + fts_score * 0.3 + fuzzy_score * 0.2) AS combined_score
WHERE combined_score >= $min_score
RETURN m, combined_score
ORDER BY combined_score DESC
LIMIT $limit
```

**Translation**: Backend-specific SQL with window functions, text search, etc.

---

## Implementation Strategy

### Phase 2.1: Define Query Abstraction Layer

**New File**: `crates/voidm-core/src/query/mod.rs`

Structure:
```rust
// Query abstraction types
pub enum CypherOp {
    MemoryCreate { ... },
    MemoryGet { id: String },
    MemoryList { limit: Option<usize> },
    MemoryDelete { id: String },
    ...
}

// Translator trait
pub trait QueryTranslator {
    fn translate_memory_get(&self, id: &str) -> String;
    fn translate_memory_create(&self, params: &MemoryCreateParams) -> (String, Params);
    // ... etc for all operations
}

// Implementations
pub struct Neo4jTranslator;
pub struct SqliteTranslator;
pub struct PostgresTranslator;

impl QueryTranslator for Neo4jTranslator { ... }
impl QueryTranslator for SqliteTranslator { ... }
impl QueryTranslator for PostgresTranslator { ... }
```

---

### Phase 2.2: Map All Database Operations to Cypher

For each Database trait method, document:
1. **Cypher canonical form** (what the operation means)
2. **Translation rules** for SQLite and PostgreSQL
3. **Parameter mapping**
4. **Return value conversion**

---

### Phase 2.3: Test Translator Layer

For each operation type:
1. **Unit tests** verifying Cypher → SQL translation correctness
2. **Integration tests** verifying translated SQL executes correctly
3. **Semantic tests** verifying both forms return equivalent results

---

### Phase 2.4: Backend-Agnostic Database Trait

Optionally refactor Database trait to:
- Accept Cypher queries directly (advanced API)
- Keep domain-model API (existing API - unchanged)
- Both can coexist

---

## Translator Implementation Details

### SQLite Translator

**Key considerations**:
- No native vector distance functions → use custom NORM function
- FTS5 for full-text search
- LIKE for substring matching
- json1 extension for JSON fields

**Example translation**:
```cypher
MATCH (m:Memory)
WHERE m.embedding IS NOT NULL
WITH m, cosine_similarity($query_emb, m.embedding) AS score
```

↓

```sql
SELECT * FROM memories
WHERE embedding IS NOT NULL
ORDER BY cosine_similarity(embedding, $query_emb) DESC
```

(Requires custom cosine_similarity() function in SQLite)

---

### PostgreSQL Translator

**Key considerations**:
- pgvector for vector search
- Full-text search with tsvector/tsquery
- pg_trgm for fuzzy matching
- Built-in string functions

**Example translation**:
```cypher
MATCH (m:Memory)
WHERE m.embedding <-> $query_emb < 0.3
WITH m, 1 - (m.embedding <-> $query_emb) AS score
```

↓

```sql
SELECT * FROM memories
WHERE embedding <-> $query_emb < 0.3
ORDER BY (1 - (embedding <-> $query_emb)) DESC
```

---

### Neo4j Translator

**Key considerations**:
- Native Cypher support
- No translation needed (pass-through)
- Leverage Neo4j's built-in functions (apoc, etc.)

---

## Success Criteria for Phase 2

- [ ] 2.1: Query translation layer designed and documented
  - [ ] CypherOp enum defined for all operation types
  - [ ] QueryTranslator trait API designed
  - [ ] Documentation of all Cypher patterns

- [ ] 2.2: Cypher patterns identified for all major operations
  - [ ] Memory CRUD operations (7 operations)
  - [ ] Memory edges/links (3 operations)
  - [ ] Ontology concepts (6 operations)
  - [ ] Ontology edges (3 operations)
  - [ ] Hybrid search (1 complex operation)

- [ ] 2.3: Translation rules documented
  - [ ] SQLite translation rules (20 operation patterns)
  - [ ] PostgreSQL translation rules (20 operation patterns)
  - [ ] Neo4j pass-through rules (0 - direct Cypher)

- [ ] 2.4: Architecture supports backend independence
  - [ ] Trait design allows new backends to be added
  - [ ] Parameter mapping is consistent across backends
  - [ ] Return value conversion is well-defined

---

## Future Phases

**Phase 3**: Implement PostgreSQL backend using these patterns
**Phase 4**: Optional migration tools using translator patterns
**Phase 5**: Documentation using translator examples

---

## References

- Current Neo4j implementation: `crates/voidm-core/src/db/neo4j.rs`
- SQLite implementation: `crates/voidm-core/src/db/sqlite.rs`
- Database trait: `crates/voidm-core/src/db/mod.rs`
- Models: `crates/voidm-core/src/models.rs`

