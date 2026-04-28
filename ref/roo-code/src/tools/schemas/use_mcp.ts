import { z } from "zod"
import type OpenAI from "openai"

import { createOpenAITool } from "./base"

// ─── Schema Definitions ────────────────────────────────────────────────────────

/**
 * Schema for use_mcp tool parameters.
 * Unified tool that handles both MCP tool calls and resource access.
 */
export const UseMcpParamsSchema = z.object({
	server_name: z
		.string()
		.describe("The name of the MCP server"),
	tool_name: z
		.string()
		.optional()
		.describe("The name of the tool to execute (for tool calls)"),
	arguments: z
		.record(z.any())
		.optional()
		.describe("Arguments to pass to the MCP tool (for tool calls)"),
	uri: z
		.string()
		.optional()
		.describe("The URI of the resource to access (for resource access)"),
})

// ─── Type Exports ──────────────────────────────────────────────────────────────

export type UseMcpParams = z.infer<typeof UseMcpParamsSchema>

// ─── Tool Creation ──────────────────────────────────────────────────────────────

const USE_MCP_DESCRIPTION = `Use a capability provided by a connected MCP (Model Context Protocol) server. MCP servers extend the capabilities of the system by providing additional tools and resources.

Parameters:
- server_name: (required) The name of the MCP server
- tool_name: (optional) The name of the tool to execute on the MCP server
- arguments: (optional) Arguments to pass to the MCP tool (used with tool_name)
- uri: (optional) The URI of the resource to access

Usage:
- To call a tool: provide server_name, tool_name, and optionally arguments
- To access a resource: provide server_name and uri

Example: Calling a weather tool
{ "server_name": "weather-server", "tool_name": "get_current_weather", "arguments": { "location": "San Francisco" } }

Example: Accessing a weather resource
{ "server_name": "weather-server", "uri": "weather://san-francisco/current" }

Example: Calling a database tool
{ "server_name": "database-server", "tool_name": "query", "arguments": { "sql": "SELECT * FROM users LIMIT 10" } }

Note: MCP servers must be configured and connected before you can use their capabilities. Refer to the MCP server documentation for available tools, resources, and their required parameters.`

/**
 * Creates the use_mcp tool definition.
 *
 * @returns Native tool definition for use_mcp
 */
export function createUseMcpTool(): OpenAI.Chat.ChatCompletionTool {
	return createOpenAITool({
		name: "use_mcp",
		description: USE_MCP_DESCRIPTION,
		schema: UseMcpParamsSchema,
		strict: true,
	})
}

/**
 * Default use_mcp tool definition.
 */
export const useMcpTool = createUseMcpTool()
