/**
 * The `use_mcp` tool parameter Schema
 */

import type { ToolParameterSchema } from "@wf-agent/types";

/**
 * use_mcp tool parameter Schema
 */
export const useMcpSchema: ToolParameterSchema = {
  type: "object",
  properties: {
    server_name: {
      type: "string",
      description: "The name of the MCP server",
    },
    tool_name: {
      type: "string",
      description: "The name of the tool to execute (for tool calls)",
    },
    arguments: {
      type: "object",
      description: "Arguments to pass to the MCP tool (for tool calls)",
      additionalProperties: true,
    },
    uri: {
      type: "string",
      description: "The URI of the resource to access (for resource access)",
    },
  },
  required: ["server_name"],
};
