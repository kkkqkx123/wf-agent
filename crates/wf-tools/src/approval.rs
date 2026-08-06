use std::collections::HashMap;

use wf_types::interaction::tool_approval::{ToolApprovalRequestData, ToolApprovalResponseData};
use wf_types::tool::approval::{
    CommandApprovalSettings, SecurityPreset, ToolApprovalOptions, ToolApprovalResult,
};
use wf_types::tool::file_permission::{
    is_operation_allowed, FileOperationType, FilePermissionSettings,
};
use wf_types::tool::mcp_approval::{
    McpApprovalSettings, McpDecision, McpDefaultBehavior, McpRequest, McpRequestType,
};
use wf_types::tool::ToolRiskLevel;

use wf_shell::command_safety::{CommandDecision, CommandPolicy};
use wf_shell::config::ShellToolConfig;

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
    protect_controller: Option<crate::protect::ProtectController>,
}

impl ToolApprovalCoordinator {
    pub fn new(options: ToolApprovalOptions) -> Self {
        Self {
            options,
            protect_controller: None,
        }
    }

    pub fn with_defaults() -> Self {
        Self {
            options: ToolApprovalOptions {
                auto_approval_enabled: Some(true),
                security_preset: Some(SecurityPreset::Balanced),
                risk_threshold: None,
                auto_approve_patterns: None,
                categories: None,
                workspace_boundary: None,
                file_permissions: None,
                command: None,
                mcp: None,
                network: None,
                interaction: None,
                allow_write_protected: None,
            },
            protect_controller: None,
        }
    }

    pub fn with_protect_controller(mut self, pc: crate::protect::ProtectController) -> Self {
        self.protect_controller = Some(pc);
        self
    }

    pub fn process_batch(&self, tools: Vec<ToolApprovalRequestData>) -> ToolBatch {
        let batch_id = wf_common::generate_id();
        let mut auto_approved = Vec::new();
        let mut pending = Vec::new();

        for (idx, tool) in tools.iter().enumerate() {
            match self.check_approval(tool) {
                ApprovalDecision::Approve => {
                    auto_approved.push(idx);
                }
                _ => {
                    pending.push(idx);
                }
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

    fn check_approval(&self, tool: &ToolApprovalRequestData) -> ApprovalDecision {
        if self.options.auto_approval_enabled != Some(true) {
            return ApprovalDecision::Ask;
        }

        if let Some(ref patterns) = self.options.auto_approve_patterns {
            if !patterns.is_empty() {
                let matched = patterns.iter().any(|p| {
                    if let Ok(glob) = globset::GlobBuilder::new(p).case_insensitive(true).build() {
                        glob.compile_matcher().is_match(&tool.tool_name)
                    } else {
                        false
                    }
                });
                if matched {
                    return ApprovalDecision::Approve;
                }
            }
        }

        if let Some(ref fp) = self.options.file_permissions {
            if let Ok(op) = extract_file_operation(&tool.tool_name, &tool.parameters) {
                if let Some(file_path) = extract_file_path(&tool.tool_name, &tool.parameters) {
                    if !check_file_permission(&file_path, &op, fp) {
                        return ApprovalDecision::Deny("File permission denied".to_string());
                    }
                }
            }
        }

        if let Some(ref pc) = self.protect_controller {
            if let Some(file_path) = extract_file_path(&tool.tool_name, &tool.parameters) {
                let op = extract_file_operation(&tool.tool_name, &tool.parameters)
                    .unwrap_or(FileOperationType::Read);
                if (op == FileOperationType::Write || op == FileOperationType::Delete)
                    && pc.is_write_protected(&file_path)
                    && self.options.allow_write_protected != Some(true)
                {
                    return ApprovalDecision::Deny("File is write-protected".to_string());
                }
            }
        }

        let risk_level = tool.risk_level.as_deref().unwrap_or("write");

        if risk_level == "system" {
            return ApprovalDecision::Ask;
        }

        match risk_level {
            "read_only" => {
                let cat = self.options.categories.as_ref();
                let allow = cat.and_then(|c| c.always_allow_read_only).unwrap_or(true);
                if allow {
                    ApprovalDecision::Approve
                } else {
                    ApprovalDecision::Ask
                }
            }
            "write" => {
                let cat_allow = self
                    .options
                    .categories
                    .as_ref()
                    .and_then(|c| c.always_allow_write)
                    .unwrap_or(false);
                if cat_allow || self.options.security_preset == Some(SecurityPreset::Permissive) {
                    ApprovalDecision::Approve
                } else {
                    ApprovalDecision::Ask
                }
            }
            "execute" => {
                let cat_allow = self
                    .options
                    .categories
                    .as_ref()
                    .and_then(|c| c.always_allow_execute)
                    .unwrap_or(false);
                if cat_allow {
                    handle_execute_command_approval(&self.options, tool)
                } else if self.options.security_preset == Some(SecurityPreset::Permissive) {
                    ApprovalDecision::Approve
                } else {
                    ApprovalDecision::Ask
                }
            }
            "mcp" => handle_mcp_approval(&self.options, tool),
            "network" => {
                if self
                    .options
                    .categories
                    .as_ref()
                    .and_then(|c| c.always_allow_network)
                    == Some(true)
                {
                    if let Some(domain) = extract_domain(tool) {
                        let denied = self
                            .options
                            .network
                            .as_ref()
                            .and_then(|n| n.denied_domains.as_ref())
                            .map(|d| d.iter().any(|d| domain.contains(d.as_str())))
                            .unwrap_or(false);
                        if denied {
                            return ApprovalDecision::Deny(format!(
                                "Domain {} is in denylist",
                                domain
                            ));
                        }
                        let allowed = self
                            .options
                            .network
                            .as_ref()
                            .and_then(|n| n.allowed_domains.as_ref());
                        if let Some(allowed) = allowed {
                            if !allowed.is_empty()
                                && !allowed.iter().any(|d| domain.contains(d.as_str()))
                            {
                                return ApprovalDecision::Ask;
                            }
                        }
                        ApprovalDecision::Approve
                    } else {
                        ApprovalDecision::Ask
                    }
                } else {
                    ApprovalDecision::Ask
                }
            }
            _ => {
                if matches!(
                    self.options.security_preset,
                    Some(SecurityPreset::Balanced) | Some(SecurityPreset::Permissive)
                ) {
                    ApprovalDecision::Approve
                } else {
                    ApprovalDecision::Ask
                }
            }
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

fn handle_execute_command_approval(
    options: &ToolApprovalOptions,
    tool: &ToolApprovalRequestData,
) -> ApprovalDecision {
    if options
        .categories
        .as_ref()
        .and_then(|c| c.always_allow_execute)
        != Some(true)
    {
        return ApprovalDecision::Ask;
    }

    let command = tool
        .parameters
        .get("command")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    // Reuse the engine-level CommandPolicy instead of re-implementing the
    // decision here: when the approval layer derives its command settings from
    // the same ShellToolConfig (see command_approval_settings_from_shell),
    // both layers always agree.
    let allowed = options
        .command
        .as_ref()
        .and_then(|c| c.allowed_commands.clone())
        .unwrap_or_default();
    let denied = options
        .command
        .as_ref()
        .and_then(|c| c.denied_commands.clone());
    let policy = CommandPolicy::new(allowed, denied);

    match policy.decision(command) {
        CommandDecision::AutoApprove => ApprovalDecision::Approve,
        CommandDecision::AutoDeny => {
            ApprovalDecision::Deny("Command is in denylist or not in allowlist".to_string())
        }
        CommandDecision::AskUser => ApprovalDecision::Ask,
    }
}

/// Derive the command approval settings for [`ToolApprovalOptions`] from a
/// [`ShellToolConfig`], so the approval layer and the engine baseline share
/// the same allow/deny lists (single config source, no double-source drift).
pub fn command_approval_settings_from_shell(config: &ShellToolConfig) -> CommandApprovalSettings {
    CommandApprovalSettings {
        allowed_commands: Some(config.allowed_commands.clone()),
        denied_commands: config.denied_commands.clone(),
    }
}

fn handle_mcp_approval(
    options: &ToolApprovalOptions,
    tool: &ToolApprovalRequestData,
) -> ApprovalDecision {
    if options.categories.as_ref().and_then(|c| c.always_allow_mcp) != Some(true) {
        return ApprovalDecision::Ask;
    }

    let Some(ref mcp_settings) = options.mcp else {
        return ApprovalDecision::Approve;
    };

    let Some(mcp_request) = build_mcp_request(tool) else {
        return ApprovalDecision::Ask;
    };

    match check_mcp_approval(mcp_settings, &mcp_request) {
        McpDecision::Approve => ApprovalDecision::Approve,
        McpDecision::Deny => ApprovalDecision::Deny("MCP request denied".to_string()),
        McpDecision::Ask => ApprovalDecision::Ask,
    }
}

fn build_mcp_request(tool: &ToolApprovalRequestData) -> Option<McpRequest> {
    let server_name = tool.parameters.get("server_name")?.as_str()?.to_string();
    let tool_name = tool.parameters.get("tool_name")?.as_str()?.to_string();
    let args = tool.parameters.get("arguments").cloned();

    Some(McpRequest {
        r#type: McpRequestType::UseMcp,
        server_name,
        tool_name: Some(tool_name),
        uri: None,
        arguments: args,
    })
}

pub fn check_mcp_approval(settings: &McpApprovalSettings, request: &McpRequest) -> McpDecision {
    let server_config = settings
        .servers
        .iter()
        .find(|s| s.name == request.server_name);

    let Some(server) = server_config else {
        match settings.default_server_behavior {
            Some(McpDefaultBehavior::AlwaysAsk) | None => return McpDecision::Ask,
            Some(McpDefaultBehavior::AlwaysDeny) => {
                return McpDecision::Deny;
            }
            Some(McpDefaultBehavior::AlwaysApprove) => return McpDecision::Approve,
        }
    };

    match request.r#type {
        McpRequestType::ListResources => McpDecision::Approve,
        McpRequestType::UseMcp => {
            let tool_name = match request.tool_name.as_ref() {
                Some(n) => n,
                None => return McpDecision::Ask,
            };

            let tool_config = server
                .tools
                .as_ref()
                .and_then(|tools| tools.iter().find(|t| t.name == *tool_name));

            match tool_config {
                Some(cfg) => {
                    if cfg.always_allow == Some(true) {
                        return McpDecision::Approve;
                    }
                    if cfg.risk_level.as_deref() == Some("READ_ONLY") {
                        return McpDecision::Approve;
                    }
                    McpDecision::Ask
                }
                None => match server.default_tool_behavior {
                    Some(McpDefaultBehavior::AlwaysApprove) => McpDecision::Approve,
                    Some(McpDefaultBehavior::AlwaysDeny) => McpDecision::Deny,
                    _ => McpDecision::Ask,
                },
            }
        }
        McpRequestType::ReadResource => {
            let uri = match request.uri.as_ref() {
                Some(u) => u,
                None => return McpDecision::Ask,
            };

            let resource_config = server.resources.as_ref().and_then(|resources| {
                resources
                    .iter()
                    .find(|r| match_uri_pattern(uri, &r.uri_pattern))
            });

            match resource_config {
                Some(cfg) => {
                    if cfg.always_allow == Some(true) {
                        McpDecision::Approve
                    } else {
                        McpDecision::Ask
                    }
                }
                None => match server.default_resource_behavior {
                    Some(McpDefaultBehavior::AlwaysApprove) => McpDecision::Approve,
                    _ => McpDecision::Ask,
                },
            }
        }
    }
}

fn match_uri_pattern(uri: &str, pattern: &str) -> bool {
    let escaped = regex::escape(pattern);
    let regex_str = format!("^{}$", escaped.replace(r"\*", ".*").replace(r"\?", "."));
    regex::Regex::new(&regex_str)
        .map(|re| re.is_match(uri))
        .unwrap_or(false)
}

fn extract_domain(tool: &ToolApprovalRequestData) -> Option<String> {
    tool.parameters
        .get("domain")
        .or_else(|| tool.parameters.get("url"))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
}

pub fn check_file_permission(
    file_path: &str,
    operation: &FileOperationType,
    settings: &FilePermissionSettings,
) -> bool {
    let normalized = file_path.replace('\\', "/");

    for rule in &settings.rules {
        let opts = globset::GlobBuilder::new(&rule.pattern)
            .case_insensitive(true)
            .literal_separator(true)
            .build();

        if let Ok(glob) = opts {
            if glob.compile_matcher().is_match(&normalized) {
                return is_operation_allowed(&rule.permission, operation);
            }
        }
    }

    let default = settings
        .default_permission
        .as_ref()
        .unwrap_or(&wf_types::tool::file_permission::FilePermissionLevel::Write);
    is_operation_allowed(default, operation)
}

fn extract_file_path(tool_name: &str, params: &serde_json::Value) -> Option<String> {
    let file_tools = [
        "read_file",
        "write_file",
        "edit",
        "apply_diff",
        "apply_patch",
        "create_file",
        "delete_file",
        "rename_file",
    ];
    if file_tools.contains(&tool_name) {
        params
            .get("path")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
    } else {
        None
    }
}

fn extract_file_operation(
    tool_name: &str,
    _params: &serde_json::Value,
) -> Result<FileOperationType, ()> {
    match tool_name {
        "read_file" => Ok(FileOperationType::Read),
        "write_file" | "edit" | "apply_diff" | "apply_patch" | "create_file" => {
            Ok(FileOperationType::Write)
        }
        "delete_file" => Ok(FileOperationType::Delete),
        _ => Err(()),
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum ApprovalDecision {
    Approve,
    Deny(String),
    Ask,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use wf_types::tool::approval::ApprovalCategories;

    fn make_request(id: &str, name: &str, risk: Option<&str>) -> ToolApprovalRequestData {
        ToolApprovalRequestData {
            tool_call_id: id.into(),
            tool_name: name.into(),
            tool_description: None,
            parameters: json!({}),
            risk_level: risk.map(String::from),
            pending_queue: None,
            batch_id: None,
            tool_index: None,
            total_tools: None,
            timeout: None,
            security_preset: None,
        }
    }

    fn make_execute_request(id: &str, command: &str) -> ToolApprovalRequestData {
        ToolApprovalRequestData {
            tool_call_id: id.into(),
            tool_name: "shell_execute".into(),
            tool_description: None,
            parameters: json!({ "command": command }),
            risk_level: Some("execute".into()),
            pending_queue: None,
            batch_id: None,
            tool_index: None,
            total_tools: None,
            timeout: None,
            security_preset: None,
        }
    }

    fn execute_approval_options(command: Option<CommandApprovalSettings>) -> ToolApprovalOptions {
        ToolApprovalOptions {
            auto_approval_enabled: Some(true),
            security_preset: Some(SecurityPreset::Balanced),
            risk_threshold: None,
            auto_approve_patterns: None,
            categories: Some(ApprovalCategories {
                always_allow_read_only: None,
                always_allow_write: None,
                always_allow_execute: Some(true),
                always_allow_mcp: None,
                always_allow_network: None,
                always_allow_interaction: None,
            }),
            workspace_boundary: None,
            file_permissions: None,
            command,
            mcp: None,
            network: None,
            interaction: None,
            allow_write_protected: None,
        }
    }

    #[test]
    fn test_engine_and_approval_command_policy_agree_from_same_source() {
        // One ShellToolConfig drives both the engine baseline (CommandPolicy)
        // and the approval layer (ToolApprovalOptions.command); their decisions
        // must agree for every command.
        let shell = ShellToolConfig {
            allowed_commands: vec!["git".into(), "echo".into()],
            denied_commands: Some(vec!["rm".into()]),
            ..Default::default()
        };
        let engine = CommandPolicy::from_config(&shell);
        let settings = command_approval_settings_from_shell(&shell);
        let approval = CommandPolicy::new(
            settings.allowed_commands.clone().unwrap_or_default(),
            settings.denied_commands.clone(),
        );

        for cmd in [
            "echo hi",
            "git status",
            "git checkout main && rm -rf /",
            "rm -rf /",
            "some unknown tool",
        ] {
            assert_eq!(
                engine.decision(cmd),
                approval.decision(cmd),
                "engine and approval policies diverged for: {}",
                cmd
            );
        }
    }

    #[test]
    fn test_execute_approval_reuses_engine_policy_from_same_source() {
        let shell = ShellToolConfig {
            allowed_commands: vec!["echo".into()],
            denied_commands: Some(vec!["rm".into()]),
            ..Default::default()
        };
        let coordinator = ToolApprovalCoordinator::new(execute_approval_options(Some(
            command_approval_settings_from_shell(&shell),
        )));

        let batch = coordinator.process_batch(vec![
            make_execute_request("t1", "echo hi"),
            make_execute_request("t2", "rm -rf /"),
            make_execute_request("t3", "some unknown tool"),
        ]);
        assert!(
            batch.auto_approved.contains(&0),
            "allowed command must be auto-approved"
        );
        assert!(
            batch.pending.contains(&1),
            "denied command must not be auto-approved"
        );
        assert!(
            batch.pending.contains(&2),
            "unknown command must ask the user"
        );
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
            categories: None,
            workspace_boundary: None,
            file_permissions: None,
            command: None,
            mcp: None,
            network: None,
            interaction: None,
            allow_write_protected: None,
        });

        let tools = vec![make_request("t1", "write_file", Some("write"))];
        let batch = coordinator.process_batch(tools);
        assert!(batch.pending.contains(&0));
    }

    #[test]
    fn test_auto_approve_patterns() {
        let coordinator = ToolApprovalCoordinator::new(ToolApprovalOptions {
            auto_approval_enabled: Some(true),
            security_preset: Some(SecurityPreset::Safe),
            risk_threshold: None,
            auto_approve_patterns: Some(vec!["read_*".to_string()]),
            categories: None,
            workspace_boundary: None,
            file_permissions: None,
            command: None,
            mcp: None,
            network: None,
            interaction: None,
            allow_write_protected: None,
        });

        let tools = vec![make_request("t1", "read_file", Some("read_only"))];
        let batch = coordinator.process_batch(tools);
        assert!(batch.auto_approved.contains(&0));
    }

    #[test]
    fn test_file_permission_deny() {
        let fp_settings = FilePermissionSettings {
            rules: vec![wf_types::tool::file_permission::FilePermissionRule {
                pattern: "**/secrets/**".to_string(),
                permission: wf_types::tool::file_permission::FilePermissionLevel::Denied,
                description: Some("Secrets".to_string()),
            }],
            default_permission: Some(wf_types::tool::file_permission::FilePermissionLevel::Write),
        };

        assert!(!check_file_permission(
            "/workspace/secrets/key.txt",
            &FileOperationType::Read,
            &fp_settings
        ));
        assert!(check_file_permission(
            "/workspace/src/main.rs",
            &FileOperationType::Read,
            &fp_settings
        ));
    }

    #[test]
    fn test_mcp_approval() {
        use wf_types::tool::mcp_approval::*;
        let settings = McpApprovalSettings {
            servers: vec![McpApprovalServerConfig {
                name: "my-server".to_string(),
                tools: Some(vec![McpApprovalToolConfig {
                    name: "safe-tool".to_string(),
                    always_allow: Some(true),
                    risk_level: None,
                }]),
                resources: None,
                default_tool_behavior: Some(McpDefaultBehavior::AlwaysAsk),
                default_resource_behavior: None,
            }],
            default_server_behavior: Some(McpDefaultBehavior::AlwaysAsk),
        };

        let approve_req = McpRequest {
            r#type: McpRequestType::UseMcp,
            server_name: "my-server".to_string(),
            tool_name: Some("safe-tool".to_string()),
            uri: None,
            arguments: None,
        };
        assert_eq!(
            check_mcp_approval(&settings, &approve_req),
            McpDecision::Approve
        );

        let ask_req = McpRequest {
            r#type: McpRequestType::UseMcp,
            server_name: "my-server".to_string(),
            tool_name: Some("unsafe-tool".to_string()),
            uri: None,
            arguments: None,
        };
        assert_eq!(check_mcp_approval(&settings, &ask_req), McpDecision::Ask);
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
