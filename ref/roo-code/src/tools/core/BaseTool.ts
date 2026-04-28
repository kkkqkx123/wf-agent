import type { ToolName } from "@coder/types"

import { Task } from "../../task/Task"
import type { ToolUse, HandleError, PushToolResult, AskApproval } from "../../../shared/tools"
import type { ToolParamsMap } from "../schemas/registry"
import { ValidationError, type LogEntry } from "../../errors/tools/validation-errors.js"

/**
 * Callbacks passed to tool execution
 */
export interface ToolCallbacks {
	askApproval: AskApproval
	handleError: HandleError
	pushToolResult: PushToolResult
	toolCallId?: string
}

/**
 * Helper type to extract the parameter type for a tool based on its name.
 * If the tool has native args defined in ToolParamsMap, use those; otherwise fall back to any.
 */
type ToolParams<TName extends ToolName> = TName extends keyof ToolParamsMap ? ToolParamsMap[TName] : any

/**
 * Abstract base class for all tools.
 *
 * Tools receive typed arguments from native tool calling via `ToolUse.nativeArgs`.
 *
 * @template TName - The specific tool name, which determines native arg types
 */
export abstract class BaseTool<TName extends ToolName> {
	/**
	 * The tool's name (must match ToolName type)
	 */
	abstract readonly name: TName

	/**
	 * Track the last seen path during streaming to detect when the path has stabilized.
	 * Used by hasPathStabilized() to prevent displaying truncated paths from partial-json parsing.
	 */
	protected lastSeenPartialPath: string | undefined = undefined

	/**
	 * Execute the tool with typed parameters.
	 *
	 * Receives typed parameters from native tool calling via `ToolUse.nativeArgs`.
	 *
	 * @param params - Typed parameters
	 * @param task - Task instance with state and API access
	 * @param callbacks - Tool execution callbacks (approval, error handling, results)
	 */
	abstract execute(params: ToolParams<TName>, task: Task, callbacks: ToolCallbacks): Promise<void>

	/**
	 * Handle partial (streaming) tool messages.
	 *
	 * Default implementation does nothing. Tools that support streaming
	 * partial messages should override this.
	 *
	 * @param task - Task instance
	 * @param block - Partial ToolUse block
	 */
	async handlePartial(task: Task, block: ToolUse<TName>): Promise<void> {
		// Default: no-op for partial messages
		// Tools can override to show streaming UI updates
	}

	/**
	 * Check if a path parameter has stabilized during streaming.
	 *
	 * During native tool call streaming, the partial-json library may return truncated
	 * string values when chunk boundaries fall mid-value. This method tracks the path
	 * value between consecutive handlePartial() calls and returns true only when the
	 * path has stopped changing (stabilized).
	 *
	 * Usage in handlePartial():
	 * ```typescript
	 * if (!this.hasPathStabilized(block.params.path)) {
	 *     return // Path still changing, wait for it to stabilize
	 * }
	 * // Path is stable, proceed with UI updates
	 * ```
	 *
	 * @param path - The current path value from the partial block
	 * @returns true if path has stabilized (same value seen twice) and is non-empty, false otherwise
	 */
	protected hasPathStabilized(path: string | undefined): boolean {
		const pathHasStabilized = this.lastSeenPartialPath !== undefined && this.lastSeenPartialPath === path
		this.lastSeenPartialPath = path
		return pathHasStabilized && !!path
	}

	/**
	 * Reset the partial state tracking.
	 *
	 * Should be called at the end of execute() (both success and error paths)
	 * to ensure clean state for the next tool invocation.
	 */
	resetPartialState(): void {
		this.lastSeenPartialPath = undefined
	}

	/**
	 * Main entry point for tool execution.
	 *
	 * Handles the complete flow:
	 * 1. Partial message handling (if partial)
	 * 2. Parameter parsing (nativeArgs only)
	 * 3. Core execution (execute)
	 * 4. Success/failure tracking (consecutiveMistakeCount management)
	 *
	 * @param task - Task instance
	 * @param block - ToolUse block from assistant message
	 * @param callbacks - Tool execution callbacks
	 */
	async handle(task: Task, block: ToolUse<TName>, callbacks: ToolCallbacks): Promise<void> {
		// Track execution state for unified error handling
		let executionStarted = false
		let executionSucceeded = false

		try {
			// Handle partial messages
			if (block.partial) {
				try {
					await this.handlePartial(task, block)
				} catch (error) {
					// Use structured logging for partial message errors
					const errorMessage = error instanceof Error ? error.message : String(error)
					const logEntry: LogEntry = {
						level: "error",
						category: "tool_execution",
						tool: this.name,
						error_type: error instanceof Error ? error.constructor.name : "UnknownError",
						message: `Error in handlePartial: ${errorMessage}`,
						timestamp: Date.now(),
					}
					if (error instanceof Error && error.stack) {
						logEntry.stack = error.stack
					}
					task.recordToolError(this.name, logEntry)
					await callbacks.handleError(
						`handling partial ${this.name}`,
						error instanceof Error ? error : new Error(String(error)),
					)
				}
				return
			}

			// Native-only: obtain typed parameters from `nativeArgs`.
			let params: ToolParams<TName>
			try {
				if (block.nativeArgs !== undefined) {
					// Native: typed args provided by NativeToolCallParser.
					params = block.nativeArgs as ToolParams<TName>
				} else {
					// If legacy/XML markup was provided via params, surface a clear error.
					const paramsText = (() => {
						try {
							return JSON.stringify(block.params ?? {})
						} catch {
							return ""
						}
					})()
					if (paramsText.includes("<") && paramsText.includes(">")) {
						throw new Error(
							"XML tool calls are no longer supported. Use native tool calling (nativeArgs) instead.",
						)
					}
					throw new Error("Tool call is missing native arguments (nativeArgs).")
				}
			} catch (error) {
				// Use structured logging for parameter parsing errors
				const errorMessage = error instanceof Error ? error.message : String(error)
				const logEntry: LogEntry = {
					level: "error",
					category: "tool_execution",
					tool: this.name,
					error_type: error instanceof Error ? error.constructor.name : "UnknownError",
					message: `Error parsing parameters: ${errorMessage}`,
					timestamp: Date.now(),
				}
				if (error instanceof Error && error.stack) {
					logEntry.stack = error.stack
				}
				task.recordToolError(this.name, logEntry)
				// Use structured error types when possible
				if (error instanceof ValidationError) {
					// Structured error with LLM guidance
					await callbacks.handleError(
						`parsing ${this.name} args`,
						error,
					)
				} else {
					// Fallback for generic errors
					const genericErrorMessage = `Failed to parse ${this.name} parameters: ${errorMessage}`
					await callbacks.handleError(`parsing ${this.name} args`, new Error(genericErrorMessage))
				}
				// Note: handleError already emits a tool_result via formatResponse.toolError in the caller.
				// Do NOT call pushToolResult here to avoid duplicate tool_result payloads.
				return
			}

			// Execute with typed parameters
			executionStarted = true
			await this.execute(params, task, callbacks)

			// Check if the tool marked itself as failed (e.g., pushToolResult with toolError)
			// If so, increment the mistake count instead of resetting it
			if (task.didToolFailInCurrentTurn) {
				task.consecutiveMistakeCount++
				// Reset the flag for the next tool execution
				task.didToolFailInCurrentTurn = false
			} else {
				// Reset consecutive mistake count on success
				// This is the central place to ensure all tools reset the counter
				task.consecutiveMistakeCount = 0
			}

			// Reset partial state tracking after successful execution
			this.resetPartialState()
		} catch (error) {
			// Error handling for execute() failures
			// Increment mistake count for execution errors
			if (executionStarted && !executionSucceeded) {
				task.consecutiveMistakeCount++
			}

			// Log the error
			const errorMessage = error instanceof Error ? error.message : String(error)
			const logEntry: LogEntry = {
				level: "error",
				category: "tool_execution",
				tool: this.name,
				error_type: error instanceof Error ? error.constructor.name : "UnknownError",
				message: `Error executing ${this.name}: ${errorMessage}`,
				timestamp: Date.now(),
			}
			if (error instanceof Error && error.stack) {
				logEntry.stack = error.stack
			}
			task.recordToolError(this.name, logEntry)

			// Use structured error types when possible
			if (error instanceof ValidationError) {
				await callbacks.handleError(
					`executing ${this.name}`,
					error,
				)
			} else {
				const genericErrorMessage = `Failed to execute ${this.name}: ${errorMessage}`
				await callbacks.handleError(`executing ${this.name}`, error instanceof Error ? error : new Error(genericErrorMessage))
			}

			// Reset partial state tracking after failed execution
			this.resetPartialState()
		}
	}
}
