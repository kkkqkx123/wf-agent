/**
 * MCP Execution Utilities
 *
 * Provides execution logic for MCP tool calls and resource access.
 */

import type { McpExecutionStatus } from "@coder/types"
import type { Task } from "../../task/Task"
import { formatResponse } from "../../prompts/responses"

// ─── Types ─────────────────────────────────────────────────────────────────────

export interface McpToolResult {
	text: string
	images: string[]
	isError?: boolean
}

export interface McpResourceResult {
	text: string
	images: string[]
}

// ─── Status Reporting ───────────────────────────────────────────────────────────

/**
 * Sends MCP execution status to the webview.
 */
export async function sendMcpExecutionStatus(
	task: Task,
	status: McpExecutionStatus,
): Promise<void> {
	const clineProvider = await task.providerRef.deref()
	clineProvider?.postMessageToWebview({
		type: "mcpExecutionStatus",
		text: JSON.stringify(status),
	})
}

// ─── Tool Execution ────────────────────────────────────────────────────────────

/**
 * Processes tool result content from MCP server.
 */
export function processMcpToolContent(toolResult: any): McpToolResult {
	if (!toolResult?.content || toolResult.content.length === 0) {
		return { text: "", images: [], isError: toolResult?.isError }
	}

	const images: string[] = []

	const textContent = toolResult.content
		.map((item: any) => {
			if (item.type === "text") {
				return item.text
			}
			if (item.type === "resource") {
				const { blob: _, ...rest } = item.resource
				return JSON.stringify(rest, null, 2)
			}
			if (item.type === "image") {
				// Handle image content (MCP image content has mimeType and data properties)
				if (item.mimeType && item.data) {
					if (item.data.startsWith("data:")) {
						images.push(item.data)
					} else {
						images.push(`data:${item.mimeType};base64,${item.data}`)
					}
				}
				return ""
			}
			return ""
		})
		.filter(Boolean)
		.join("\n\n")

	return { text: textContent, images, isError: toolResult.isError }
}

/**
 * Executes an MCP tool and returns the result.
 */
export async function executeMcpTool(
	task: Task,
	serverName: string,
	toolName: string,
	arguments_: Record<string, unknown> | undefined,
	executionId: string,
): Promise<McpToolResult> {
	await task.say("mcp_server_request_started")

	// Send started status
	await sendMcpExecutionStatus(task, {
		executionId,
		status: "started",
		serverName,
		toolName,
	})

	const toolResult = await task.providerRef.deref()?.getMcpHub()?.callTool(serverName, toolName, arguments_)

	if (!toolResult) {
		// Send error status if no result
		await sendMcpExecutionStatus(task, {
			executionId,
			status: "error",
			error: "No response from MCP server",
		})
		return { text: "(No response)", images: [] }
	}

	const { text: outputText, images } = processMcpToolContent(toolResult)

	if (outputText || images.length > 0) {
		await sendMcpExecutionStatus(task, {
			executionId,
			status: "output",
			response: outputText || (images.length > 0 ? `[${images.length} image(s)]` : ""),
		})
	}

	const toolResultPretty =
		(toolResult.isError ? "Error:\n" : "") +
		(outputText || (images.length > 0 ? `[${images.length} image(s) received]` : ""))

	// Send completion status
	await sendMcpExecutionStatus(task, {
		executionId,
		status: toolResult.isError ? "error" : "completed",
		response: toolResultPretty,
		error: toolResult.isError ? "Error executing MCP tool" : undefined,
	})

	return { text: toolResultPretty, images, isError: toolResult.isError }
}

// ─── Resource Execution ─────────────────────────────────────────────────────────

/**
 * Processes resource result content from MCP server.
 */
export function processMcpResourceContent(resourceResult: any): McpResourceResult {
	if (!resourceResult?.contents || resourceResult.contents.length === 0) {
		return { text: "(Empty response)", images: [] }
	}

	const images: string[] = []

	const textContent = resourceResult.contents
		.map((item: any) => {
			if (item.text) {
				return item.text
			}
			return ""
		})
		.filter(Boolean)
		.join("\n\n") || "(Empty response)"

	// Handle images (image must contain mimetype and blob)
	resourceResult.contents.forEach((item: any) => {
		if (item.mimeType?.startsWith("image") && item.blob) {
			if (item.blob.startsWith("data:")) {
				images.push(item.blob)
			} else {
				images.push(`data:${item.mimeType};base64,` + item.blob)
			}
		}
	})

	return { text: textContent, images }
}

/**
 * Accesses an MCP resource and returns the result.
 */
export async function executeMcpResourceAccess(
	task: Task,
	serverName: string,
	uri: string,
): Promise<McpResourceResult> {
	await task.say("mcp_server_request_started")

	const resourceResult = await task.providerRef.deref()?.getMcpHub()?.readResource(serverName, uri)

	return processMcpResourceContent(resourceResult)
}

// ─── Result Reporting ──────────────────────────────────────────────────────────

/**
 * Reports tool execution result to the task and pushes the result.
 */
export async function reportMcpToolResult(
	task: Task,
	result: McpToolResult,
	pushToolResult: (content: string | Array<any>) => void,
): Promise<void> {
	await task.say("mcp_server_response", result.text, result.images)
	pushToolResult(formatResponse.toolResult(result.text, result.images))
}

/**
 * Reports resource access result to the task and pushes the result.
 */
export async function reportMcpResourceResult(
	task: Task,
	result: McpResourceResult,
	pushToolResult: (content: string | Array<any>) => void,
): Promise<void> {
	await task.say("mcp_server_response", result.text, result.images)
	pushToolResult(formatResponse.toolResult(result.text, result.images))
}
