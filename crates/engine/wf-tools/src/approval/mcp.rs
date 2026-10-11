//! MCP approval: request construction and settings-based decisions.

use wf_types::interaction::tool_approval::ToolApprovalRequestData;
use wf_types::tool::approval::ToolApprovalOptions;
use wf_types::tool::mcp_approval::{
    McpApprovalSettings, McpDecision, McpDefaultBehavior, McpRequest, McpRequestType,
};
use wf_types::tool::Tool;

use super::network_policy::match_uri_pattern;
use super::ApprovalDecision;

/// Per-request registry context for MCP approval.
///
/// Callers that hold a tool registry pass the registered tool (for the
/// server name of single-server MCP tools, kept in the tool config) and
/// the MCP server registry (so connection-level disables agree with the
/// approval decision). Both are optional; absent context keeps the
/// parameter-only behavior.
#[derive(Default)]
pub struct McpToolContext<'a> {
    pub tool: Option<&'a Tool>,
    pub mcp_registry: Option<&'a crate::mcp::connection::McpServerRegistry>,
}

pub(super) fn handle_mcp_approval(
    options: &ToolApprovalOptions,
    tool: &ToolApprovalRequestData,
    mcp_tool: Option<&Tool>,
    mcp_registry: Option<&crate::mcp::connection::McpServerRegistry>,
) -> ApprovalDecision {
    if options.categories.as_ref().and_then(|c| c.always_allow_mcp) != Some(true) {
        return ApprovalDecision::Ask;
    }

    let Some(ref mcp_settings) = options.mcp else {
        return ApprovalDecision::Approve;
    };

    let Some(mcp_request) = build_mcp_request(tool, mcp_tool) else {
        return ApprovalDecision::Ask;
    };

    if let Some(registry) = mcp_registry {
        if let Some(tool_name) = mcp_request.tool_name.as_deref() {
            if !registry.is_tool_allowed(&mcp_request.server_name, tool_name) {
                return ApprovalDecision::Deny(format!(
                    "MCP tool '{}' is disabled on server '{}'",
                    tool_name, mcp_request.server_name
                ));
            }
        } else if registry.is_disabled(&mcp_request.server_name) {
            return ApprovalDecision::Deny(format!(
                "MCP server '{}' is disabled",
                mcp_request.server_name
            ));
        }
    }

    match check_mcp_approval(mcp_settings, &mcp_request) {
        McpDecision::Approve => ApprovalDecision::Approve,
        McpDecision::Deny => ApprovalDecision::Deny("MCP request denied".to_string()),
        McpDecision::Ask => ApprovalDecision::Ask,
    }
}

fn build_mcp_request(
    tool: &ToolApprovalRequestData,
    mcp_tool: Option<&Tool>,
) -> Option<McpRequest> {
    if let Some(uri) = tool.parameters.get("uri").and_then(|v| v.as_str()) {
        let server_name = tool
            .parameters
            .get("server_name")
            .and_then(|v| v.as_str())
            .map(str::to_string)
            .or_else(|| server_name_from_tool_config(mcp_tool))?;
        return Some(McpRequest {
            r#type: McpRequestType::ReadResource,
            server_name,
            tool_name: None,
            uri: Some(uri.to_string()),
            arguments: None,
        });
    }

    let server_name = tool
        .parameters
        .get("server_name")
        .and_then(|v| v.as_str())
        .map(str::to_string)
        .or_else(|| server_name_from_tool_config(mcp_tool))?;
    let tool_name = tool
        .parameters
        .get("tool_name")
        .and_then(|v| v.as_str())
        .map(str::to_string)
        .unwrap_or_else(|| tool.tool_name.clone());
    let args = tool.parameters.get("arguments").cloned();

    Some(McpRequest {
        r#type: McpRequestType::UseMcp,
        server_name,
        tool_name: Some(tool_name),
        uri: None,
        arguments: args,
    })
}

fn server_name_from_tool_config(mcp_tool: Option<&Tool>) -> Option<String> {
    mcp_tool.and_then(|t| {
        t.config
            .as_ref()
            .and_then(|c| c.get("server_name"))
            .and_then(|v| v.as_str())
            .map(str::to_string)
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
                    if cfg
                        .risk_level
                        .as_deref()
                        .is_some_and(|level| level.eq_ignore_ascii_case("read_only"))
                    {
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
