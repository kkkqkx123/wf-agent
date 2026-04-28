/**
 * Tool Description for `use_mcp`
 */

import type { ToolDescriptionData } from "@wf-agent/prompt-templates";

export const USE_MCP_TOOL_DESCRIPTION: ToolDescriptionData = {
  name: "use_mcp",
  id: "use_mcp",
  type: "STATELESS",
  category: "code",
  description: `Use a capability provided by a connected MCP (Model Context Protocol) server. MCP servers extend the capabilities of the system by providing additional tools and resources.

Usage:
- To call a tool: provide server_name, tool_name, and optionally arguments
- To access a resource: provide server_name and uri

MCP servers must be configured and connected before you can use their capabilities. Refer to the MCP server documentation for available tools, resources, and their required parameters.`,
  parameters: [
    {
      name: "server_name",
      type: "string",
      required: true,
      description: "The name of the MCP server",
    },
    {
      name: "tool_name",
      type: "string",
      required: false,
      description: "The name of the tool to execute on the MCP server",
    },
    {
      name: "arguments",
      type: "object",
      required: false,
      description: "Arguments to pass to the MCP tool (used with tool_name)",
    },
    {
      name: "uri",
      type: "string",
      required: false,
      description: "The URI of the resource to access",
    },
  ],
  tips: [
    "To call a tool: provide server_name, tool_name, and optionally arguments",
    "To access a resource: provide server_name and uri",
    "MCP servers must be configured and connected first",
  ],
};
