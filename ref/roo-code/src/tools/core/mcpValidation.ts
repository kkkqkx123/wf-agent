/**
 * MCP Validation Utilities
 *
 * Provides validation logic for MCP tool calls and resource access.
 */

import type { Task } from "../../task/Task"
import { formatResponse } from "../../prompts/responses"
import { t } from "../../../i18n"
import { toolNamesMatch } from "../../../utils/mcp-name"
import type { McpHub } from "../../../services/mcp/McpHub"

// ─── Types ─────────────────────────────────────────────────────────────────────

export interface McpServerValidation {
	isValid: boolean
	server?: {
		name: string
		tools?: Array<{ name: string; enabledForPrompt?: boolean }>
		resources?: Array<{ uri: string }>
	}
	errorMessage?: string
}

export interface McpToolValidation {
	isValid: boolean
	resolvedToolName?: string
	availableTools?: string[]
	errorMessage?: string
}

export interface McpResourceValidation {
	isValid: boolean
	availableResources?: string[]
	errorMessage?: string
}

// ─── Server Validation ─────────────────────────────────────────────────────────

/**
 * Validates that an MCP server exists and is connected.
 */
export async function validateMcpServer(
	task: Task,
	serverName: string,
	pushToolResult: (content: string) => void,
): Promise<McpServerValidation> {
	const provider = task.providerRef.deref()
	const mcpHub = provider?.getMcpHub()

	if (!mcpHub) {
		// If we can't get the MCP hub, we can't validate, so proceed with caution
		return { isValid: true }
	}

	const servers = mcpHub.getAllServers()
	const server = servers.find((s) => s.name === serverName)

	if (!server) {
		const availableServersArray = servers.map((s) => s.name)
		const availableServers =
			availableServersArray.length > 0 ? availableServersArray.join(", ") : "No servers available"

		task.recordToolError("use_mcp")
		await task.say("error", t("mcp:errors.serverNotFound", { serverName, availableServers }))
		task.didToolFailInCurrentTurn = true

		pushToolResult(formatResponse.unknownMcpServerError(serverName, availableServersArray))
		return { isValid: false, errorMessage: "Server not found" }
	}

	return { isValid: true, server }
}

// ─── Tool Validation ───────────────────────────────────────────────────────────

/**
 * Validates that a tool exists on an MCP server and is enabled.
 */
export async function validateMcpTool(
	task: Task,
	serverName: string,
	toolName: string,
	server: { tools?: Array<{ name: string; enabledForPrompt?: boolean }> },
	pushToolResult: (content: string) => void,
): Promise<McpToolValidation> {
	// Check if the server has tools defined
	if (!server.tools || server.tools.length === 0) {
		task.recordToolError("use_mcp")
		await task.say("error", t("mcp:errors.toolNotFound", {
			toolName,
			serverName,
			availableTools: "No tools available",
		}))
		task.didToolFailInCurrentTurn = true

		pushToolResult(formatResponse.unknownMcpToolError(serverName, toolName, []))
		return { isValid: false, availableTools: [] }
	}

	// Check if the requested tool exists (using fuzzy matching to handle model mangling of hyphens)
	const tool = server.tools.find((t) => toolNamesMatch(t.name, toolName))

	if (!tool) {
		const availableToolNames = server.tools.map((tool) => tool.name)

		task.recordToolError("use_mcp")
		await task.say("error", t("mcp:errors.toolNotFound", {
			toolName,
			serverName,
			availableTools: availableToolNames.join(", "),
		}))
		task.didToolFailInCurrentTurn = true

		pushToolResult(formatResponse.unknownMcpToolError(serverName, toolName, availableToolNames))
		return { isValid: false, availableTools: availableToolNames }
	}

	// Check if the tool is disabled (enabledForPrompt is false)
	if (tool.enabledForPrompt === false) {
		const enabledTools = server.tools.filter((t) => t.enabledForPrompt !== false)
		const enabledToolNames = enabledTools.map((t) => t.name)

		task.recordToolError("use_mcp")
		await task.say(
			"error",
			t("mcp:errors.toolDisabled", {
				toolName,
				serverName,
				availableTools:
					enabledToolNames.length > 0 ? enabledToolNames.join(", ") : "No enabled tools available",
			}),
		)
		task.didToolFailInCurrentTurn = true

		pushToolResult(formatResponse.unknownMcpToolError(serverName, toolName, enabledToolNames))
		return { isValid: false, availableTools: enabledToolNames }
	}

	// Tool exists and is enabled - return the original tool name for use with the MCP server
	return {
		isValid: true,
		resolvedToolName: tool.name,
		availableTools: server.tools.map((t) => t.name),
	}
}

// ─── Resource Validation ───────────────────────────────────────────────────────

/**
 * Validates that resources are available on an MCP server.
 * Note: MCP doesn't have a standard way to validate specific resource URIs,
 * so we just check if the server has any resources.
 */
export async function validateMcpResource(
	task: Task,
	serverName: string,
	server: { resources?: Array<{ uri: string }> },
	pushToolResult: (content: string) => void,
): Promise<McpResourceValidation> {
	// Resources are optional on MCP servers, so we don't fail if none exist
	// The actual resource access will fail if the URI is invalid
	const availableResources = server.resources?.map((r) => r.uri) ?? []

	return {
		isValid: true,
		availableResources,
	}
}

// ─── Helper Functions ───────────────────────────────────────────────────────────

/**
 * Checks if any MCP server has resources available.
 */
export function hasAnyMcpResources(mcpHub: McpHub): boolean {
	const servers = mcpHub.getServers()
	return servers.some((server) => server.resources && server.resources.length > 0)
}

/**
 * Checks if any MCP server has tools available.
 */
export function hasAnyMcpTools(mcpHub: McpHub): boolean {
	const servers = mcpHub.getServers()
	return servers.some((server) => server.tools && server.tools.length > 0)
}
