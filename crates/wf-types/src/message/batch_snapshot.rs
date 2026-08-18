use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BatchSnapshot {
    pub id: super::super::Id,
    pub messages: Vec<super::Message>,
    pub timestamp: super::super::Timestamp,
}
