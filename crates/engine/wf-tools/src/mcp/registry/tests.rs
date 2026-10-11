//! Registry tests: registration, disabled servers, tool allowances and
//! timeout/lifecycle resolution.

use super::*;
use wf_types::tool::mcp_connection::*;

fn make_stdio_config(command: &str) -> McpServerConfig {
    McpServerConfig::Stdio(McpStdioConfig {
        base: McpServerConfigBase {
            disabled: None,
            timeout: Some(5),
            always_allow: None,
            disabled_tools: None,
            lifecycle: None,
            idle_timeout: None,
            health_check_interval: None,
        },
        command: command.into(),
        args: None,
        cwd: None,
        env: None,
    })
}

#[test]
fn test_register_and_get_server() {
    let registry = McpServerRegistry::new();
    registry.register("test", make_stdio_config("echo"));

    let entry = registry.get("test").unwrap();
    assert_eq!(entry.name, "test");
    assert_eq!(entry.status, McpServerStatus::Disconnected);
    assert_eq!(entry.lifecycle, McpLifecycleMode::Lazy);
}

#[test]
fn test_disabled_server() {
    let registry = McpServerRegistry::new();
    registry.register("test", make_stdio_config("echo"));
    assert!(!registry.is_disabled("test"));

    let mut cfg = make_stdio_config("echo");
    if let McpServerConfig::Stdio(ref mut c) = cfg {
        c.base.disabled = Some(true);
    }
    registry.register("disabled_sv", cfg);
    assert!(registry.is_disabled("disabled_sv"));
}

#[test]
fn test_tool_allowed() {
    let registry = McpServerRegistry::new();
    registry.register("test", make_stdio_config("echo"));
    assert!(registry.is_tool_allowed("test", "any_tool"));

    let mut cfg = make_stdio_config("echo");
    if let McpServerConfig::Stdio(ref mut c) = cfg {
        c.base.disabled_tools = Some(vec!["dangerous_tool".into()]);
    }
    registry.register("restricted", cfg);
    assert!(registry.is_tool_allowed("restricted", "safe_tool"));
    assert!(!registry.is_tool_allowed("restricted", "dangerous_tool"));
}

#[test]
fn test_unregister_server() {
    let registry = McpServerRegistry::new();
    registry.register("test", make_stdio_config("echo"));
    assert!(registry.get("test").is_some());

    registry.unregister("test");
    assert!(registry.get("test").is_none());
}

#[test]
fn test_timeout_is_seconds_based() {
    assert_eq!(server_timeout_ms(&make_stdio_config("echo")), 5000);
    let mut cfg = make_stdio_config("echo");
    if let McpServerConfig::Stdio(ref mut c) = cfg {
        c.base.timeout = None;
    }
    assert_eq!(server_timeout_ms(&cfg), DEFAULT_MCP_TIMEOUT_MS);
}

#[test]
fn test_lifecycle_from_config() {
    let mut cfg = make_stdio_config("echo");
    if let McpServerConfig::Stdio(ref mut c) = cfg {
        c.base.lifecycle = Some(McpServerLifecycle::KeepAlive);
        c.base.health_check_interval = Some(30);
    }
    let entry = McpServerEntry::new("ka".into(), cfg);
    assert_eq!(entry.lifecycle, McpLifecycleMode::KeepAlive);
    assert_eq!(entry.health_check_interval, Some(Duration::from_secs(30)));
}
