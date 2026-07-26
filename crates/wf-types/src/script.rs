use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Script {
    pub id: crate::Id,
    pub name: String,
    pub content: String,
    pub language: Option<String>,
    pub description: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ScriptExecutionOptions {
    pub timeout_seconds: Option<u64>,
    pub max_memory_mb: Option<u64>,
    pub environment: Option<std::collections::HashMap<String, String>>,
}

pub mod argument;
pub mod executor;
pub mod flow;
pub mod interactive;
pub mod sandbox;
pub mod security;

pub use argument::*;
pub use executor::*;
pub use flow::*;
pub use interactive::*;
pub use sandbox::*;
pub use security::*;
