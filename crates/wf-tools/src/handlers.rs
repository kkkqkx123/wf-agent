use crate::error::ToolResult;
use crate::filesystem::{FsToolConfig, FsToolHandlers};
use crate::protect::ProtectController;
use crate::registry::ToolRegistry;
use crate::shell::{execute_command_handler, ShellToolConfig};

/// Configuration for registering builtin tool handlers.
#[derive(Debug, Clone, Default)]
pub struct BuiltinHandlersConfig {
    pub fs: FsToolConfig,
    pub shell: ShellToolConfig,
    pub protect: Option<ProtectController>,
}

/// Create a ToolRegistry pre-wired with the builtin filesystem and shell
/// handlers (read_file/write_file/edit_file/list_files/grep_search/
/// glob_search/execute_command).
pub fn create_default_tool_registry() -> ToolRegistry {
    let registry = ToolRegistry::new();
    let _ = register_builtin_handlers(&registry, BuiltinHandlersConfig::default());
    registry
}

/// Register execution handlers for the builtin filesystem and shell tools:
/// read_file, write_file, edit_file, list_files, grep_search, glob_search,
/// execute_command. The tool definitions are registered by wf-resource;
/// this wires the actual execution logic into the tool registry.
pub fn register_builtin_handlers(
    registry: &ToolRegistry,
    config: BuiltinHandlersConfig,
) -> ToolResult<()> {
    let handlers = FsToolHandlers::new(config.fs);
    let handlers = match config.protect {
        Some(protect) => handlers.with_protect(protect),
        None => handlers,
    };

    for tool_name in [
        "read_file",
        "write_file",
        "edit_file",
        "list_files",
        "grep_search",
        "glob_search",
    ] {
        let handler = handlers.handler(tool_name)?;
        registry.register_stateless_handler(tool_name, handler);
    }

    let shell_handler = execute_command_handler(config.shell);
    registry.register_stateless_async_handler("execute_command", shell_handler);

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
}
