#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Message {
    pub id: String,
    pub role: MessageRole,
    pub content: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<ChatToolCall>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<HashMap<String, serde_json::Value>>,
    pub created_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum MessageRole {
    #[serde(rename = "system")]
    System,
    #[serde(rename = "user")]
    User,
    #[serde(rename = "assistant")]
    Assistant,
    #[serde(rename = "tool")]
    Tool,
    #[serde(rename = "function")]
    Function,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct MessageContext {
    pub id: String,
    pub messages: Vec<Message>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<HashMap<String, serde_json::Value>>,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct NamedMessageContext {
    pub name: String,
    pub context: MessageContext,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct MessagePassingConfig {
    pub direction: MessagePassingDirection,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context_mappings: Option<Vec<ContextMapping>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum MessagePassingDirection {
    #[serde(rename = "BIDIRECTIONAL")]
    Bidirectional,
    #[serde(rename = "PARENT_TO_CHILD")]
    ParentToChild,
    #[serde(rename = "CHILD_TO_PARENT")]
    ChildToParent,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ContextMapping {
    pub source_context: String,
    pub target_context: String,
}

// ============================================================================
// LLM Message Extension
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct LlmMessage {
    pub id: String,
    pub role: MessageRole,
    pub content: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<ChatToolCall>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thinking: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<HashMap<String, serde_json::Value>>,
    pub created_at: i64,
}

// ============================================================================
// Message Operations
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "SCREAMING_SNAKE_CASE")]
pub enum MessageOperationConfig {
    Append(AppendMessageOperation),
    Insert(InsertMessageOperation),
    Update(UpdateMessageOperation),
    Delete(DeleteMessageOperation),
    Clear(ClearMessageOperation),
    Batch(BatchMessageOperation),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AppendMessageOperation {
    pub messages: Vec<Message>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct InsertMessageOperation {
    pub message: Message,
    pub position: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct UpdateMessageOperation {
    pub message_id: String,
    pub message: Message,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct DeleteMessageOperation {
    pub message_ids: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ClearMessageOperation {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context_id: Option<String>,
    #[serde(default)]
    pub keep_system_messages: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct BatchMessageOperation {
    pub operations: Vec<MessageOperationConfig>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context_id: Option<String>,
}
