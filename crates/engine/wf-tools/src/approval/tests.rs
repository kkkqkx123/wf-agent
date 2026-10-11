//! Approval tests grouped by policy area: coordinator/batching, file
//! permissions, MCP decisions and network domain matching.

use super::*;
use serde_json::json;
use wf_types::tool::approval::ApprovalCategories;
use wf_types::tool::file_permission::{FileOperationType, FilePermissionSettings};

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
    }
}

fn execute_approval_options(command: Option<CommandApprovalSettings>) -> ToolApprovalOptions {
    ToolApprovalOptions {
        auto_approval_enabled: Some(true),
        security_preset: Some(SecurityPreset::Balanced),
        auto_approve_patterns: None,
        categories: Some(ApprovalCategories {
            always_allow_read_only: None,
            always_allow_write: None,
            always_allow_execute: Some(true),
            always_allow_mcp: None,
            always_allow_network: None,
            always_allow_interaction: None,
        }),
        file_permissions: None,
        command,
        mcp: None,
        network: None,
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
        auto_approve_patterns: None,
        categories: None,
        file_permissions: None,
        command: None,
        mcp: None,
        network: None,
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
        auto_approve_patterns: Some(vec!["read_*".to_string()]),
        categories: None,
        file_permissions: None,
        command: None,
        mcp: None,
        network: None,
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
fn test_default_ruleset_protects_sensitive_files() {
    // Ported default ruleset must deny sensitive files and allow source code.
    let fp = FilePermissionSettings::default_rules();

    // Sensitive files are denied for both read and write.
    assert!(!check_file_permission(
        "/workspace/app/.env",
        &FileOperationType::Read,
        &fp
    ));
    assert!(!check_file_permission(
        "/workspace/app/.env.production",
        &FileOperationType::Read,
        &fp
    ));
    assert!(!check_file_permission(
        "/workspace/secrets/key.txt",
        &FileOperationType::Read,
        &fp
    ));
    assert!(!check_file_permission(
        "/workspace/keys/server.pem",
        &FileOperationType::Write,
        &fp
    ));
    assert!(!check_file_permission(
        "/workspace/keys/api.key",
        &FileOperationType::Read,
        &fp
    ));
    assert!(!check_file_permission(
        "/workspace/credentials.json",
        &FileOperationType::Read,
        &fp
    ));

    // Important config files are read-only.
    assert!(check_file_permission(
        "/workspace/package.json",
        &FileOperationType::Read,
        &fp
    ));
    assert!(!check_file_permission(
        "/workspace/package.json",
        &FileOperationType::Write,
        &fp
    ));
    assert!(check_file_permission(
        "/workspace/.git/config",
        &FileOperationType::Read,
        &fp
    ));

    // Everything else defaults to write access.
    assert!(check_file_permission(
        "/workspace/src/main.rs",
        &FileOperationType::Write,
        &fp
    ));
}

#[test]
fn test_with_defaults_injects_default_ruleset() {
    // The default coordinator must now ship with the sensitive-file
    // ruleset so secrets are protected even when the caller supplies no
    // explicit file permission configuration.
    let coordinator = ToolApprovalCoordinator::with_defaults();
    let fp = coordinator
        .options
        .file_permissions
        .as_ref()
        .expect("with_defaults must provide a default file permissions ruleset");

    assert!(!check_file_permission(
        "/workspace/app/.env",
        &FileOperationType::Read,
        fp
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

#[test]
fn test_file_permission_violation_requires_approval_not_deny() {
    // Editing a sensitive file (e.g. .env) is legitimate but needs sign-off,
    // so it must surface as `Ask` (routable to a human reviewer) rather than
    // `Deny` (which would hard-block even intentional, reviewed changes).
    let coordinator = ToolApprovalCoordinator::with_defaults();
    let mut req = make_request("t1", "write_file", Some("write"));
    req.parameters = json!({ "path": "/workspace/app/.env" });
    assert_eq!(
        coordinator.check_approval(&req, None, None),
        ApprovalDecision::Ask
    );
}

#[test]
fn test_write_protected_file_is_still_hard_denied() {
    // Truly write-protected files (guarded by the protect_controller) must
    // remain hard-denied and never be surfaced as a mere approval request.
    let pc = crate::protect::ProtectController::new(None);
    let coordinator = ToolApprovalCoordinator::new(ToolApprovalOptions {
        auto_approval_enabled: Some(true),
        security_preset: Some(SecurityPreset::Permissive),
        auto_approve_patterns: None,
        categories: None,
        file_permissions: None,
        command: None,
        mcp: None,
        network: None,
        allow_write_protected: None,
    })
    .with_protect_controller(pc);
    let mut req = make_request("t1", "write_file", Some("write"));
    req.parameters = json!({ "path": "/workspace/app/.env" });
    assert_eq!(
        coordinator.check_approval(&req, None, None),
        ApprovalDecision::Deny("File is write-protected".to_string())
    );
}

fn mcp_options() -> ToolApprovalOptions {
    use wf_types::tool::mcp_approval::*;
    ToolApprovalOptions {
        auto_approval_enabled: Some(true),
        security_preset: Some(SecurityPreset::Balanced),
        auto_approve_patterns: None,
        categories: Some(ApprovalCategories {
            always_allow_read_only: None,
            always_allow_write: None,
            always_allow_execute: None,
            always_allow_mcp: Some(true),
            always_allow_network: None,
            always_allow_interaction: None,
        }),
        file_permissions: None,
        command: None,
        mcp: Some(McpApprovalSettings {
            servers: vec![McpApprovalServerConfig {
                name: "db".to_string(),
                tools: Some(vec![McpApprovalToolConfig {
                    name: "query".to_string(),
                    always_allow: Some(true),
                    risk_level: None,
                }]),
                resources: None,
                default_tool_behavior: Some(McpDefaultBehavior::AlwaysAsk),
                default_resource_behavior: None,
            }],
            default_server_behavior: Some(McpDefaultBehavior::AlwaysAsk),
        }),
        network: None,
        allow_write_protected: None,
    }
}

fn mcp_tool_entry() -> wf_types::tool::Tool {
    wf_types::tool::Tool {
        id: "mcp_db_query".into(),
        name: "query".into(),
        description: "Run SQL".into(),
        tool_type: wf_types::tool::ToolType::Mcp,
        parameters: None,
        metadata: None,
        config: Some(json!({ "server_name": "db" })),
        enabled: Some(true),
        strict: None,
        default_timeout_ms: None,
    }
}

#[test]
fn test_single_server_mcp_tool_resolves_server_from_registry_tool() {
    let coordinator = ToolApprovalCoordinator::new(mcp_options());
    let tool = mcp_tool_entry();
    let mut req = make_request("t1", "query", Some("mcp"));
    req.parameters = json!({});
    let ctx = McpToolContext {
        tool: Some(&tool),
        mcp_registry: None,
    };
    assert_eq!(
        coordinator
            .evaluate_with_mcp_context(std::slice::from_ref(&req), std::slice::from_ref(&ctx)),
        vec![ApprovalDecision::Approve]
    );
}

#[test]
fn test_single_server_mcp_tool_unknown_without_context_asks() {
    let coordinator = ToolApprovalCoordinator::new(mcp_options());
    let mut req = make_request("t1", "query", Some("mcp"));
    req.parameters = json!({});
    assert_eq!(
        coordinator.evaluate(std::slice::from_ref(&req)),
        vec![ApprovalDecision::Ask]
    );
}

#[test]
fn test_mcp_disabled_tool_is_denied_not_asked() {
    use wf_types::tool::mcp_connection::*;
    let coordinator = ToolApprovalCoordinator::new(mcp_options());
    let tool = mcp_tool_entry();
    let registry = crate::mcp::connection::McpServerRegistry::new();
    registry.register(
        "db",
        McpServerConfig::Stdio(McpStdioConfig {
            base: McpServerConfigBase {
                disabled: None,
                timeout: None,
                always_allow: None,
                disabled_tools: Some(vec!["query".to_string()]),
                lifecycle: None,
                idle_timeout: None,
                health_check_interval: None,
            },
            command: "echo".into(),
            args: None,
            cwd: None,
            env: None,
        }),
    );
    let mut req = make_request("t1", "query", Some("mcp"));
    req.parameters = json!({});
    let ctx = McpToolContext {
        tool: Some(&tool),
        mcp_registry: Some(&registry),
    };
    assert_eq!(
        coordinator
            .evaluate_with_mcp_context(std::slice::from_ref(&req), std::slice::from_ref(&ctx)),
        vec![ApprovalDecision::Deny(
            "MCP tool 'query' is disabled on server 'db'".to_string()
        )]
    );
}

#[test]
fn test_mcp_read_only_risk_level_case_insensitive() {
    use wf_types::tool::mcp_approval::*;
    let settings = McpApprovalSettings {
        servers: vec![McpApprovalServerConfig {
            name: "db".to_string(),
            tools: Some(vec![McpApprovalToolConfig {
                name: "query".to_string(),
                always_allow: None,
                risk_level: Some("read_only".to_string()),
            }]),
            resources: None,
            default_tool_behavior: Some(McpDefaultBehavior::AlwaysAsk),
            default_resource_behavior: None,
        }],
        default_server_behavior: Some(McpDefaultBehavior::AlwaysAsk),
    };
    let req = McpRequest {
        r#type: McpRequestType::UseMcp,
        server_name: "db".to_string(),
        tool_name: Some("query".to_string()),
        uri: None,
        arguments: None,
    };
    assert_eq!(check_mcp_approval(&settings, &req), McpDecision::Approve);
}

#[test]
fn test_apply_patch_with_sensitive_target_requires_approval() {
    let coordinator = ToolApprovalCoordinator::with_defaults();
    let mut req = make_request("t1", "apply_patch", Some("write"));
    req.parameters = json!({ "patch": "*** Begin Patch\n*** Update File: app/.env\n@@\n-old\n+new\n*** End Patch" });
    assert_eq!(
        coordinator.check_approval(&req, None, None),
        ApprovalDecision::Ask
    );
}

#[test]
fn test_network_domain_suffix_match() {
    assert!(domain_matches("api.evil.example", "evil.example"));
    assert!(domain_matches(
        "https://api.evil.example/v1",
        "evil.example"
    ));
    assert!(!domain_matches("not-evil.example", "evil.example"));
    assert!(!domain_matches("evil.example.evil.com", "evil.example"));
}

#[test]
fn test_shell_command_sensitive_write_routes_to_approval() {
    use super::file_policy::extract_shell_file_paths;
    let coordinator = ToolApprovalCoordinator::with_defaults();
    let mut req = make_execute_request("t1", "echo secret > app/.env");
    assert_eq!(
        coordinator.check_approval(&req, None, None),
        ApprovalDecision::Ask
    );
    let (_, writes) = extract_shell_file_paths("echo secret > app/.env");
    assert!(writes.iter().any(|p| p.ends_with("app/.env")));
    let fp = FilePermissionSettings::default_rules();
    assert!(writes
        .iter()
        .any(|p| !check_file_permission(p, &FileOperationType::Write, &fp)));
    req.parameters = json!({ "command": "echo hi" });
    let (reads, writes) = extract_shell_file_paths("echo hi");
    assert!(reads.is_empty() && writes.is_empty());
}

#[test]
fn test_shell_command_sensitive_read_routes_to_approval() {
    use super::file_policy::extract_shell_file_paths;
    let coordinator = ToolApprovalCoordinator::with_defaults();
    let req = make_execute_request("t1", "cat app/secrets/key.txt");
    assert_eq!(
        coordinator.check_approval(&req, None, None),
        ApprovalDecision::Ask
    );
    let (reads, _) = extract_shell_file_paths("cat app/secrets/key.txt");
    let fp = FilePermissionSettings::default_rules();
    assert!(reads
        .iter()
        .any(|p| !check_file_permission(p, &FileOperationType::Read, &fp)));
}

#[test]
fn test_nested_arguments_paths_are_extracted() {
    use super::file_policy::extract_file_paths;
    let params = json!({
        "server_name": "s",
        "tool_name": "t",
        "arguments": { "path": "secrets/key.txt" },
        "cwd": "/workspace/app",
        "uri": "file:///workspace/app/.env",
    });
    let paths = extract_file_paths(&params);
    assert!(paths.iter().any(|p| p.ends_with("secrets/key.txt")));
    assert!(paths.contains(&"/workspace/app".to_string()));
    assert!(paths.iter().any(|p| p.ends_with(".env")));
}
