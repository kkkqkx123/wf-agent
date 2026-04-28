import * as vscode from "vscode"

import { CoderEventName, type HistoryItem } from "@coder/types"

import { Task } from "../task/Task"
import { formatResponse } from "../prompts/responses"
import { Package } from "../../shared/package"
import type { ToolUse } from "../../shared/tools"
import { t } from "../../i18n"

import { BaseTool, ToolCallbacks } from "./core/BaseTool"

interface AttemptCompletionParams {
	result: string
	command?: string
}

export interface AttemptCompletionCallbacks extends ToolCallbacks {
	askFinishSubTaskApproval: () => Promise<boolean>
	toolDescription: () => string
}

/**
 * Interface for provider methods needed by AttemptCompletionTool for delegation handling.
 */
interface DelegationProvider {
	getTaskWithId(id: string): Promise<{ historyItem: HistoryItem }>
	reopenParentFromDelegation(params: {
		parentTaskId: string
		childTaskId: string
		completionResultSummary: string
	}): Promise<void>
}

export class AttemptCompletionTool extends BaseTool<"attempt_completion"> {
	readonly name = "attempt_completion" as const

	/**
	 * Track the last streamed result to avoid duplicate rendering during streaming.
	 * This ensures we only update the UI when the content actually changes.
	 */
	private lastStreamedResult: string | undefined = undefined

	async execute(params: AttemptCompletionParams, task: Task, callbacks: AttemptCompletionCallbacks): Promise<void> {
		const { result } = params
		const { handleError, pushToolResult, askFinishSubTaskApproval } = callbacks

		// Prevent attempt_completion if any tool failed in the current turn
		if (task.didToolFailInCurrentTurn) {
			const errorMsg = t("common:errors.attempt_completion_tool_failed")

			await task.say("error", errorMsg)
			pushToolResult(formatResponse.toolError(errorMsg))
			return
		}

		const preventCompletionWithOpenTodos = vscode.workspace
			.getConfiguration(Package.name)
			.get<boolean>("preventCompletionWithOpenTodos", false)

		const hasIncompleteTodos = task.todoList && task.todoList.some((todo) => todo.status !== "completed")

		if (preventCompletionWithOpenTodos && hasIncompleteTodos) {
			task.metricsService?.recordToolError("attempt_completion")

			pushToolResult(
				formatResponse.toolError(
					"Cannot complete task while there are incomplete todos. Please finish all todos before attempting completion.",
				),
			)
			task.didToolFailInCurrentTurn = true

			return
		}

		try {
			if (!result) {
				// Reset streaming state on error
				this.lastStreamedResult = undefined

				task.metricsService?.recordToolError("attempt_completion")
				pushToolResult(await task.sayAndCreateMissingParamError("attempt_completion", "result"))
				task.didToolFailInCurrentTurn = true
				return
			}

			// Reset streaming state before final display to ensure clean transition
			this.lastStreamedResult = undefined

			await task.say("completion_result", result)

			// Force final token usage update before emitting TaskCompleted
			// This ensures the most recent stats are captured regardless of throttle timer
			// and properly updates the snapshot to prevent redundant emissions
			task.emitFinalTokenUsageUpdate()

			task.emit(CoderEventName.TaskCompleted, task.taskId, task.metricsService.getTokenUsage(task.clineMessages.slice(1)), task.metricsService.getToolUsage())

			// Check for subtask using parentTaskId (metadata-driven delegation)
			if (task.parentTaskId) {
				// Check if this subtask has already completed and returned to parent
				// to prevent duplicate tool_results when user revisits from history
				const provider = task.providerRef.deref() as DelegationProvider | undefined
				if (provider) {
					try {
						const { historyItem } = await provider.getTaskWithId(task.taskId)
						const status = historyItem?.status

						if (status === "completed") {
							// Subtask already completed - skip delegation flow entirely
							// Fall through to normal completion ask flow below (outside this if block)
							// This shows the user the completion result and waits for acceptance
							// without injecting another tool_result to the parent
						} else if (status === "active") {
							// Normal subtask completion - do delegation
							const delegated = await this.delegateToParent(
								task,
								result,
								provider,
								askFinishSubTaskApproval,
								pushToolResult,
							)
							if (delegated) return
						} else {
							// Unexpected status (undefined or "delegated") - log error and skip delegation
							// undefined indicates a bug in status persistence during child creation
							// "delegated" would mean this child has its own grandchild pending (shouldn't reach attempt_completion)
							console.error(
								`[AttemptCompletionTool] Unexpected child task status "${status}" for task ${task.taskId}. ` +
								`Expected "active" or "completed". Skipping delegation to prevent data corruption.`,
							)
							// Fall through to normal completion ask flow
						}
					} catch (err) {
						// If we can't get the history, log error and skip delegation
						console.error(
							`[AttemptCompletionTool] Failed to get history for task ${task.taskId}: ${(err as Error)?.message ?? String(err)}. ` +
							`Skipping delegation.`,
						)
						// Fall through to normal completion ask flow
					}
				}
			}

			const { response, text, images } = await task.ask("completion_result", "", false)

			if (response === "yesButtonClicked") {
				// CRITICAL FIX: Set abort flag to stop the main task loop when user confirms completion
				// Without this, the loop continues running (while (!this.abort)) even though taskState is 'completed',
				// preventing the task from properly ending and blocking new user input.
				task.abort = true
				return
			}

			// User provided feedback - push tool result to continue the conversation
			await task.say("user_feedback", text ?? "", images)

			const feedbackText = `<user_message>\n${text}\n</user_message>`
			pushToolResult(formatResponse.toolResult(feedbackText, images))
		} catch (error) {
			// Reset streaming state on error
			this.lastStreamedResult = undefined
			await handleError("inspecting site", error as Error)
		}
	}

	/**
	 * Handles the common delegation flow when a subtask completes.
	 * Returns true if delegation was performed and the caller should return early.
	 */
	private async delegateToParent(
		task: Task,
		result: string,
		provider: DelegationProvider,
		askFinishSubTaskApproval: () => Promise<boolean>,
		pushToolResult: (result: string) => void,
	): Promise<boolean> {
		const didApprove = await askFinishSubTaskApproval()

		if (!didApprove) {
			pushToolResult(formatResponse.toolDenied())
			return true
		}

		pushToolResult("")

		await provider.reopenParentFromDelegation({
			parentTaskId: task.parentTaskId!,
			childTaskId: task.taskId,
			completionResultSummary: result,
		})

		return true
	}

	override async handlePartial(task: Task, block: ToolUse<"attempt_completion">): Promise<void> {
		const result: string | undefined = block.params.result
		const command: string | undefined = block.params.command

		// Handle command parameter separately
		if (command) {
			const lastMessage = task.clineMessages.at(-1)
			if (lastMessage && lastMessage.ask === "command") {
				await task.ask("command", command, block.partial).catch(() => { })
			} else {
				// For command with result, show result and emit completion event
				await task.say("completion_result", result ?? "")

				// Force final token usage update before emitting TaskCompleted for consistency
				task.emitFinalTokenUsageUpdate()

				task.emit(CoderEventName.TaskCompleted, task.taskId, task.metricsService.getTokenUsage(task.clineMessages.slice(1)), task.metricsService.getToolUsage())

				await task.ask("command", command, block.partial).catch(() => { })
			}
			return
		}

		// For result-only cases, show streaming content only if we have something new
		// This prevents duplicate rendering and ensures smooth visual updates
		if (result !== undefined && result !== this.lastStreamedResult) {
			await task.say("completion_result", result, undefined, block.partial)
			this.lastStreamedResult = result
		}
	}
}

export const attemptCompletionTool = new AttemptCompletionTool()
