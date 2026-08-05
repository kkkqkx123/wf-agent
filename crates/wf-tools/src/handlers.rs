use crate::error::ToolResult;
use crate::filesystem::{FsToolConfig, FsToolHandlers};
use crate::predefined;
use crate::protect::ProtectController;
use crate::registry::ToolRegistry;
use wf_shell::config::ShellToolConfig;

/// Configuration for registering builtin tool handlers.
#[derive(Debug, Clone, Default)]
pub struct BuiltinHandlersConfig {
    pub fs: FsToolConfig,
    pub shell: ShellToolConfig,
    pub web: predefined::web::WebToolConfig,
    pub protect: Option<ProtectController>,
}

/// Create a ToolRegistry pre-wired with all builtin tool handlers
/// (filesystem, shell, memory, web, utility and background shell tools).
pub fn create_default_tool_registry() -> ToolRegistry {
    let registry = ToolRegistry::new();
    let _ = register_builtin_handlers(&registry, BuiltinHandlersConfig::default());
    registry
}

/// Register execution handlers for all builtin tools: filesystem
/// (read_file/write_file/edit_file/apply_patch/apply_diff/list_files/
/// grep_search/glob_search), shell (execute_command + background shell
/// sessions), memory (session notes + long-term memory), utility
/// (update_todo_list) and web (web_search/web_fetch). The tool definitions
/// are registered by wf-resource; this wires the actual execution logic into
/// the tool registry.
pub fn register_builtin_handlers(
    registry: &ToolRegistry,
    config: BuiltinHandlersConfig,
) -> ToolResult<()> {
    let handlers = FsToolHandlers::new(config.fs);
    let handlers = match config.protect {
        Some(protect) => handlers.with_protect(protect),
        None => handlers,
    };
    predefined::filesystem::register_handlers(registry, &handlers)?;

    predefined::shell::register(registry, &config.shell)?;
    predefined::memory::register(registry)?;
    predefined::utility::register(registry)?;
    predefined::web::register(registry, &config.web)?;

    // The `skill` builtin tool is always available; content loading is
    // served by the skill loader injected via ToolRegistry::set_skill_loader.
    registry.register_tool(predefined::knowledge::SKILL.tool_def());

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_register_and_execute_read_file() {
        let root = std::env::temp_dir().join(format!("wf-handlers-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join("x.txt"), "content-x\n").unwrap();

        let registry = ToolRegistry::new();
        let config = BuiltinHandlersConfig {
            fs: FsToolConfig {
                workspace_dir: Some(root.clone()),
                ..Default::default()
            },
            ..Default::default()
        };
        register_builtin_handlers(&registry, config).unwrap();

        let tool = wf_types::tool::Tool {
            id: "read_file".into(),
            name: "read_file".into(),
            description: "Read a file".into(),
            tool_type: wf_types::tool::ToolType::Stateless,
            parameters: None,
            metadata: None,
            config: None,
            enabled: Some(true),
            strict: None,
            default_timeout_ms: None,
        };
        registry.register_tool(tool);

        let ctx = crate::executor::trait_def::ToolExecutionContext::new("exec-1".into());
        let options = wf_types::tool::ToolExecutionOptions {
            timeout: None,
            retries: None,
            retry_delay: None,
            exponential_backoff: None,
        };
        let result = registry
            .execute_tool(
                "read_file",
                &serde_json::json!({ "path": "x.txt" }),
                &options,
                &ctx,
            )
            .await
            .unwrap();
        assert!(result.success);
        assert!(result
            .result
            .and_then(|v| v.as_str().map(String::from))
            .unwrap_or_default()
            .contains("content-x"));

        let _ = std::fs::remove_dir_all(&root);
    }

    #[tokio::test]
    async fn test_register_and_execute_execute_command() {
        let registry = ToolRegistry::new();
        register_builtin_handlers(&registry, BuiltinHandlersConfig::default()).unwrap();

        let tool = wf_types::tool::Tool {
            id: "execute_command".into(),
            name: "execute_command".into(),
            description: "Run a command".into(),
            tool_type: wf_types::tool::ToolType::Stateless,
            parameters: None,
            metadata: None,
            config: None,
            enabled: Some(true),
            strict: None,
            default_timeout_ms: None,
        };
        registry.register_tool(tool);

        let ctx = crate::executor::trait_def::ToolExecutionContext::new("exec-1".into());
        let options = wf_types::tool::ToolExecutionOptions {
            timeout: None,
            retries: None,
            retry_delay: None,
            exponential_backoff: None,
        };
        let result = registry
            .execute_tool(
                "execute_command",
                &serde_json::json!({ "command": "echo hi" }),
                &options,
                &ctx,
            )
            .await
            .unwrap();
        assert!(result.success);
    }

    #[tokio::test]
    async fn test_register_and_execute_apply_patch() {
        let root = std::env::temp_dir().join(format!("wf-handlers-patch-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();

        let registry = ToolRegistry::new();
        let config = BuiltinHandlersConfig {
            fs: FsToolConfig {
                workspace_dir: Some(root.clone()),
                ..Default::default()
            },
            ..Default::default()
        };
        register_builtin_handlers(&registry, config).unwrap();
        registry.register_tool(predefined::filesystem::APPLY_PATCH.tool_def());

        let ctx = crate::executor::trait_def::ToolExecutionContext::new("exec-1".into());
        let options = wf_types::tool::ToolExecutionOptions {
            timeout: None,
            retries: None,
            retry_delay: None,
            exponential_backoff: None,
        };

        let patch = "*** Begin Patch\n*** Add File: new.txt\n+hello patch\n*** Update File: new.txt\n@@\n+more\n*** End Patch";
        let result = registry
            .execute_tool(
                "apply_patch",
                &serde_json::json!({ "patch": patch }),
                &options,
                &ctx,
            )
            .await
            .unwrap();
        assert!(result.success, "patch failed: {:?}", result.error);
        assert!(root.join("new.txt").exists());
        assert_eq!(
            std::fs::read_to_string(root.join("new.txt")).unwrap(),
            "hello patch\nmore\n"
        );

        let _ = std::fs::remove_dir_all(&root);
    }
}
