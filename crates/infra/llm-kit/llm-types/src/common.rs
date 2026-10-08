use std::collections::HashMap;

pub type Id = String;
pub type Timestamp = i64;
pub type Metadata = HashMap<String, serde_json::Value>;
