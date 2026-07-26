use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct NamedMessageContext {
    pub id: super::super::Id,
    pub messages: Vec<super::Message>,
    pub created_at: super::super::Timestamp,
    pub updated_at: super::super::Timestamp,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<super::super::Metadata>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum BuiltinContextId {
    Current,
    System,
    Temp,
}
