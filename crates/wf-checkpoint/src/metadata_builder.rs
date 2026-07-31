use std::collections::HashMap;
use wf_types::checkpoint::base::{CheckpointMetadata, CheckpointStateBase};
use wf_types::Id;

#[derive(Debug, Clone)]
pub struct CheckpointMetadataBuilder {
    description: Option<String>,
    tags: Vec<String>,
    custom_fields: Option<HashMap<String, serde_json::Value>>,
}

impl CheckpointMetadataBuilder {
    pub fn new() -> Self {
        Self {
            description: None,
            tags: Vec::new(),
            custom_fields: None,
        }
    }

    pub fn description(mut self, desc: impl Into<String>) -> Self {
        self.description = Some(desc.into());
        self
    }

    pub fn tag(mut self, tag: impl Into<String>) -> Self {
        self.tags.push(tag.into());
        self
    }

    pub fn tags(mut self, tags: Vec<String>) -> Self {
        self.tags = tags;
        self
    }

    pub fn custom_field(mut self, key: impl Into<String>, value: serde_json::Value) -> Self {
        let map = self.custom_fields.get_or_insert_with(HashMap::new);
        map.insert(key.into(), value);
        self
    }

    pub fn build(self) -> CheckpointMetadata {
        CheckpointMetadata {
            description: self.description,
            tags: if self.tags.is_empty() {
                None
            } else {
                Some(self.tags)
            },
            custom_fields: self.custom_fields,
        }
    }
}

impl Default for CheckpointMetadataBuilder {
    fn default() -> Self {
        Self::new()
    }
}

pub fn build_checkpoint_state(
    id: Id,
    workflow_id: Option<Id>,
    execution_id: Option<Id>,
) -> CheckpointStateBase {
    CheckpointStateBase {
        id,
        workflow_id,
        execution_id,
        timestamp: wf_common::time::now(),
        format_version: "1.0".to_string(),
        status: Some("active".to_string()),
        start_time: None,
        end_time: None,
        error: None,
        hierarchy: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_metadata_builder() {
        let metadata = CheckpointMetadataBuilder::new()
            .description("test checkpoint")
            .tag("auto")
            .tag("node-1")
            .custom_field("node_id", serde_json::json!("node-1"))
            .build();

        assert_eq!(metadata.description, Some("test checkpoint".to_string()));
        assert_eq!(
            metadata.tags,
            Some(vec!["auto".to_string(), "node-1".to_string()])
        );
        assert!(metadata.custom_fields.is_some());
    }

    #[test]
    fn test_build_checkpoint_state() {
        let state = build_checkpoint_state(Id::new(), None, None);
        assert_eq!(state.format_version, "1.0");
        assert_eq!(state.status, Some("active".to_string()));
    }
}
