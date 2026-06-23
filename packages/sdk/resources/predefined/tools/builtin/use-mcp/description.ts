/**
 * Tool Description for `use_mcp`
 */

import type { ToolDescriptionData } from "@wf-agent/types";

export const USE_MCP_TOOL_DESCRIPTION: ToolDescriptionData = {
  id: "use_mcp",
  type: "STATELESS",
  category: "code",
  description: `Use capabilities provided by connected MCP (Model Context Protocol) servers. MCP servers extend system capabilities by providing additional tools and resources through a standardized protocol.

## Available MCP Servers
To see available servers and their tools, check the system context. Each server may provide:
- Custom tools: Execute with server_name and tool_name
- Resources: Access with server_name and resource URI

## Usage Patterns

### Calling a Tool
use_mcp(server_name="server_name", tool_name="tool_name", arguments={...})

### Accessing a Resource
use_mcp(server_name="server_name", uri="resource://uri")

## Important Notes
- MCP servers must be configured in mcp-settings.json (global or project .wf/)
- Use the list_mcp_tools tool to discover available servers and tools
- Tool call success depends on server being connected and tool being available
- Resource URIs follow the pattern: protocol://path (e.g., file://path/to/file)

## Configuration
Servers are defined in:
- Global: ~/.agent/mcp-settings.json
- Project: .wf/mcp.json (highest priority)

Each server can be configured with:
- Connection type: stdio, sse, or streamable-http
- Lifecycle: lazy (connect on demand), eager, or keep-alive
- Timeout and health check intervals`,
  parameters: [
    {
      name: "server_name",
      type: "string",
      required: true,
      description: "The name of the MCP server to connect to (e.g., 'filesystem', 'github')",
    },
    {
      name: "tool_name",
      type: "string",
      required: false,
      description: "The name of the tool to execute. Required for tool calls. Optional for resource access.",
    },
    {
      name: "arguments",
      type: "object",
      required: false,
      description: "Tool arguments as key-value pairs. Used with tool_name for tool execution.",
    },
    {
      name: "uri",
      type: "string",
      required: false,
      description: "Resource URI to access (e.g., 'file://path/to/file'). Used without tool_name for resource reading.",
    },
  ],
  tips: [
    "Each request should provide either tool_name+arguments OR uri, not both",
    "Start with server_name to discover what tools are available",
    "Check tool descriptions in system context for available MCP servers",
    "Resource access is read-only through the URI pattern",
  ],
};
