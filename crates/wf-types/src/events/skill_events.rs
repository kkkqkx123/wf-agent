use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SkillLoadStartedEvent {
    pub base: super::BaseEvent,
    pub skill_name: String,
    pub load_type: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SkillLoadCompletedEvent {
    pub base: super::BaseEvent,
    pub skill_name: String,
    pub load_type: String,
    pub success: bool,
    pub cached: bool,
    pub load_time: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SkillLoadFailedEvent {
    pub base: super::BaseEvent,
    pub skill_name: String,
    pub load_type: String,
    pub error: String,
    pub load_time: i64,
}
