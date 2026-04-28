import type { ClineAskUseMcpServer } from "@coder/types"

import { Task } from "../task/Task"
import { formatResponse } from "../prompts/responses"
import { t } from "../../i18n"
import type { ToolUse } from "../../shared/tools"

import { BaseTool, ToolCallbacks } from "./core/BaseTool"
import {
	validateMcpServer,
	validateMcpTool,
	validateMcpResource,
} from "./core/mcpValidation"
import {
	executeMcpTool,
	executeMcpResourceAccess,
	reportMcpToolResult,
	reportMcpResourceResult,
} from "./core/mcpExecution"

interface UseMcpParams {
	server_name: string
	tool_name?: string
	arguments?: Record<string, unknown>
	uri?: string
}

export class UseMcpTool extends BaseTool<"use_mcp"> {
	readonly name = "use_mcp" as const

	async execute(params: UseMcpParams, task: Task, callbacks: ToolCallbacks): Promise<void> {
		const { askApproval, handleError, pushToolResult } = callbacks

		try {
			// Validate required parameters
			if (!params.server_name) {
				task.didToolFailInCurrentTurn = true
				task.recordToolError("use_mcp")
				pushToolResult(await task.sayAndCreateMissingParamError("use_mcp", "server_name"))
				return
			}

			// Determine operation type and validate
			const isToolCall = !!params.tool_name
			const isResourceAccess = !!params.uri

			if (!isToolCall && !isResourceAccess) {
				task.didToolFailInCurrentTurn = true
				task.recordToolError("use_mcp")
				await task.say("error", t("mcp:errors.missingToolOrUri"))
				pushToolResult(
					formatResponse.toolError(
						"Either 'tool_name' or 'uri' must be provided. Use 'tool_name' to call a tool, or 'uri' to access a resource.",
					),
				)
				return
			}

			if (isToolCall && isResourceAccess) {
				task.didToolFailInCurrentTurn = true
				task.recordToolError("use_mcp")
				await task.say("error", t("mcp:errors.bothToolAndUri"))
				pushToolResult(
					formatResponse.toolError(
						"Cannot specify both 'tool_name' and 'uri'. Use one or the other.",
					),
				)
				return
			}

			// Validate arguments for tool calls
			let parsedArguments: Record<string, unknown> | undefined
			if (isToolCall && params.arguments !== undefined) {
				if (typeof params.arguments !== "object" || params.arguments === null || Array.isArray(params.arguments)) {
					task.recordToolError("use_mcp")
					await task.say("error", t("mcp:errors.invalidJsonArgument", { toolName: params.tool_name! }))
					task.didToolFailInCurrentTurn = true
					pushToolResult(
						formatResponse.toolError(
							formatResponse.invalidMcpToolArgumentError(params.server_name, params.tool_name!),
						),
					)
					return
				}
				parsedArguments = params.arguments
			}

			// Validate server exists
			const serverValidation = await validateMcpServer(task, params.server_name, pushToolResult)
			if (!serverValidation.isValid) {
				return
			}

			const server = serverValidation.server!

			// Execute based on operation type
			if (isToolCall) {
				await this.executeToolCall(
					task,
					params.server_name,
					params.tool_name!,
					parsedArguments,
					server,
					askApproval,
					pushToolResult,
				)
			} else {
				await this.executeResourceAccess(
					task,
					params.server_name,
					params.uri!,
					server,
					askApproval,
					pushToolResult,
				)
			}
		} catch (error) {
			await handleError("executing MCP operation", error as Error)
		}
	}

	private async executeToolCall(
		task: Task,
		serverName: string,
		toolName: string,
		arguments_: Record<string, unknown> | undefined,
		server: any,
		askApproval: ToolCallbacks["askApproval"],
		pushToolResult: ToolCallbacks["pushToolResult"],
	): Promise<void> {
		// Validate tool exists
		const toolValidation = await validateMcpTool(task, serverName, toolName, server, pushToolResult)
		if (!toolValidation.isValid) {
			return
		}

		// Use the resolved tool name (original name from the server)
		const resolvedToolName = toolValidation.resolvedToolName ?? toolName

		// Get user approval
		const completeMessage = JSON.stringify({
			type: "use_mcp",
			serverName,
			toolName: resolvedToolName,
			arguments: arguments_ ? JSON.stringify(arguments_) : undefined,
		} satisfies ClineAskUseMcpServer)

		const didApprove = await askApproval("use_mcp_server", completeMessage)
		if (!didApprove) {
			return
		}

		// Execute the tool
		const executionId = task.lastMessageTs?.toString() ?? Date.now().toString()
		const result = await executeMcpTool(task, serverName, resolvedToolName, arguments_, executionId)

		// Report result
		await reportMcpToolResult(task, result, pushToolResult)
	}

	private async executeResourceAccess(
		task: Task,
		serverName: string,
		uri: string,
		server: any,
		askApproval: ToolCallbacks["askApproval"],
		pushToolResult: ToolCallbacks["pushToolResult"],
	): Promise<void> {
		// Validate resources are available (optional check)
		await validateMcpResource(task, serverName, server, pushToolResult)

		// Get user approval
		const completeMessage = JSON.stringify({
			type: "use_mcp",
			serverName,
			uri,
		} satisfies ClineAskUseMcpServer)

		const didApprove = await askApproval("use_mcp_server", completeMessage)
		if (!didApprove) {
			pushToolResult(formatResponse.toolDenied())
			return
		}

		// Access the resource
		const result = await executeMcpResourceAccess(task, serverName, uri)

		// Report result
		await reportMcpResourceResult(task, result, pushToolResult)
	}

	override async handlePartial(task: Task, block: ToolUse<"use_mcp">): Promise<void> {
		const params = block.params

		const partialMessage = JSON.stringify({
			type: "use_mcp",
			serverName: params.server_name ?? "",
			toolName: params.tool_name ?? undefined,
			arguments: params.arguments,
			uri: params.uri ?? undefined,
		} satisfies ClineAskUseMcpServer)

		await task.ask("use_mcp_server", partialMessage, true).catch(() => { })
	}
}

export const useMcpTool = new UseMcpTool()
