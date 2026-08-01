use serde_json::Value;

use wf_types::interaction::tool_approval::ToolApprovalResponseData;

/// Request handed to an external tool approval handler. The interaction id
/// links the request to the asynchronous approval response channel.
#[derive(Debug, Clone)]
pub struct ToolApprovalRequest {
    pub tool_call_id: String,
    pub tool_name: String,
    pub arguments: Value,
    pub interaction_id: String,
    pub batch_id: Option<String>,
    pub tool_index: Option<u32>,
    pub total_tools: Option<u32>,
    pub pending_queue: Option<Vec<wf_types::interaction::tool_approval::PendingToolCallInfo>>,
}

#[derive(Debug, Clone)]
pub struct ToolApprovalResult {
    pub tool_call_id: String,
    pub approved: bool,
    pub edited_parameters: Option<Value>,
    pub user_instruction: Option<String>,
    pub rejection_reason: Option<String>,
}

impl ToolApprovalResult {
    pub fn approved(tool_call_id: impl Into<String>) -> Self {
        Self {
            tool_call_id: tool_call_id.into(),
            approved: true,
            edited_parameters: None,
            user_instruction: None,
            rejection_reason: None,
        }
    }

    pub fn rejected(tool_call_id: impl Into<String>, reason: impl Into<String>) -> Self {
        Self {
            tool_call_id: tool_call_id.into(),
            approved: false,
            edited_parameters: None,
            user_instruction: None,
            rejection_reason: Some(reason.into()),
        }
    }
}

impl From<ToolApprovalResponseData> for ToolApprovalResult {
    fn from(value: ToolApprovalResponseData) -> Self {
        Self {
            tool_call_id: String::new(),
            approved: value.approved,
            edited_parameters: value.edited_parameters,
            user_instruction: value.user_instruction,
            rejection_reason: value.rejection_reason,
        }
    }
}

/// External approval handler registered on the agent loop coordinator.
/// When absent, tools are auto-approved (TS behavior without a handler).
#[async_trait::async_trait]
pub trait ToolApprovalHandler: Send + Sync {
    async fn request_approval(&self, request: &ToolApprovalRequest) -> ToolApprovalResult;
}

/// Rejection message builder, mirrors TS rejection-message-builder.
#[derive(Debug, Clone)]
pub struct RejectionMessageBuilder {
    global_default_template: String,
    tool_specific_templates: std::collections::HashMap<String, String>,
    inject_user_message_hint: bool,
    user_message_hint_template: String,
}

impl Default for RejectionMessageBuilder {
    fn default() -> Self {
        Self {
            global_default_template: "Tool '{{toolId}}' is currently unavailable. {{reason}}"
                .to_string(),
            tool_specific_templates: std::collections::HashMap::new(),
            inject_user_message_hint: true,
            user_message_hint_template:
                "[Note: {{disabledTools}} are now disabled. {{enabledTools}} are now available.]"
                    .to_string(),
        }
    }
}

impl RejectionMessageBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_tool_template(
        mut self,
        tool_id: impl Into<String>,
        template: impl Into<String>,
    ) -> Self {
        self.tool_specific_templates
            .insert(tool_id.into(), template.into());
        self
    }

    pub fn with_global_template(mut self, template: impl Into<String>) -> Self {
        self.global_default_template = template.into();
        self
    }

    pub fn without_user_hint(mut self) -> Self {
        self.inject_user_message_hint = false;
        self
    }

    pub fn build_rejection_message(&self, tool_id: &str, reason: Option<&str>) -> String {
        let template = self
            .tool_specific_templates
            .get(tool_id)
            .map(String::as_str)
            .unwrap_or(&self.global_default_template);
        render_template(
            template,
            &[("toolId", tool_id), ("reason", reason.unwrap_or(""))],
        )
    }

    pub fn build_user_message_hint(
        &self,
        enabled_tools: &[String],
        disabled_tools: &[String],
    ) -> Option<String> {
        if !self.inject_user_message_hint || (enabled_tools.is_empty() && disabled_tools.is_empty())
        {
            return None;
        }
        Some(render_template(
            &self.user_message_hint_template,
            &[
                ("enabledTools", &enabled_tools.join(", ")),
                ("disabledTools", &disabled_tools.join(", ")),
            ],
        ))
    }
}

fn render_template(template: &str, vars: &[(&str, &str)]) -> String {
    let mut out = template.to_string();
    for (key, value) in vars {
        out = out.replace(&format!("{{{{{}}}}}", key), value);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_rejection_message() {
        let builder = RejectionMessageBuilder::new();
        assert_eq!(
            builder.build_rejection_message("write_file", Some("Not allowed")),
            "Tool 'write_file' is currently unavailable. Not allowed"
        );
        assert_eq!(
            builder.build_rejection_message("write_file", None),
            "Tool 'write_file' is currently unavailable. "
        );
    }

    #[test]
    fn test_tool_specific_template_wins() {
        let builder = RejectionMessageBuilder::new()
            .with_tool_template("write_file", "WRITE BLOCKED: {{reason}}");
        assert_eq!(
            builder.build_rejection_message("write_file", Some("readonly fs")),
            "WRITE BLOCKED: readonly fs"
        );
    }

    #[test]
    fn test_user_message_hint() {
        let builder = RejectionMessageBuilder::new();
        let hint = builder
            .build_user_message_hint(&["read_file".to_string()], &["write_file".to_string()])
            .unwrap();
        assert!(hint.contains("read_file"));
        assert!(hint.contains("write_file"));

        assert!(builder.build_user_message_hint(&[], &[]).is_none());

        let no_hint = RejectionMessageBuilder::new().without_user_hint();
        assert!(no_hint
            .build_user_message_hint(&["a".to_string()], &["b".to_string()])
            .is_none());
    }
}
