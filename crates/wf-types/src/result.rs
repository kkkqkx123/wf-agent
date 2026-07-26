use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "_tag")]
pub enum Result<T, E = serde_json::Value> {
    Ok(T),
    Err(E),
}
