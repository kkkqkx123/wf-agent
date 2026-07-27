use std::collections::HashMap;

use wf_types::interaction::tool_approval::{
    ToolApprovalRequestData, ToolApprovalResponseData,
};
use wf_types::tool::approval::{SecurityPreset, ToolApprovalOptions, ToolApprovalResult};
use wf_types::tool::ToolRiskLevel;

#[derive(Debug, Clone)]
pub struct ToolBatch {
    pub batch_id: String,
    pub tools: Vec<ToolApprovalRequestData>,
    pub auto_approved: Vec<usize>,
    pub pending: Vec<usize>,
    pub results: HashMap<String, ToolApprovalResponseData>,
}

pub struct ToolApprovalCoordinator {
    options: ToolApprovalOptions,
}

impl ToolApprovalCoordinator {
    pub fn new(options: ToolApprovalOptions) -> Self {
        Self { options }
    }

    pub fn with_defaults() -> Self {
        Self {
            options: ToolApprovalOptions {
                auto_approval_enabled: Some(true),
                security_preset: Some(SecurityPreset::Balanced),
                risk_threshold: None,
                auto_approve_patterns: None,
            },
        }
    }

    pub fn process_batch(
        &self,
        tools: Vec<ToolApprovalRequestData>,
    ) -> ToolBatch {
        let batch_id = uuid::Uuid::new_v4().to_string();
        let mut auto_approved = Vec::new();
        let mut pending = Vec::new();

        for (idx, tool) in tools.iter().enumerate() {
            if self.should_auto_approve(tool) {
                auto_approved.push(idx);
            } else {
                pending.push(idx);
            }
        }

        ToolBatch {
            batch_id,
            tools,
            auto_approved,
            pending,
            results: HashMap::new(),
        }
    }

    pub fn record_response(
        &self,
        batch: &mut ToolBatch,
        tool_call_id: &str,
        response: ToolApprovalResponseData,
    ) {
        batch.results.insert(tool_call_id.to_string(), response);
    }

    pub fn is_batch_complete(&self, batch: &ToolBatch) -> bool {
        batch.results.len() == batch.pending.len()
    }

    pub fn get_approval_result(
        &self,
        batch: &ToolBatch,
        tool_call_id: &str,
    ) -> Option<ToolApprovalResult> {
        batch.results.get(tool_call_id).map(|r| ToolApprovalResult {
            approved: r.approved,
            tool_call_id: tool_call_id.to_string(),
            edited_parameters: r.edited_parameters.clone(),
            rejection_reason: r.rejection_reason.clone(),
        })
    }

    fn should_auto_approve(&self, tool: &ToolApprovalRequestData) -> bool {
        if self.options.auto_approval_enabled != Some(true) {
            return false;
        }

        if let Some(risk) = &tool.risk_level {
            match risk.as_str() {
                "read_only" => true,
                "write" | "execute" => self.options.security_preset == Some(SecurityPreset::Permissive),
                "network" | "system" | "interaction" => false,
                _ => false,
            }
        } else {
            matches!(
                self.options.security_preset,
                Some(SecurityPreset::Balanced) | Some(SecurityPreset::Permissive)
            )
        }
    }

    pub fn risk_level_from_str(level: &str) -> Option<ToolRiskLevel> {
        match level {
            "read_only" => Some(ToolRiskLevel::ReadOnly),
            "write" => Some(ToolRiskLevel::Write),
            "execute" => Some(ToolRiskLevel::Execute),
            "mcp" => Some(ToolRiskLevel::Mcp),
            "network" => Some(ToolRiskLevel::Network),
            "system" => Some(ToolRiskLevel::System),
            "interaction" => Some(ToolRiskLevel::Interaction),
            _ => None,
        }
    }
}

impl Default for ToolApprovalCoordinator {
    fn default() -> Self {
        Self::with_defaults()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    fn make_request(id: &str, name: &str, risk: Option<&str>) -> ToolApprovalRequestData {
        ToolApprovalRequestData {
            tool_call_id: id.into(),
            tool_name: name.into(),
            tool_description: None,
            parameters: Value::Null,
            risk_level: risk.map(String::from),
            pending_queue: None,
            batch_id: None,
            tool_index: None,
            total_tools: None,
            timeout: None,
            security_preset: None,
        }
    }

    #[test]
    fn test_auto_approve_read_only() {
        let coordinator = ToolApprovalCoordinator::with_defaults();
        let tools = vec![
            make_request("t1", "read_file", Some("read_only")),
            make_request("t2", "write_file", Some("write")),
        ];

        let batch = coordinator.process_batch(tools);
        assert!(batch.auto_approved.contains(&0));
        assert!(batch.pending.contains(&1));
    }

    #[test]
    fn test_safe_preset_blocks_write() {
        let coordinator = ToolApprovalCoordinator::new(ToolApprovalOptions {
            auto_approval_enabled: Some(true),
            security_preset: Some(SecurityPreset::Safe),
            risk_threshold: None,
            auto_approve_patterns: None,
        });

        let tools = vec![make_request("t1", "write_file", Some("write"))];
        let batch = coordinator.process_batch(tools);
        assert!(batch.pending.contains(&0));
        assert!(batch.auto_approved.is_empty());
    }

    #[test]
    fn test_batch_completion() {
        let coordinator = ToolApprovalCoordinator::with_defaults();
        let tools = vec![
            make_request("t1", "network_call", Some("network")),
            make_request("t2", "system_cmd", Some("system")),
        ];

        let mut batch = coordinator.process_batch(tools);
        assert!(!coordinator.is_batch_complete(&batch));

        coordinator.record_response(
            &mut batch,
            "t1",
            ToolApprovalResponseData {
                approved: true,
                edited_parameters: None,
                user_instruction: None,
                annotation: None,
                rejection_reason: None,
                continue_batch: None,
            },
        );
        assert!(!coordinator.is_batch_complete(&batch));

        coordinator.record_response(
            &mut batch,
            "t2",
            ToolApprovalResponseData {
                approved: false,
                edited_parameters: None,
                user_instruction: None,
                annotation: None,
                rejection_reason: Some("Too risky".into()),
                continue_batch: None,
            },
        );
        assert!(coordinator.is_batch_complete(&batch));
    }
}
