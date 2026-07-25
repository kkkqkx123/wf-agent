#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Checkpoint {
    pub id: String,
    pub entity_id: String,
    pub entity_type: CheckpointEntityType,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    pub checkpoint_type: CheckpointType,
    pub state: serde_json::Value,
    pub created_at: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<HashMap<String, serde_json::Value>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_checkpoint_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub chain_root_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub chain_position: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base_checkpoint_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub previous_checkpoint_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub diff_data: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum CheckpointEntityType {
    #[serde(rename = "workflow")]
    Workflow,
    #[serde(rename = "agent")]
    Agent,
    #[serde(rename = "task")]
    Task,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum CheckpointType {
    #[serde(rename = "FULL")]
    Full,
    #[serde(rename = "DELTA")]
    Delta,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CheckpointStrategy {
    pub checkpoint_type: CheckpointType,
    #[serde(default)]
    pub create_before_execution: bool,
    #[serde(default)]
    pub create_after_execution: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_checkpoints: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ttl_seconds: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cleanup_policy: Option<CheckpointCleanupPolicy>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CheckpointCleanupPolicy {
    #[serde(default)]
    pub max_checkpoints_per_entity: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_age_seconds: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_total_size_bytes: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CheckpointMetadata {
    pub checkpoint_id: String,
    pub entity_id: String,
    pub entity_type: CheckpointEntityType,
    pub checkpoint_type: CheckpointType,
    pub created_at: i64,
    pub size_bytes: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_checkpoint_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub chain_position: Option<u32>,
}

// ============================================================================
// Variable State Snapshot
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct VariableStateSnapshot {
    pub entity_id: String,
    pub variables: HashMap<String, serde_json::Value>,
    pub created_at: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub execution_context: Option<serde_json::Value>,
}

// ============================================================================
// Agent Checkpoint
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AgentCheckpoint {
    pub id: String,
    pub agent_id: String,
    pub session_id: String,
    pub messages: Vec<Message>,
    pub state: serde_json::Value,
    pub created_at: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub iteration: Option<u32>,
}
