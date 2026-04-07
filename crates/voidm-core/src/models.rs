use serde::{Deserialize, Serialize};
use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, sqlx::Type)]
#[sqlx(rename_all = "lowercase")]
#[serde(rename_all = "lowercase")]
pub enum MemoryType {
    Episodic,
    Semantic,
    Procedural,
    Conceptual,
    Contextual,
}

impl fmt::Display for MemoryType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            MemoryType::Episodic => "episodic",
            MemoryType::Semantic => "semantic",
            MemoryType::Procedural => "procedural",
            MemoryType::Conceptual => "conceptual",
            MemoryType::Contextual => "contextual",
        };
        write!(f, "{}", s)
    }
}

impl std::str::FromStr for MemoryType {
    type Err = anyhow::Error;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "episodic" => Ok(MemoryType::Episodic),
            "semantic" => Ok(MemoryType::Semantic),
            "procedural" => Ok(MemoryType::Procedural),
            "conceptual" => Ok(MemoryType::Conceptual),
            "contextual" => Ok(MemoryType::Contextual),
            other => Err(anyhow::anyhow!(
                "Unknown memory type: '{}'. Valid types: episodic, semantic, procedural, conceptual, contextual",
                other
            )),
        }
    }
}

/// Valid graph edge relationship types.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum EdgeType {
    RelatesTo,
    Supports,
    Contradicts,
    DerivedFrom,
    Precedes,
    PartOf,
    Exemplifies,
    Invalidates,
    // Ontology edges (also valid in ontology_edges table)
    IsA,
    InstanceOf,
    HasProperty,
}

impl EdgeType {
    pub fn as_str(&self) -> &'static str {
        match self {
            EdgeType::RelatesTo => "RELATES_TO",
            EdgeType::Supports => "SUPPORTS",
            EdgeType::Contradicts => "CONTRADICTS",
            EdgeType::DerivedFrom => "DERIVED_FROM",
            EdgeType::Precedes => "PRECEDES",
            EdgeType::PartOf => "PART_OF",
            EdgeType::Exemplifies => "EXEMPLIFIES",
            EdgeType::Invalidates => "INVALIDATES",
            EdgeType::IsA => "IS_A",
            EdgeType::InstanceOf => "INSTANCE_OF",
            EdgeType::HasProperty => "HAS_PROPERTY",
        }
    }

    /// Returns the conflicting edge type if one exists (SUPPORTS↔CONTRADICTS, PRECEDES↔INVALIDATES)
    pub fn conflict(&self) -> Option<&'static str> {
        match self {
            EdgeType::Supports => Some("CONTRADICTS"),
            EdgeType::Contradicts => Some("SUPPORTS"),
            EdgeType::Precedes => Some("INVALIDATES"),
            EdgeType::Invalidates => Some("PRECEDES"),
            _ => None,
        }
    }

    /// Whether this edge requires a note (RELATES_TO)
    pub fn requires_note(&self) -> bool {
        matches!(self, EdgeType::RelatesTo)
    }
}

impl fmt::Display for EdgeType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl std::str::FromStr for EdgeType {
    type Err = anyhow::Error;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_uppercase().replace('-', "_").as_str() {
            "RELATES_TO" => Ok(EdgeType::RelatesTo),
            "SUPPORTS" => Ok(EdgeType::Supports),
            "CONTRADICTS" => Ok(EdgeType::Contradicts),
            "DERIVED_FROM" => Ok(EdgeType::DerivedFrom),
            "PRECEDES" => Ok(EdgeType::Precedes),
            "PART_OF" => Ok(EdgeType::PartOf),
            "EXEMPLIFIES" => Ok(EdgeType::Exemplifies),
            "INVALIDATES" => Ok(EdgeType::Invalidates),
            "IS_A" | "ISA" => Ok(EdgeType::IsA),
            "INSTANCE_OF" => Ok(EdgeType::InstanceOf),
            "HAS_PROPERTY" => Ok(EdgeType::HasProperty),
            other => Err(anyhow::anyhow!(
                "Unknown edge type: '{}'. Valid types: RELATES_TO, SUPPORTS, CONTRADICTS, DERIVED_FROM, PRECEDES, PART_OF, EXEMPLIFIES, INVALIDATES, IS_A, INSTANCE_OF, HAS_PROPERTY",
                other
            )),
        }
    }
}

/// A memory record as stored in the DB.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Memory {
    pub id: String,
    #[serde(rename = "type")]
    pub memory_type: String,
    pub content: String,
    pub importance: i64,
    pub tags: Vec<String>,
    pub metadata: serde_json::Value,
    pub scopes: Vec<String>,
    pub created_at: String,
    pub updated_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub quality_score: Option<f32>,
    /// Short label for fast lexical retrieval and display (max 200 chars).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// Semantic label: gotcha | decision | procedure | reference.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context: Option<String>,
}

/// Request to add a memory.
#[derive(Debug, Clone)]
pub struct AddMemoryRequest {
    pub id: Option<String>,
    pub content: String,
    pub memory_type: MemoryType,
    pub scopes: Vec<String>,
    pub tags: Vec<String>,
    pub importance: i64,
    pub metadata: serde_json::Value,
    pub links: Vec<LinkSpec>,
    pub title: Option<String>,
    pub context: Option<String>,
}

/// A link spec from --link id:TYPE or --link id:RELATES_TO:"note"
#[derive(Debug, Clone)]
pub struct LinkSpec {
    pub target_id: String,
    pub edge_type: EdgeType,
    pub note: Option<String>,
}

/// Response from voidm add.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AddMemoryResponse {
    pub id: String,
    #[serde(rename = "type")]
    pub memory_type: String,
    pub content: String,
    pub scopes: Vec<String>,
    pub tags: Vec<String>,
    pub importance: i64,
    pub created_at: String,
    pub quality_score: Option<f32>,
    pub suggested_links: Vec<SuggestedLink>,
    pub duplicate_warning: Option<DuplicateWarning>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SuggestedLink {
    pub id: String,
    pub score: f32,
    #[serde(rename = "type")]
    pub memory_type: String,
    pub content: String, // truncated at 120 chars
    pub hint: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DuplicateWarning {
    pub id: String,
    pub score: f32,
    pub content: String,
    pub message: String,
}

/// A graph edge record.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Edge {
    pub from_id: String,
    pub rel_type: String,
    pub to_id: String,
    pub note: Option<String>,
    pub created_at: String,
}

/// Return type for link command.
/// Representation of a link/edge between two memories for migration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryEdge {
    pub from_id: String,
    pub to_id: String,
    pub rel_type: String,
    pub note: Option<String>,
}

/// Representation of an ontology edge (concept-concept, concept-memory, etc.) for migration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OntologyEdgeForMigration {
    pub from_id: String,
    pub from_type: String, // "memory" or "concept"
    pub to_id: String,
    pub to_type: String, // "memory" or "concept"
    pub rel_type: String,
    pub note: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LinkResponse {
    pub created: bool,
    pub from: String,
    pub rel: String,
    pub to: String,
    pub conflict_warning: Option<ConflictWarning>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConflictWarning {
    pub existing_rel: String,
    pub message: String,
}

/// Hint logic for suggested links based on type pairs.
pub fn edge_hint(new_type: &str, existing_type: &str) -> &'static str {
    match (new_type, existing_type) {
        ("episodic", "episodic") => "PRECEDES, RELATES_TO",
        ("episodic", "semantic") => "SUPPORTS, CONTRADICTS",
        ("episodic", "procedural") => "DERIVED_FROM, RELATES_TO",
        ("semantic", "semantic") => "SUPPORTS, CONTRADICTS, DERIVED_FROM",
        ("semantic", "conceptual") => "SUPPORTS, EXEMPLIFIES",
        ("conceptual", "conceptual") => "SUPPORTS, CONTRADICTS, DERIVED_FROM",
        ("conceptual", "semantic") => "DERIVED_FROM, SUPPORTS",
        ("procedural", "procedural") => "INVALIDATES, PART_OF",
        ("procedural", "episodic") => "DERIVED_FROM",
        ("contextual", "contextual") => "RELATES_TO (with note), PART_OF",
        ("contextual", "semantic") => "EXEMPLIFIES, RELATES_TO",
        _ => "RELATES_TO (with note required)",
    }
}

// ── Batch merge operations ─────────────────────────────────────────────────

/// Machine-readable merge plan: list of (source_id, target_id) pairs
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MergePlan {
    pub merges: Vec<MergePair>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MergePair {
    pub source: String,
    pub target: String,
}

/// Merge log entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MergeLogEntry {
    pub id: String,
    pub batch_id: String,
    pub source_id: String,
    pub target_id: String,
    pub edges_retargeted: i32,
    pub conflicts_kept: i32,
    pub status: String,
    pub reason: Option<String>,
    pub created_at: String,
    pub completed_at: Option<String>,
}
