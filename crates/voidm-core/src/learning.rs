use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeSet, HashMap, HashSet};
use strsim::jaro_winkler;

use crate::models::{AddMemoryRequest, AddMemoryResponse, EdgeType, Memory, MemoryType};
use crate::search::{SearchMode, SearchOptions};

pub const LEARNING_TIP_VERSION: u8 = 1;
pub const LEARNING_TRAJECTORY_VERSION: u8 = 1;
const LEARNING_TIP_KEY: &str = "learning_tip";
const LEARNING_CONSOLIDATION_KEY: &str = "learning_consolidation";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LearningTipCategory {
    Strategy,
    Recovery,
    Optimization,
}

impl std::fmt::Display for LearningTipCategory {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let value = match self {
            LearningTipCategory::Strategy => "strategy",
            LearningTipCategory::Recovery => "recovery",
            LearningTipCategory::Optimization => "optimization",
        };
        write!(f, "{value}")
    }
}

impl std::str::FromStr for LearningTipCategory {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_lowercase().as_str() {
            "strategy" => Ok(Self::Strategy),
            "recovery" => Ok(Self::Recovery),
            "optimization" => Ok(Self::Optimization),
            other => bail!(
                "Unknown learning category: '{}'. Valid categories: strategy, recovery, optimization",
                other
            ),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LearningSourceOutcome {
    Success,
    RecoveredFailure,
    Failure,
    Inefficient,
}

impl std::fmt::Display for LearningSourceOutcome {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let value = match self {
            LearningSourceOutcome::Success => "success",
            LearningSourceOutcome::RecoveredFailure => "recovered_failure",
            LearningSourceOutcome::Failure => "failure",
            LearningSourceOutcome::Inefficient => "inefficient",
        };
        write!(f, "{value}")
    }
}

impl std::str::FromStr for LearningSourceOutcome {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_lowercase().as_str() {
            "success" => Ok(Self::Success),
            "recovered_failure" | "recovered-failure" | "recovered" => Ok(Self::RecoveredFailure),
            "failure" => Ok(Self::Failure),
            "inefficient" => Ok(Self::Inefficient),
            other => bail!(
                "Unknown learning source outcome: '{}'. Valid outcomes: success, recovered_failure, failure, inefficient",
                other
            ),
        }
    }
}

impl Default for LearningSourceOutcome {
    fn default() -> Self {
        Self::Success
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LearningTip {
    pub version: u8,
    pub category: LearningTipCategory,
    pub trigger: String,
    pub application_context: String,
    pub task_category: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subtask: Option<String>,
    pub priority: u8,
    pub source_outcome: LearningSourceOutcome,
    pub source_trajectory_ids: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub negative_example: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_by: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum TrajectoryStepKind {
    Search,
    Inspect,
    Edit,
    Command,
    Test,
    Validation,
    Error,
    Recovery,
    Analysis,
    Optimization,
    #[default]
    Other,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TrajectoryStepOutcome {
    Success,
    Failure,
    Recovered,
    Inefficient,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct LearningTrajectoryStep {
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub kind: TrajectoryStepKind,
    #[serde(default)]
    pub outcome: Option<TrajectoryStepOutcome>,
    #[serde(default)]
    pub action: Option<String>,
    #[serde(default)]
    pub observation: Option<String>,
    #[serde(default)]
    pub error: Option<String>,
    #[serde(default)]
    pub resolution: Option<String>,
    #[serde(default)]
    pub why_useful: Option<String>,
    #[serde(default)]
    pub subtask: Option<String>,
    #[serde(default)]
    pub commands: Vec<String>,
    #[serde(default)]
    pub files: Vec<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub duration_ms: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LearningTrajectory {
    #[serde(default = "default_learning_trajectory_version")]
    pub version: u8,
    pub trajectory_id: String,
    pub task: String,
    #[serde(default)]
    pub task_category: Option<String>,
    #[serde(default)]
    pub application_context: Option<String>,
    #[serde(default)]
    pub subtask: Option<String>,
    #[serde(default)]
    pub summary: Option<String>,
    #[serde(default)]
    pub scopes: Vec<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub agent: Option<String>,
    #[serde(default)]
    pub outcome: LearningSourceOutcome,
    #[serde(default)]
    pub steps: Vec<LearningTrajectoryStep>,
}

#[derive(Debug, Clone, Serialize)]
pub struct TrajectoryLearningCandidate {
    pub trajectory_id: String,
    pub task: String,
    pub reason: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_step_index: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_step_title: Option<String>,
    pub content: String,
    pub memory_type: MemoryType,
    pub scopes: Vec<String>,
    pub tags: Vec<String>,
    pub learning_tip: LearningTip,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LearningConsolidationMetadata {
    pub cluster_size: usize,
    pub member_ids: Vec<String>,
    pub member_trajectory_ids: Vec<String>,
    pub similarity_score: f32,
}

#[derive(Debug, Clone)]
pub struct LearningConsolidationRequest {
    pub scope_filter: Option<String>,
    pub category: Option<LearningTipCategory>,
    pub task_category: Option<String>,
    pub threshold: f32,
    pub limit: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct LearningConsolidationCluster {
    pub similarity_score: f32,
    pub member_ids: Vec<String>,
    pub canonical_content: String,
    pub canonical_memory_type: MemoryType,
    pub canonical_importance: i64,
    pub canonical_scopes: Vec<String>,
    pub canonical_tags: Vec<String>,
    pub canonical_learning_tip: LearningTip,
    pub members: Vec<LearningMemoryRecord>,
}

#[derive(Debug, Clone, Serialize)]
pub struct LearningConsolidationApplied {
    pub canonical_memory: AddMemoryResponse,
    pub invalidated_member_ids: Vec<String>,
}

fn default_learning_trajectory_version() -> u8 {
    LEARNING_TRAJECTORY_VERSION
}

impl LearningTip {
    pub fn validate(&self) -> Result<()> {
        if self.version != LEARNING_TIP_VERSION {
            bail!(
                "Unsupported learning tip version '{}'. Expected {}",
                self.version,
                LEARNING_TIP_VERSION
            );
        }
        if self.trigger.trim().is_empty() {
            bail!("learning tip trigger must not be empty");
        }
        if self.application_context.trim().is_empty() {
            bail!("learning tip application_context must not be empty");
        }
        if self.task_category.trim().is_empty() {
            bail!("learning tip task_category must not be empty");
        }
        if self.priority == 0 || self.priority > 10 {
            bail!("learning tip priority must be between 1 and 10");
        }
        if self.source_trajectory_ids.is_empty() {
            bail!("learning tip must reference at least one source trajectory id");
        }
        if self
            .source_trajectory_ids
            .iter()
            .any(|trajectory| trajectory.trim().is_empty())
        {
            bail!("learning tip trajectory ids must not be empty");
        }
        Ok(())
    }
}

impl LearningTrajectoryStep {
    fn validate(&self) -> Result<()> {
        if non_empty(self.title.as_deref()).is_none()
            && non_empty(self.action.as_deref()).is_none()
            && non_empty(self.error.as_deref()).is_none()
            && non_empty(self.resolution.as_deref()).is_none()
            && non_empty(self.observation.as_deref()).is_none()
        {
            bail!("trajectory step must contain at least one descriptive field");
        }
        Ok(())
    }
}

impl LearningTrajectory {
    pub fn validate(&self) -> Result<()> {
        if self.version != LEARNING_TRAJECTORY_VERSION {
            bail!(
                "Unsupported learning trajectory version '{}'. Expected {}",
                self.version,
                LEARNING_TRAJECTORY_VERSION
            );
        }
        if self.trajectory_id.trim().is_empty() {
            bail!("trajectory_id must not be empty");
        }
        if self.task.trim().is_empty() {
            bail!("task must not be empty");
        }
        for step in &self.steps {
            step.validate()?;
        }
        Ok(())
    }

    pub fn resolved_application_context(&self) -> String {
        non_empty(self.application_context.as_deref())
            .map(normalize_text)
            .unwrap_or_else(|| "coding-agent repository workflow".to_string())
    }

    pub fn resolved_task_category(&self) -> String {
        non_empty(self.task_category.as_deref())
            .map(normalize_text)
            .unwrap_or_else(|| infer_task_category(&self.task, &self.steps))
    }

    fn normalized_scopes(&self) -> Vec<String> {
        dedupe_non_empty(
            self.scopes
                .iter()
                .filter_map(|scope| non_empty(Some(scope.as_str())).map(str::to_string))
                .collect(),
        )
    }
}

pub fn parse_learning_trajectories(input: &str) -> Result<Vec<LearningTrajectory>> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        bail!("trajectory input is empty");
    }

    if let Ok(trajectories) = serde_json::from_str::<Vec<LearningTrajectory>>(trimmed) {
        return validate_trajectories(trajectories);
    }

    if let Ok(trajectory) = serde_json::from_str::<LearningTrajectory>(trimmed) {
        return validate_trajectories(vec![trajectory]);
    }

    let mut trajectories = Vec::new();
    for (index, line) in input.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let trajectory = serde_json::from_str::<LearningTrajectory>(line).map_err(|err| {
            anyhow::anyhow!("Invalid trajectory JSON on line {}: {}", index + 1, err)
        })?;
        trajectories.push(trajectory);
    }

    if trajectories.is_empty() {
        bail!("trajectory input did not contain any JSON records");
    }

    validate_trajectories(trajectories)
}

pub fn extract_learning_candidates(
    trajectory: &LearningTrajectory,
) -> Result<Vec<TrajectoryLearningCandidate>> {
    trajectory.validate()?;

    let mut candidates = Vec::new();
    for (index, step) in trajectory.steps.iter().enumerate() {
        if let Some(candidate) = recovery_candidate(trajectory, step, index) {
            candidates.push(candidate);
        }
        if let Some(candidate) = optimization_candidate(trajectory, step, index) {
            candidates.push(candidate);
        }
        if let Some(candidate) = strategy_candidate(trajectory, step, index) {
            candidates.push(candidate);
        }
    }

    let mut seen = HashSet::new();
    candidates.retain(|candidate| {
        let key = format!(
            "{}::{}",
            candidate.learning_tip.category,
            candidate.content.to_lowercase()
        );
        seen.insert(key)
    });

    Ok(candidates)
}

#[derive(Debug, Clone, Default)]
pub struct LearningSearchFilters {
    pub category: Option<LearningTipCategory>,
    pub trigger: Option<String>,
    pub application_context: Option<String>,
    pub task_category: Option<String>,
    pub subtask: Option<String>,
    pub source_outcome: Option<LearningSourceOutcome>,
    pub trajectory_id: Option<String>,
    pub priority_min: Option<u8>,
}

#[derive(Debug, Clone)]
pub struct LearningSearchRequest {
    pub query: String,
    pub mode: SearchMode,
    pub limit: usize,
    pub scope_filter: Option<String>,
    pub min_score: Option<f32>,
    pub min_quality: Option<f32>,
    pub filters: LearningSearchFilters,
}

#[derive(Debug, Clone, Serialize)]
pub struct LearningMemoryRecord {
    pub memory: Memory,
    pub learning_tip: LearningTip,
}

#[derive(Debug, Clone, Serialize)]
pub struct LearningSearchResult {
    pub id: String,
    pub score: f32,
    #[serde(rename = "type")]
    pub memory_type: String,
    pub content: String,
    pub scopes: Vec<String>,
    pub tags: Vec<String>,
    pub importance: i64,
    pub created_at: String,
    pub source: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rel_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub direction: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hop_depth: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub quality_score: Option<f32>,
    pub learning_tip: LearningTip,
}

#[derive(Debug, Clone, Serialize)]
pub struct LearningSearchResponse {
    pub results: Vec<LearningSearchResult>,
    pub threshold_applied: Option<f32>,
    pub best_score: Option<f32>,
}

pub fn default_memory_type_for_learning(category: &LearningTipCategory) -> MemoryType {
    match category {
        LearningTipCategory::Strategy => MemoryType::Conceptual,
        LearningTipCategory::Recovery => MemoryType::Procedural,
        LearningTipCategory::Optimization => MemoryType::Procedural,
    }
}

pub fn attach_learning_tip_metadata(
    metadata: serde_json::Value,
    learning_tip: &LearningTip,
) -> Result<serde_json::Value> {
    learning_tip.validate()?;

    let mut metadata_map = match metadata {
        serde_json::Value::Object(map) => map,
        serde_json::Value::Null => serde_json::Map::new(),
        _ => bail!("memory metadata must be a JSON object when attaching a learning tip"),
    };

    metadata_map.insert(
        LEARNING_TIP_KEY.to_string(),
        serde_json::to_value(learning_tip)?,
    );

    Ok(serde_json::Value::Object(metadata_map))
}

pub fn attach_learning_consolidation_metadata(
    metadata: serde_json::Value,
    consolidation: &LearningConsolidationMetadata,
) -> Result<serde_json::Value> {
    let mut metadata_map = match metadata {
        serde_json::Value::Object(map) => map,
        serde_json::Value::Null => serde_json::Map::new(),
        _ => bail!("memory metadata must be a JSON object when attaching consolidation metadata"),
    };

    metadata_map.insert(
        LEARNING_CONSOLIDATION_KEY.to_string(),
        serde_json::to_value(consolidation)?,
    );

    Ok(serde_json::Value::Object(metadata_map))
}

pub fn extract_learning_tip(metadata: &serde_json::Value) -> Option<LearningTip> {
    let tip = metadata.get(LEARNING_TIP_KEY)?;
    serde_json::from_value::<LearningTip>(tip.clone()).ok()
}

pub fn extract_learning_consolidation(
    metadata: &serde_json::Value,
) -> Option<LearningConsolidationMetadata> {
    let consolidation = metadata.get(LEARNING_CONSOLIDATION_KEY)?;
    serde_json::from_value::<LearningConsolidationMetadata>(consolidation.clone()).ok()
}

pub fn memory_to_learning_record(memory: Memory) -> Option<LearningMemoryRecord> {
    let learning_tip = extract_learning_tip(&memory.metadata)?;
    Some(LearningMemoryRecord {
        memory,
        learning_tip,
    })
}

pub fn is_learning_tip(memory: &Memory) -> bool {
    extract_learning_tip(&memory.metadata).is_some()
}

pub fn learning_tip_matches(tip: &LearningTip, filters: &LearningSearchFilters) -> bool {
    if let Some(category) = &filters.category {
        if &tip.category != category {
            return false;
        }
    }
    if let Some(source_outcome) = &filters.source_outcome {
        if &tip.source_outcome != source_outcome {
            return false;
        }
    }
    if let Some(priority_min) = filters.priority_min {
        if tip.priority < priority_min {
            return false;
        }
    }
    if let Some(trigger) = &filters.trigger {
        if !contains_case_insensitive(&tip.trigger, trigger) {
            return false;
        }
    }
    if let Some(application_context) = &filters.application_context {
        if !contains_case_insensitive(&tip.application_context, application_context) {
            return false;
        }
    }
    if let Some(task_category) = &filters.task_category {
        if !contains_case_insensitive(&tip.task_category, task_category) {
            return false;
        }
    }
    if let Some(subtask) = &filters.subtask {
        let Some(existing_subtask) = &tip.subtask else {
            return false;
        };
        if !contains_case_insensitive(existing_subtask, subtask) {
            return false;
        }
    }
    if let Some(trajectory_id) = &filters.trajectory_id {
        if !tip
            .source_trajectory_ids
            .iter()
            .any(|candidate| contains_case_insensitive(candidate, trajectory_id))
        {
            return false;
        }
    }
    true
}

pub fn boosted_learning_score(base_score: f32, tip: &LearningTip) -> f32 {
    base_score + (tip.priority as f32 * 0.02)
}

pub async fn search_learning_tips(
    pool: &sqlx::SqlitePool,
    request: &LearningSearchRequest,
    model_name: &str,
    embeddings_enabled: bool,
    config_min_score: f32,
    config_search: &crate::config::SearchConfig,
) -> Result<LearningSearchResponse> {
    let overfetch = request.limit.saturating_mul(5).max(50);
    let opts = SearchOptions {
        query: request.query.clone(),
        mode: request.mode.clone(),
        limit: overfetch,
        scope_filter: request.scope_filter.clone(),
        type_filter: None,
        min_score: request.min_score,
        min_quality: request.min_quality,
        include_neighbors: false,
        neighbor_depth: None,
        neighbor_decay: None,
        neighbor_min_score: None,
        neighbor_limit: None,
        edge_types: None,
        intent: None,
    };

    let response = crate::search::search(
        pool,
        &opts,
        model_name,
        embeddings_enabled,
        config_min_score,
        config_search,
    )
    .await?;

    if response.best_score.is_none() {
        return Ok(LearningSearchResponse {
            results: Vec::new(),
            threshold_applied: response.threshold_applied,
            best_score: None,
        });
    }

    let mut results = Vec::new();
    for result in response.results {
        let Some(memory) = crate::crud::get_memory(pool, &result.id).await? else {
            continue;
        };
        let Some(learning_tip) = extract_learning_tip(&memory.metadata) else {
            continue;
        };
        if is_superseded_learning_tip(pool, &memory.id).await? {
            continue;
        }
        if !learning_tip_matches(&learning_tip, &request.filters) {
            continue;
        }

        results.push(LearningSearchResult {
            id: result.id,
            score: boosted_learning_score(result.score, &learning_tip),
            memory_type: result.memory_type,
            content: result.content,
            scopes: result.scopes,
            tags: result.tags,
            importance: result.importance,
            created_at: result.created_at,
            source: result.source,
            rel_type: result.rel_type,
            direction: result.direction,
            hop_depth: result.hop_depth,
            parent_id: result.parent_id,
            quality_score: result.quality_score,
            learning_tip,
        });
    }

    results.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    results.truncate(request.limit);

    let best_score = results.first().map(|result| result.score);
    Ok(LearningSearchResponse {
        results,
        threshold_applied: response.threshold_applied,
        best_score,
    })
}

pub async fn consolidate_learning_tips(
    pool: &sqlx::SqlitePool,
    request: &LearningConsolidationRequest,
) -> Result<Vec<LearningConsolidationCluster>> {
    if !(0.0..=1.0).contains(&request.threshold) {
        bail!("consolidation threshold must be between 0.0 and 1.0");
    }

    let records = load_active_learning_records(pool, request).await?;
    if records.len() < 2 {
        return Ok(Vec::new());
    }

    let mut parent: Vec<usize> = (0..records.len()).collect();
    let mut pair_scores = HashMap::new();

    for i in 0..records.len() {
        for j in (i + 1)..records.len() {
            let score = learning_consolidation_similarity(&records[i], &records[j]);
            if score >= request.threshold {
                union_indices(&mut parent, i, j);
                pair_scores.insert((i, j), score);
            }
        }
    }

    let mut groups: HashMap<usize, Vec<usize>> = HashMap::new();
    for index in 0..records.len() {
        let root = find_root(&mut parent, index);
        groups.entry(root).or_default().push(index);
    }

    let mut clusters = Vec::new();
    for indices in groups.into_values() {
        if indices.len() < 2 {
            continue;
        }
        clusters.push(build_consolidation_cluster(
            &records,
            &indices,
            &pair_scores,
        )?);
    }

    clusters.sort_by(|a, b| {
        b.members.len().cmp(&a.members.len()).then_with(|| {
            b.similarity_score
                .partial_cmp(&a.similarity_score)
                .unwrap_or(std::cmp::Ordering::Equal)
        })
    });

    Ok(clusters)
}

pub async fn apply_learning_consolidation(
    pool: &sqlx::SqlitePool,
    cluster: &LearningConsolidationCluster,
    config: &crate::Config,
) -> Result<LearningConsolidationApplied> {
    if cluster.members.len() < 2 {
        bail!("consolidation clusters must contain at least two members");
    }

    let consolidation_metadata = LearningConsolidationMetadata {
        cluster_size: cluster.members.len(),
        member_ids: cluster.member_ids.clone(),
        member_trajectory_ids: cluster.canonical_learning_tip.source_trajectory_ids.clone(),
        similarity_score: cluster.similarity_score,
    };

    let metadata = attach_learning_consolidation_metadata(
        attach_learning_tip_metadata(serde_json::json!({}), &cluster.canonical_learning_tip)?,
        &consolidation_metadata,
    )?;

    let request = AddMemoryRequest {
        id: None,
        content: cluster.canonical_content.clone(),
        memory_type: cluster.canonical_memory_type.clone(),
        scopes: cluster.canonical_scopes.clone(),
        tags: cluster.canonical_tags.clone(),
        importance: cluster.canonical_importance,
        metadata,
        links: vec![],
    };

    let canonical_memory = crate::crud::add_memory(pool, request, config).await?;
    let canonical_id = canonical_memory.id.clone();

    let mut invalidated_member_ids = Vec::new();
    for member in &cluster.members {
        crate::crud::link_memories(
            pool,
            &canonical_id,
            &EdgeType::DerivedFrom,
            &member.memory.id,
            None,
        )
        .await?;
        crate::crud::link_memories(
            pool,
            &canonical_id,
            &EdgeType::Invalidates,
            &member.memory.id,
            None,
        )
        .await?;
        invalidated_member_ids.push(member.memory.id.clone());
    }

    Ok(LearningConsolidationApplied {
        canonical_memory,
        invalidated_member_ids,
    })
}

fn contains_case_insensitive(haystack: &str, needle: &str) -> bool {
    haystack.to_lowercase().contains(&needle.to_lowercase())
}

async fn load_active_learning_records(
    pool: &sqlx::SqlitePool,
    request: &LearningConsolidationRequest,
) -> Result<Vec<LearningMemoryRecord>> {
    let memories = crate::crud::list_memories(
        pool,
        request.scope_filter.as_deref(),
        None,
        request.limit.max(2),
    )
    .await?;

    let mut records = Vec::new();
    for memory in memories {
        let Some(learning_tip) = extract_learning_tip(&memory.metadata) else {
            continue;
        };
        if is_superseded_learning_tip(pool, &memory.id).await? {
            continue;
        }
        if let Some(category) = &request.category {
            if &learning_tip.category != category {
                continue;
            }
        }
        if let Some(task_category) = &request.task_category {
            if !contains_case_insensitive(&learning_tip.task_category, task_category) {
                continue;
            }
        }
        records.push(LearningMemoryRecord {
            memory,
            learning_tip,
        });
    }

    Ok(records)
}

async fn is_superseded_learning_tip(pool: &sqlx::SqlitePool, memory_id: &str) -> Result<bool> {
    let exists: Option<i64> = sqlx::query_scalar(
        r#"
        SELECT 1
        FROM graph_edges ge
        JOIN graph_nodes target_node ON ge.target_id = target_node.id
        JOIN graph_nodes source_node ON ge.source_id = source_node.id
        JOIN memories source_memory ON source_memory.id = source_node.memory_id
        WHERE ge.rel_type = 'INVALIDATES'
          AND target_node.memory_id = ?
          AND json_extract(source_memory.metadata, '$.learning_tip.version') IS NOT NULL
        LIMIT 1
        "#,
    )
    .bind(memory_id)
    .fetch_optional(pool)
    .await?;

    Ok(exists.is_some())
}

fn learning_consolidation_similarity(
    left: &LearningMemoryRecord,
    right: &LearningMemoryRecord,
) -> f32 {
    if left.learning_tip.category != right.learning_tip.category {
        return 0.0;
    }

    let task_similarity = text_similarity(
        &left.learning_tip.task_category,
        &right.learning_tip.task_category,
    );
    if task_similarity < 0.75 {
        return 0.0;
    }

    let trigger_similarity =
        text_similarity(&left.learning_tip.trigger, &right.learning_tip.trigger);
    let content_similarity = text_similarity(&left.memory.content, &right.memory.content);
    let context_similarity = text_similarity(
        &left.learning_tip.application_context,
        &right.learning_tip.application_context,
    );
    let subtask_similarity = match (&left.learning_tip.subtask, &right.learning_tip.subtask) {
        (Some(left_subtask), Some(right_subtask)) => text_similarity(left_subtask, right_subtask),
        _ => 0.0,
    };

    let score = content_similarity * 0.45
        + trigger_similarity * 0.3
        + context_similarity * 0.15
        + task_similarity * 0.1
        + subtask_similarity * 0.05;

    score.min(1.0)
}

fn text_similarity(left: &str, right: &str) -> f32 {
    let left = normalize_similarity_text(left);
    let right = normalize_similarity_text(right);
    if left.is_empty() || right.is_empty() {
        return 0.0;
    }
    if left == right {
        return 1.0;
    }

    let jaro = jaro_winkler(&left, &right) as f32;
    let token_overlap = token_overlap_similarity(&left, &right);
    jaro.max(token_overlap)
}

fn normalize_similarity_text(value: &str) -> String {
    let normalized = value
        .chars()
        .map(|ch| if ch.is_alphanumeric() { ch } else { ' ' })
        .collect::<String>()
        .to_lowercase();
    normalize_text(&normalized)
}

fn token_overlap_similarity(left: &str, right: &str) -> f32 {
    let left_tokens: HashSet<&str> = left.split_whitespace().collect();
    let right_tokens: HashSet<&str> = right.split_whitespace().collect();
    if left_tokens.is_empty() || right_tokens.is_empty() {
        return 0.0;
    }

    let intersection = left_tokens.intersection(&right_tokens).count() as f32;
    let union = left_tokens.union(&right_tokens).count() as f32;
    if union == 0.0 {
        0.0
    } else {
        intersection / union
    }
}

fn build_consolidation_cluster(
    records: &[LearningMemoryRecord],
    indices: &[usize],
    pair_scores: &HashMap<(usize, usize), f32>,
) -> Result<LearningConsolidationCluster> {
    let mut members: Vec<LearningMemoryRecord> = indices
        .iter()
        .map(|index| records[*index].clone())
        .collect();
    members.sort_by(compare_records_for_canonical);

    let representative = members
        .first()
        .ok_or_else(|| anyhow::anyhow!("consolidation cluster is empty"))?;

    let mut similarity_total = 0.0f32;
    let mut similarity_count = 0usize;
    for (position, left_index) in indices.iter().enumerate() {
        for right_index in indices.iter().skip(position + 1) {
            let key = if left_index < right_index {
                (*left_index, *right_index)
            } else {
                (*right_index, *left_index)
            };
            if let Some(score) = pair_scores.get(&key) {
                similarity_total += *score;
                similarity_count += 1;
            }
        }
    }
    let similarity_score = if similarity_count == 0 {
        0.0
    } else {
        similarity_total / similarity_count as f32
    };

    let canonical_learning_tip = merge_cluster_learning_tip(&members);
    let canonical_content = merge_cluster_content(&members, &canonical_learning_tip);
    let canonical_memory_type = representative
        .memory
        .memory_type
        .parse::<MemoryType>()
        .unwrap_or_else(|_| default_memory_type_for_learning(&canonical_learning_tip.category));
    let canonical_importance = members
        .iter()
        .map(|member| member.memory.importance)
        .max()
        .unwrap_or(5);
    let canonical_scopes = dedupe_non_empty(
        members
            .iter()
            .flat_map(|member| member.memory.scopes.clone())
            .collect(),
    );
    let canonical_tags = dedupe_non_empty({
        let mut tags: Vec<String> = members
            .iter()
            .flat_map(|member| member.memory.tags.clone())
            .collect();
        tags.push("learning-tip".to_string());
        tags.push(canonical_learning_tip.category.to_string());
        tags.push("consolidated-learning".to_string());
        tags
    });
    let member_ids = members
        .iter()
        .map(|member| member.memory.id.clone())
        .collect::<Vec<_>>();

    Ok(LearningConsolidationCluster {
        similarity_score,
        member_ids,
        canonical_content,
        canonical_memory_type,
        canonical_importance,
        canonical_scopes,
        canonical_tags,
        canonical_learning_tip,
        members,
    })
}

fn merge_cluster_learning_tip(members: &[LearningMemoryRecord]) -> LearningTip {
    let representative = &members[0];
    let source_trajectory_ids = members
        .iter()
        .flat_map(|member| member.learning_tip.source_trajectory_ids.iter().cloned())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();

    let priority = members
        .iter()
        .map(|member| member.learning_tip.priority)
        .max()
        .unwrap_or(representative.learning_tip.priority);
    let source_outcome = dominant_source_outcome(members);
    let subtask = dominant_optional_text(
        members
            .iter()
            .filter_map(|member| member.learning_tip.subtask.as_deref())
            .collect(),
    )
    .or_else(|| representative.learning_tip.subtask.clone());
    let negative_example = dominant_optional_text(
        members
            .iter()
            .filter_map(|member| member.learning_tip.negative_example.as_deref())
            .collect(),
    )
    .or_else(|| representative.learning_tip.negative_example.clone());

    LearningTip {
        version: LEARNING_TIP_VERSION,
        category: representative.learning_tip.category.clone(),
        trigger: representative.learning_tip.trigger.clone(),
        application_context: representative.learning_tip.application_context.clone(),
        task_category: representative.learning_tip.task_category.clone(),
        subtask,
        priority,
        source_outcome,
        source_trajectory_ids,
        negative_example,
        created_by: Some("voidm.learn.consolidate".to_string()),
    }
}

fn merge_cluster_content(members: &[LearningMemoryRecord], tip: &LearningTip) -> String {
    let representative = &members[0].memory.content;
    let merged = ensure_sentence(representative);
    let trajectory_count = tip.source_trajectory_ids.len();
    if trajectory_count <= 1 {
        return merged;
    }

    format!("{merged} Observed across {trajectory_count} related trajectories.")
}

fn dominant_source_outcome(members: &[LearningMemoryRecord]) -> LearningSourceOutcome {
    let mut counts = HashMap::new();
    for member in members {
        *counts
            .entry(member.learning_tip.source_outcome.clone())
            .or_insert(0usize) += 1;
    }

    counts
        .into_iter()
        .max_by(|(left_outcome, left_count), (right_outcome, right_count)| {
            left_count.cmp(right_count).then_with(|| {
                source_outcome_rank(left_outcome).cmp(&source_outcome_rank(right_outcome))
            })
        })
        .map(|(outcome, _)| outcome)
        .unwrap_or(LearningSourceOutcome::Success)
}

fn source_outcome_rank(outcome: &LearningSourceOutcome) -> u8 {
    match outcome {
        LearningSourceOutcome::RecoveredFailure => 4,
        LearningSourceOutcome::Success => 3,
        LearningSourceOutcome::Inefficient => 2,
        LearningSourceOutcome::Failure => 1,
    }
}

fn dominant_optional_text(values: Vec<&str>) -> Option<String> {
    let mut counts = HashMap::new();
    for value in values {
        let normalized = normalize_text(value);
        if normalized.is_empty() {
            continue;
        }
        *counts.entry(normalized).or_insert(0usize) += 1;
    }

    counts
        .into_iter()
        .max_by(|(left_value, left_count), (right_value, right_count)| {
            left_count
                .cmp(right_count)
                .then_with(|| left_value.len().cmp(&right_value.len()))
        })
        .map(|(value, _)| value)
}

fn compare_records_for_canonical(
    left: &LearningMemoryRecord,
    right: &LearningMemoryRecord,
) -> std::cmp::Ordering {
    right
        .learning_tip
        .priority
        .cmp(&left.learning_tip.priority)
        .then_with(|| compare_quality_scores(right.memory.quality_score, left.memory.quality_score))
        .then_with(|| {
            right
                .learning_tip
                .source_trajectory_ids
                .len()
                .cmp(&left.learning_tip.source_trajectory_ids.len())
        })
        .then_with(|| right.memory.importance.cmp(&left.memory.importance))
        .then_with(|| right.memory.created_at.cmp(&left.memory.created_at))
}

fn compare_quality_scores(left: Option<f32>, right: Option<f32>) -> std::cmp::Ordering {
    left.unwrap_or_default()
        .partial_cmp(&right.unwrap_or_default())
        .unwrap_or(std::cmp::Ordering::Equal)
}

fn find_root(parent: &mut [usize], index: usize) -> usize {
    if parent[index] != index {
        let root = find_root(parent, parent[index]);
        parent[index] = root;
    }
    parent[index]
}

fn union_indices(parent: &mut [usize], left: usize, right: usize) {
    let left_root = find_root(parent, left);
    let right_root = find_root(parent, right);
    if left_root != right_root {
        parent[right_root] = left_root;
    }
}

fn validate_trajectories(trajectories: Vec<LearningTrajectory>) -> Result<Vec<LearningTrajectory>> {
    for trajectory in &trajectories {
        trajectory.validate()?;
    }
    Ok(trajectories)
}

fn recovery_candidate(
    trajectory: &LearningTrajectory,
    step: &LearningTrajectoryStep,
    source_step_index: usize,
) -> Option<TrajectoryLearningCandidate> {
    let error = non_empty(step.error.as_deref())?;
    let resolution = first_non_empty([
        step.resolution.as_deref(),
        step.why_useful.as_deref(),
        step.observation.as_deref(),
    ])?;

    let priority = match step.outcome.as_ref() {
        Some(TrajectoryStepOutcome::Recovered) => 8,
        Some(TrajectoryStepOutcome::Failure) => 7,
        _ => {
            if trajectory.outcome == LearningSourceOutcome::RecoveredFailure {
                8
            } else {
                7
            }
        }
    };

    Some(build_candidate(
        trajectory,
        step,
        source_step_index,
        LearningTipCategory::Recovery,
        "recovery_from_error",
        normalize_text(error),
        combine_tip_content(
            resolution,
            step.why_useful.as_deref(),
            step.observation.as_deref(),
        ),
        step.action.as_deref().map(normalize_text),
        priority,
    ))
}

fn optimization_candidate(
    trajectory: &LearningTrajectory,
    step: &LearningTrajectoryStep,
    source_step_index: usize,
) -> Option<TrajectoryLearningCandidate> {
    let is_optimization_step = matches!(step.kind, TrajectoryStepKind::Optimization)
        || matches!(
            step.outcome.as_ref(),
            Some(TrajectoryStepOutcome::Inefficient)
        )
        || trajectory.outcome == LearningSourceOutcome::Inefficient;
    if !is_optimization_step {
        return None;
    }

    let improvement = first_non_empty([
        step.resolution.as_deref(),
        step.observation.as_deref(),
        step.why_useful.as_deref(),
    ])?;
    let trigger = if let Some(error) = non_empty(step.error.as_deref()) {
        normalize_text(error)
    } else if let Some(action) = first_non_empty([step.action.as_deref(), step.title.as_deref()]) {
        format!(
            "When {} becomes inefficient",
            normalize_text(action).to_lowercase()
        )
    } else {
        "When the current approach is inefficient".to_string()
    };

    let priority = if step.duration_ms.unwrap_or_default() >= 60_000 {
        8
    } else {
        7
    };

    Some(build_candidate(
        trajectory,
        step,
        source_step_index,
        LearningTipCategory::Optimization,
        "optimization_from_inefficiency",
        trigger,
        combine_tip_content(improvement, step.why_useful.as_deref(), None),
        step.action.as_deref().map(normalize_text),
        priority,
    ))
}

fn strategy_candidate(
    trajectory: &LearningTrajectory,
    step: &LearningTrajectoryStep,
    source_step_index: usize,
) -> Option<TrajectoryLearningCandidate> {
    let is_successful = matches!(step.outcome.as_ref(), Some(TrajectoryStepOutcome::Success))
        || trajectory.outcome == LearningSourceOutcome::Success;
    if !is_successful {
        return None;
    }

    let detail = first_non_empty([step.why_useful.as_deref(), step.observation.as_deref()])?;
    let action = first_non_empty([step.action.as_deref(), step.title.as_deref()])?;
    let trigger = if let Some(subtask) =
        first_non_empty([step.subtask.as_deref(), trajectory.subtask.as_deref()])
    {
        format!("When working on {}", normalize_text(subtask))
    } else {
        format!("When working on {}", trajectory.resolved_task_category())
    };

    Some(build_candidate(
        trajectory,
        step,
        source_step_index,
        LearningTipCategory::Strategy,
        "strategy_from_successful_step",
        trigger,
        combine_tip_content(action, Some(detail), None),
        None,
        6,
    ))
}

fn build_candidate(
    trajectory: &LearningTrajectory,
    step: &LearningTrajectoryStep,
    source_step_index: usize,
    category: LearningTipCategory,
    reason: &str,
    trigger: String,
    content: String,
    negative_example: Option<String>,
    priority: u8,
) -> TrajectoryLearningCandidate {
    let task_category = trajectory.resolved_task_category();
    let subtask = first_non_empty([step.subtask.as_deref(), trajectory.subtask.as_deref()])
        .map(normalize_text);
    let source_step_title =
        first_non_empty([step.title.as_deref(), step.action.as_deref()]).map(normalize_text);
    let tags = dedupe_non_empty({
        let mut tags = trajectory.tags.clone();
        tags.extend(step.tags.iter().cloned());
        tags.push("learning-tip".to_string());
        tags.push(category.to_string());
        tags.push("trajectory-ingested".to_string());
        tags
    });

    TrajectoryLearningCandidate {
        trajectory_id: trajectory.trajectory_id.clone(),
        task: normalize_text(&trajectory.task),
        reason: reason.to_string(),
        source_step_index: Some(source_step_index),
        source_step_title,
        content,
        memory_type: default_memory_type_for_learning(&category),
        scopes: trajectory.normalized_scopes(),
        tags,
        learning_tip: LearningTip {
            version: LEARNING_TIP_VERSION,
            category,
            trigger,
            application_context: trajectory.resolved_application_context(),
            task_category,
            subtask,
            priority,
            source_outcome: trajectory.outcome.clone(),
            source_trajectory_ids: vec![trajectory.trajectory_id.clone()],
            negative_example,
            created_by: Some("voidm.learn.ingest".to_string()),
        },
    }
}

fn infer_task_category(task: &str, steps: &[LearningTrajectoryStep]) -> String {
    let mut corpus = normalize_text(task);
    for step in steps {
        if let Some(action) = non_empty(step.action.as_deref()) {
            corpus.push(' ');
            corpus.push_str(&action.to_lowercase());
        }
        if let Some(error) = non_empty(step.error.as_deref()) {
            corpus.push(' ');
            corpus.push_str(&error.to_lowercase());
        }
    }

    let keyword_groups = [
        (
            "authentication",
            &["auth", "oauth", "token", "login", "credential"][..],
        ),
        (
            "retrieval",
            &["search", "query", "rerank", "embedding", "retriev"][..],
        ),
        (
            "database",
            &["sqlite", "database", "schema", "migration", "sql"][..],
        ),
        (
            "testing",
            &["test", "assert", "fixture", "integration test"][..],
        ),
        (
            "build",
            &["build", "compile", "cargo", "dependency", "linker"][..],
        ),
        ("documentation", &["readme", "docs", "documentation"][..]),
        ("integration", &["api", "client", "server", "tool"][..]),
        (
            "parsing",
            &["parse", "json", "yaml", "toml", "deserialize"][..],
        ),
        (
            "deployment",
            &["deploy", "release", "rollout", "production"][..],
        ),
    ];

    for (category, keywords) in keyword_groups {
        if keywords.iter().any(|keyword| corpus.contains(keyword)) {
            return category.to_string();
        }
    }

    "general".to_string()
}

fn combine_tip_content(
    primary: &str,
    detail: Option<&str>,
    fallback_detail: Option<&str>,
) -> String {
    let mut content = ensure_sentence(primary);
    let detail = first_non_empty([detail, fallback_detail]);
    if let Some(detail) = detail {
        let detail = normalize_text(detail);
        let detail = trim_terminal_punctuation(&detail);
        let primary = normalize_text(primary);
        let primary = trim_terminal_punctuation(&primary);
        if !detail.eq_ignore_ascii_case(primary) {
            content.push_str(" Why it matters: ");
            content.push_str(detail);
            content.push('.');
        }
    }
    content
}

fn first_non_empty<const N: usize>(values: [Option<&str>; N]) -> Option<&str> {
    values
        .into_iter()
        .flatten()
        .find_map(|value| non_empty(Some(value)))
}

fn non_empty(value: Option<&str>) -> Option<&str> {
    value.and_then(|item| {
        if item.trim().is_empty() {
            None
        } else {
            Some(item.trim())
        }
    })
}

fn normalize_text(value: &str) -> String {
    value.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn ensure_sentence(value: &str) -> String {
    let normalized = normalize_text(value);
    if normalized.is_empty() {
        return normalized;
    }
    match normalized.chars().last() {
        Some('.') | Some('!') | Some('?') => normalized,
        _ => format!("{normalized}."),
    }
}

fn trim_terminal_punctuation(value: &str) -> &str {
    value.trim_end_matches(['.', '!', '?'])
}

fn dedupe_non_empty(values: Vec<String>) -> Vec<String> {
    let mut seen = HashSet::new();
    let mut deduped = Vec::new();
    for value in values {
        let normalized = normalize_text(&value);
        if normalized.is_empty() {
            continue;
        }
        let key = normalized.to_lowercase();
        if seen.insert(key) {
            deduped.push(normalized);
        }
    }
    deduped
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crud;
    use crate::db::sqlite::open_sqlite_pool;
    use crate::migrate;
    use crate::Config;

    fn sample_tip() -> LearningTip {
        LearningTip {
            version: LEARNING_TIP_VERSION,
            category: LearningTipCategory::Recovery,
            trigger: "When OAuth refresh returns a transient 401".to_string(),
            application_context: "OAuth2 token refresh flow".to_string(),
            task_category: "authentication".to_string(),
            subtask: Some("token refresh".to_string()),
            priority: 7,
            source_outcome: LearningSourceOutcome::RecoveredFailure,
            source_trajectory_ids: vec!["traj-123".to_string()],
            negative_example: Some(
                "Retrying without jitter caused another burst failure.".to_string(),
            ),
            created_by: Some("test".to_string()),
        }
    }

    #[test]
    fn learning_tip_validation_rejects_empty_trajectory_ids() {
        let mut tip = sample_tip();
        tip.source_trajectory_ids = vec![];
        assert!(tip.validate().is_err());
    }

    #[test]
    fn learning_tip_metadata_round_trip() {
        let tip = sample_tip();
        let metadata = attach_learning_tip_metadata(serde_json::json!({}), &tip).unwrap();
        let extracted = extract_learning_tip(&metadata).unwrap();

        assert_eq!(extracted.category, LearningTipCategory::Recovery);
        assert_eq!(
            extracted.source_outcome,
            LearningSourceOutcome::RecoveredFailure
        );
        assert_eq!(extracted.source_trajectory_ids, vec!["traj-123"]);
    }

    #[test]
    fn boosted_score_prefers_higher_priority() {
        let mut low = sample_tip();
        low.priority = 3;
        let mut high = sample_tip();
        high.priority = 9;

        assert!(boosted_learning_score(0.5, &high) > boosted_learning_score(0.5, &low));
    }

    #[test]
    fn learning_tip_filters_match_expected_fields() {
        let tip = sample_tip();
        let filters = LearningSearchFilters {
            category: Some(LearningTipCategory::Recovery),
            trigger: Some("transient 401".to_string()),
            application_context: Some("token refresh".to_string()),
            task_category: Some("authentication".to_string()),
            subtask: Some("refresh".to_string()),
            source_outcome: Some(LearningSourceOutcome::RecoveredFailure),
            trajectory_id: Some("traj-123".to_string()),
            priority_min: Some(6),
        };

        assert!(learning_tip_matches(&tip, &filters));

        let failing_filters = LearningSearchFilters {
            priority_min: Some(9),
            ..Default::default()
        };
        assert!(!learning_tip_matches(&tip, &failing_filters));
    }

    #[test]
    fn parse_learning_trajectories_supports_jsonl() {
        let input = r#"{"trajectory_id":"traj-1","task":"fix auth","outcome":"recovered_failure","steps":[{"kind":"recovery","error":"401 refresh failed","resolution":"Use jittered retries"}]}
{"trajectory_id":"traj-2","task":"update docs","steps":[{"kind":"edit","outcome":"success","action":"Read the existing docs first","why_useful":"it preserves the current structure"}]}"#;

        let trajectories = parse_learning_trajectories(input).unwrap();
        assert_eq!(trajectories.len(), 2);
        assert_eq!(trajectories[0].trajectory_id, "traj-1");
        assert_eq!(trajectories[1].outcome, LearningSourceOutcome::Success);
    }

    #[test]
    fn extraction_builds_recovery_and_strategy_candidates() {
        let trajectory = LearningTrajectory {
            version: LEARNING_TRAJECTORY_VERSION,
            trajectory_id: "traj-auth-1".to_string(),
            task: "Fix OAuth refresh failures".to_string(),
            task_category: Some("authentication".to_string()),
            application_context: Some("Rust OAuth refresh flow".to_string()),
            subtask: None,
            summary: None,
            scopes: vec!["voidm".to_string()],
            tags: vec!["repo:voidm".to_string()],
            agent: None,
            outcome: LearningSourceOutcome::RecoveredFailure,
            steps: vec![
                LearningTrajectoryStep {
                    kind: TrajectoryStepKind::Recovery,
                    outcome: Some(TrajectoryStepOutcome::Recovered),
                    error: Some("transient 401 during token refresh".to_string()),
                    resolution: Some(
                        "Use jittered retries before failing the refresh flow".to_string(),
                    ),
                    action: Some("retry immediately".to_string()),
                    subtask: Some("token refresh".to_string()),
                    ..Default::default()
                },
                LearningTrajectoryStep {
                    kind: TrajectoryStepKind::Inspect,
                    outcome: Some(TrajectoryStepOutcome::Success),
                    action: Some("Inspect the existing auth code before editing".to_string()),
                    why_useful: Some(
                        "it reveals the real failure path before writing a fix".to_string(),
                    ),
                    subtask: Some("auth debugging".to_string()),
                    ..Default::default()
                },
            ],
        };

        let candidates = extract_learning_candidates(&trajectory).unwrap();
        assert_eq!(candidates.len(), 2);
        assert_eq!(
            candidates[0].learning_tip.category,
            LearningTipCategory::Recovery
        );
        assert_eq!(
            candidates[0].learning_tip.source_outcome,
            LearningSourceOutcome::RecoveredFailure
        );
        assert_eq!(candidates[0].scopes, vec!["voidm"]);
        assert!(candidates[0]
            .tags
            .iter()
            .any(|tag| tag == "trajectory-ingested"));

        assert_eq!(
            candidates[1].learning_tip.category,
            LearningTipCategory::Strategy
        );
        assert!(candidates[1].content.contains("Why it matters"));
    }

    #[test]
    fn extraction_inferrs_general_task_category_when_missing() {
        let trajectory = LearningTrajectory {
            version: LEARNING_TRAJECTORY_VERSION,
            trajectory_id: "traj-search-1".to_string(),
            task: "Tune search reranking".to_string(),
            task_category: None,
            application_context: None,
            subtask: None,
            summary: None,
            scopes: vec![],
            tags: vec![],
            agent: None,
            outcome: LearningSourceOutcome::Inefficient,
            steps: vec![LearningTrajectoryStep {
                kind: TrajectoryStepKind::Optimization,
                outcome: Some(TrajectoryStepOutcome::Inefficient),
                action: Some("Rerank every candidate".to_string()),
                resolution: Some("Rerank only the top-k results after retrieval".to_string()),
                ..Default::default()
            }],
        };

        let candidates = extract_learning_candidates(&trajectory).unwrap();
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].learning_tip.task_category, "retrieval");
        assert_eq!(
            candidates[0].learning_tip.application_context,
            "coding-agent repository workflow"
        );
    }

    #[test]
    fn consolidation_similarity_groups_close_learning_tips() {
        let left = sample_learning_record(
            "left",
            "Use jittered retries before failing token refresh.",
            "When OAuth refresh returns a transient 401",
            "authentication",
            7,
            vec!["traj-1"],
        );
        let right = sample_learning_record(
            "right",
            "Use jittered retries before failing the token refresh flow.",
            "When OAuth refresh returns a transient 401 error",
            "authentication",
            8,
            vec!["traj-2"],
        );
        let distant = sample_learning_record(
            "distant",
            "Inspect migrations before rerunning database tests.",
            "When schema tests fail after storage changes",
            "database",
            6,
            vec!["traj-3"],
        );

        assert!(learning_consolidation_similarity(&left, &right) > 0.82);
        assert!(learning_consolidation_similarity(&left, &distant) < 0.75);
    }

    #[test]
    fn consolidation_cluster_merges_provenance() {
        let first = sample_learning_record(
            "first",
            "Use jittered retries before failing token refresh.",
            "When OAuth refresh returns a transient 401",
            "authentication",
            7,
            vec!["traj-1"],
        );
        let second = sample_learning_record(
            "second",
            "Use jittered retries before failing the token refresh flow.",
            "When OAuth refresh returns a transient 401 error",
            "authentication",
            9,
            vec!["traj-2", "traj-3"],
        );

        let pair_scores = HashMap::from([((0usize, 1usize), 0.91f32)]);
        let cluster = build_consolidation_cluster(&[first, second], &[0, 1], &pair_scores).unwrap();

        assert_eq!(cluster.members.len(), 2);
        assert_eq!(cluster.canonical_learning_tip.priority, 9);
        assert_eq!(
            cluster.canonical_learning_tip.source_trajectory_ids,
            vec![
                "traj-1".to_string(),
                "traj-2".to_string(),
                "traj-3".to_string()
            ]
        );
        assert!(cluster
            .canonical_tags
            .iter()
            .any(|tag| tag == "consolidated-learning"));
    }

    #[tokio::test]
    async fn applying_consolidation_invalidates_member_tips_in_search() -> Result<()> {
        let pool = open_sqlite_pool(":memory:").await?;
        migrate::run(&pool).await?;

        let mut config = Config::default();
        config.embeddings.enabled = false;

        add_test_learning_memory(
            &pool,
            &config,
            "Use jittered retries before failing token refresh.",
            "When OAuth refresh returns a transient 401",
            "traj-1",
        )
        .await?;
        add_test_learning_memory(
            &pool,
            &config,
            "Use jittered retries before failing the token refresh flow.",
            "When OAuth refresh returns a transient 401 error",
            "traj-2",
        )
        .await?;

        let clusters = consolidate_learning_tips(
            &pool,
            &LearningConsolidationRequest {
                scope_filter: Some("voidm".to_string()),
                category: Some(LearningTipCategory::Recovery),
                task_category: Some("authentication".to_string()),
                threshold: 0.82,
                limit: 50,
            },
        )
        .await?;
        assert_eq!(clusters.len(), 1);

        let applied = apply_learning_consolidation(&pool, &clusters[0], &config).await?;
        assert_eq!(applied.invalidated_member_ids.len(), 2);

        for member_id in &applied.invalidated_member_ids {
            assert!(is_superseded_learning_tip(&pool, member_id).await?);
        }

        let active_records = load_active_learning_records(
            &pool,
            &LearningConsolidationRequest {
                scope_filter: Some("voidm".to_string()),
                category: Some(LearningTipCategory::Recovery),
                task_category: Some("authentication".to_string()),
                threshold: 0.82,
                limit: 50,
            },
        )
        .await?;

        assert_eq!(active_records.len(), 1);
        assert_eq!(active_records[0].memory.id, applied.canonical_memory.id);
        assert_eq!(
            active_records[0].learning_tip.created_by.as_deref(),
            Some("voidm.learn.consolidate")
        );

        Ok(())
    }

    fn sample_learning_record(
        id: &str,
        content: &str,
        trigger: &str,
        task_category: &str,
        priority: u8,
        trajectory_ids: Vec<&str>,
    ) -> LearningMemoryRecord {
        let tip = LearningTip {
            version: LEARNING_TIP_VERSION,
            category: LearningTipCategory::Recovery,
            trigger: trigger.to_string(),
            application_context: "Rust OAuth refresh flow".to_string(),
            task_category: task_category.to_string(),
            subtask: Some("token refresh".to_string()),
            priority,
            source_outcome: LearningSourceOutcome::RecoveredFailure,
            source_trajectory_ids: trajectory_ids.into_iter().map(str::to_string).collect(),
            negative_example: None,
            created_by: Some("test".to_string()),
        };
        LearningMemoryRecord {
            memory: Memory {
                id: id.to_string(),
                memory_type: "procedural".to_string(),
                content: content.to_string(),
                importance: priority as i64,
                tags: vec!["learning-tip".to_string(), "recovery".to_string()],
                metadata: attach_learning_tip_metadata(serde_json::json!({}), &tip).unwrap(),
                scopes: vec!["voidm".to_string()],
                created_at: "2026-03-16T00:00:00Z".to_string(),
                updated_at: "2026-03-16T00:00:00Z".to_string(),
                quality_score: Some(0.9),
            },
            learning_tip: tip,
        }
    }

    async fn add_test_learning_memory(
        pool: &sqlx::SqlitePool,
        config: &Config,
        content: &str,
        trigger: &str,
        trajectory_id: &str,
    ) -> Result<()> {
        let tip = LearningTip {
            version: LEARNING_TIP_VERSION,
            category: LearningTipCategory::Recovery,
            trigger: trigger.to_string(),
            application_context: "Rust OAuth refresh flow".to_string(),
            task_category: "authentication".to_string(),
            subtask: Some("token refresh".to_string()),
            priority: 7,
            source_outcome: LearningSourceOutcome::RecoveredFailure,
            source_trajectory_ids: vec![trajectory_id.to_string()],
            negative_example: None,
            created_by: Some("test".to_string()),
        };

        let metadata = attach_learning_tip_metadata(serde_json::json!({}), &tip)?;
        crud::add_memory(
            pool,
            AddMemoryRequest {
                id: None,
                content: content.to_string(),
                memory_type: MemoryType::Procedural,
                scopes: vec!["voidm".to_string()],
                tags: vec!["learning-tip".to_string(), "recovery".to_string()],
                importance: 7,
                metadata,
                links: vec![],
            },
            config,
        )
        .await?;

        Ok(())
    }
}
