//! End-to-end MCP integration tests: a real stdio server script speaking
//! minimal JSON-RPC over newline-delimited stdin/stdout.

use std::path::PathBuf;
use std::sync::Arc;

use wf_tools::mcp::connection::{McpConnectionManager, McpServerRegistry};
use wf_tools::mcp::registration::register_mcp_tools;
use wf_tools::registry::ToolRegistry;
use wf_types::tool::mcp_connection::{McpServerConfig, McpServerConfigBase, McpStdioConfig};

/// Minimal MCP server responding to initialize / tools/list / tools/call.
const SERVER_SCRIPT: &str = r#"#!/bin/sh
while IFS= read -r line; do
  id=$(printf '%s' "$line" | sed -n 's/.*"id":"\([^"]*\)".*/\1/p')
  case "$line" in
    *"\"method\":\"initialize\""*)
      printf '{"jsonrpc":"2.0","id":"%s","result":{"protocolVersion":"2024-11-05","capabilities":{"tools":{}},"serverInfo":{"name":"test-srv","version":"1.0"},"instructions":"use me"}}\n' "$id"
      ;;
    *"\"method\":\"tools/list\""*)
      printf '{"jsonrpc":"2.0","id":"%s","result":{"tools":[{"name":"ping","description":"ping tool","inputSchema":{"type":"object","properties":{"msg":{"type":"string"}},"required":[]}}]}}\n' "$id"
      ;;
    *"\"method\":\"tools/call\""*)
      printf '{"jsonrpc":"2.0","id":"%s","result":{"content":[{"type":"text","text":"pong"}]}}\n' "$id"
      ;;
    *"\"method\":\"resources/list\""*)
      printf '{"jsonrpc":"2.0","id":"%s","result":{"resources":[]}}\n' "$id"
      ;;
    *"\"method\":\"resources/templates/list\""*)
      printf '{"jsonrpc":"2.0","id":"%s","result":{"resourceTemplates":[]}}\n' "$id"
      ;;
  esac
done
"#;

fn write_server_script() -> PathBuf {
    let dir = std::env::temp_dir().join(format!("wf-mcp-e2e-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("mcp_server.sh");
    std::fs::write(&path, SERVER_SCRIPT).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(&path).unwrap().permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&path, perms).unwrap();
    }
    path
}

fn stdio_config(command: &str) -> McpServerConfig {
    McpServerConfig::Stdio(McpStdioConfig {
        base: McpServerConfigBase {
            disabled: None,
            timeout: Some(10),
            always_allow: None,
            disabled_tools: None,
            lifecycle: None,
            idle_timeout: None,
            health_check_interval: None,
        },
        command: command.to_string(),
        args: None,
        cwd: None,
        env: None,
    })
}

#[tokio::test]
async fn test_connect_discover_and_call_tool() {
    let script = write_server_script();
    let registry = Arc::new(McpServerRegistry::new());
    let manager = McpConnectionManager::new(registry.clone());
    registry.register("e2e", stdio_config(script.to_str().unwrap()));

    manager.connect("e2e").await.expect("connect + handshake");

    // Capabilities were discovered during connect.
    let entry = registry.get("e2e").unwrap();
    assert_eq!(entry.status, wf_types::tool::mcp_connection::McpServerStatus::Connected);
    assert_eq!(entry.tools.len(), 1);
    assert_eq!(entry.tools[0].name, "ping");

    let tools = manager.discover_tools("e2e").await.unwrap();
    assert_eq!(tools[0].description.as_deref(), Some("ping tool"));

    // Tool call round-trip.
    let result = manager
        .call_tool_on_server("e2e", "ping", &serde_json::json!({"msg": "hi"}), 5000)
        .await
        .unwrap();
    assert!(result.to_string().contains("pong"));

    manager.disconnect("e2e").await.unwrap();
    let _ = std::fs::remove_dir_all(script.parent().unwrap());
}

#[tokio::test]
async fn test_lazy_server_connects_on_first_use() {
    let script = write_server_script();
    let registry = Arc::new(McpServerRegistry::new());
    let manager = McpConnectionManager::new(registry.clone());
    registry.register("lazy", stdio_config(script.to_str().unwrap()));

    // Lazy registration: not connected yet.
    assert!(!manager.connected_servers().contains(&"lazy".to_string()));

    // First use auto-connects.
    let result = manager
        .call_tool_on_server("lazy", "ping", &serde_json::json!({}), 5000)
        .await;
    assert!(result.is_ok(), "lazy connect failed: {:?}", result);
    assert!(manager.connected_servers().contains(&"lazy".to_string()));

    manager.disconnect("lazy").await.unwrap();
    let _ = std::fs::remove_dir_all(script.parent().unwrap());
}

#[tokio::test]
async fn test_registration_registers_use_mcp_and_discovered_tools() {
    let script = write_server_script();
    let registry = Arc::new(McpServerRegistry::new());
    let manager = McpConnectionManager::new(registry.clone());
    registry.register("e2e2", stdio_config(script.to_str().unwrap()));
    manager.connect("e2e2").await.unwrap();

    let tool_registry = Arc::new(ToolRegistry::new());
    register_mcp_tools(&tool_registry, &manager).unwrap();

    // use_mcp registered.
    assert!(tool_registry.get_tool("use_mcp").is_some());
    // Per-server tool registered with config.
    let ping = tool_registry.get_tool("mcp_e2e2_ping").unwrap();
    assert_eq!(ping.name, "ping");
    assert_eq!(
        ping.config
            .as_ref()
            .and_then(|c| c.get("server_name"))
            .and_then(|v| v.as_str()),
        Some("e2e2")
    );

    manager.disconnect("e2e2").await.unwrap();
    let _ = std::fs::remove_dir_all(script.parent().unwrap());
}
