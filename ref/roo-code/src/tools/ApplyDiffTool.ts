import path from "path"
import fs from "fs/promises"

import { type ClineSayTool, DEFAULT_WRITE_DELAY_MS } from "@coder/types"

import { getReadablePath } from "../../utils/path"
import { Task } from "../task/Task"
import { formatResponse } from "../prompts/responses"
import { fileExistsAtPath } from "../../utils/fs"
import { RecordSource } from "../context/tracking/FileContextTrackerTypes"
import { unescapeHtmlEntities } from "../../utils/text-normalization"
import { EXPERIMENT_IDS, experiments } from "../../shared/experiments"
import { computeDiffStats, sanitizeUnifiedDiff } from "../diff/stats"
import type { ToolUse } from "../../shared/tools"

import { BaseTool, ToolCallbacks } from "./core/BaseTool"
import {
	MissingParameterError,
	RooIgnoreViolationError,
	FileNotFoundToolError,
	DiffApplyFailedError,
} from "../errors/tools/index.js"

interface ApplyDiffParams {
	path: string
	diff: string
}

export class ApplyDiffTool extends BaseTool<"apply_diff"> {
	readonly name = "apply_diff" as const

	async execute(params: ApplyDiffParams, task: Task, callbacks: ToolCallbacks): Promise<void> {
		const { askApproval, handleError, pushToolResult } = callbacks
		let { path: relPath, diff: diffContent } = params

		if (diffContent && !task.api.getModel().id.includes("claude")) {
			diffContent = unescapeHtmlEntities(diffContent)
		}

		try {
			// Validate required parameters using structured errors
			if (!relPath) {
				task.didToolFailInCurrentTurn = true
				const error = new MissingParameterError("apply_diff", "path")
				task.recordToolError("apply_diff", error.toLogEntry())
				pushToolResult(formatResponse.toolErrorFromInstance(error.toLLMMessage()))
				return
			}

			if (!diffContent) {
				task.didToolFailInCurrentTurn = true
				const error = new MissingParameterError("apply_diff", "diff")
				task.recordToolError("apply_diff", error.toLogEntry())
				pushToolResult(formatResponse.toolErrorFromInstance(error.toLLMMessage()))
				return
			}

			// Check .rooignore access
			const accessAllowed = task.rooIgnoreController?.validateAccess(relPath)

			if (!accessAllowed) {
				const error = new RooIgnoreViolationError("apply_diff", relPath)
				await task.say("rooignore_error", relPath)
				task.recordToolError("apply_diff", error.toLogEntry())
				pushToolResult(formatResponse.toolErrorFromInstance(error.toLLMMessage()))
				return
			}

			const absolutePath = path.resolve(task.cwd, relPath)
			const fileExists = await fileExistsAtPath(absolutePath)

			if (!fileExists) {
				task.consecutiveMistakeCount++
				const error = new FileNotFoundToolError("apply_diff", relPath)
				task.recordToolError("apply_diff", error.toLogEntry())
				task.didToolFailInCurrentTurn = true
				await task.say("error", error.message)
				pushToolResult(formatResponse.toolErrorFromInstance(error.toLLMMessage()))
				return
			}

			const originalContent: string = await fs.readFile(absolutePath, "utf-8")

			// Apply the diff to the original content
			const diffResult = (await task.diffStrategy?.applyDiff(
				originalContent,
				diffContent,
				parseInt(params.diff.match(/:start_line:(\d+)/)?.[1] ?? ""),
			)) ?? {
				success: false,
				error: "No diff strategy available",
			}

			if (!diffResult.success) {
				task.didToolFailInCurrentTurn = true
				const currentCount = (task.consecutiveMistakeCountForApplyDiff.get(relPath) || 0) + 1
				task.consecutiveMistakeCountForApplyDiff.set(relPath, currentCount)

				// Extract error reason from failParts or the main error
				let errorReason = "Unknown diff apply error"
				if (diffResult.failParts && diffResult.failParts.length > 0 && diffResult.failParts[0]?.success === false) {
					errorReason = (diffResult.failParts[0] as Extract<typeof diffResult.failParts[number], { success: false }>).error ?? errorReason
				} else if ("error" in diffResult && diffResult.error) {
					errorReason = diffResult.error
				}

				const error = new DiffApplyFailedError("apply_diff", relPath, errorReason)

				if (currentCount >= 2) {
					await task.say("diff_error", error.message)
				}

				task.recordToolError("apply_diff", error.toLogEntry())
				pushToolResult(formatResponse.toolErrorFromInstance(error.toLLMMessage()))
				return
			}

			task.consecutiveMistakeCountForApplyDiff.delete(relPath)

			// Generate backend-unified diff for display in chat/webview
			const unifiedPatchRaw = formatResponse.createPrettyPatch(relPath, originalContent, diffResult.content)
			const unifiedPatch = sanitizeUnifiedDiff(unifiedPatchRaw)
			const diffStats = computeDiffStats(unifiedPatch) || undefined

			// Check if preventFocusDisruption experiment is enabled
			const provider = task.providerRef.deref()
			const state = await provider?.configurationService.getState()
			const diagnosticsEnabled = state?.diagnosticsEnabled ?? true
			const writeDelayMs = state?.writeDelayMs ?? DEFAULT_WRITE_DELAY_MS
			const isPreventFocusDisruptionEnabled = experiments.isEnabled(
				state?.experiments ?? {},
				EXPERIMENT_IDS.PREVENT_FOCUS_DISRUPTION,
			)

			// Check if file is write-protected
			const isWriteProtected = task.rooProtectedController?.isWriteProtected(relPath) || false

			const sharedMessageProps: ClineSayTool = {
				tool: "appliedDiff",
				path: getReadablePath(task.cwd, relPath),
				diff: diffContent,
			}

			if (isPreventFocusDisruptionEnabled) {
				// Direct file write without diff view
				const completeMessage = JSON.stringify({
					...sharedMessageProps,
					diff: diffContent,
					content: unifiedPatch,
					diffStats,
					isProtected: isWriteProtected,
				} satisfies ClineSayTool)

				let toolProgressStatus

				if (task.diffStrategy && task.diffStrategy.getProgressStatus) {
					const block: ToolUse<"apply_diff"> = {
						type: "tool_use",
						name: "apply_diff",
						params: { path: relPath, diff: diffContent },
						partial: false,
					}
					toolProgressStatus = task.diffStrategy.getProgressStatus(block, diffResult)
				}

				const didApprove = await askApproval("tool", completeMessage, toolProgressStatus, isWriteProtected)

				if (!didApprove) {
					return
				}

				// Save directly without showing diff view or opening the file
				task.diffViewProvider.editType = "modify"
				task.diffViewProvider.originalContent = originalContent
				await task.diffViewProvider.saveDirectly(
					relPath,
					diffResult.content,
					false,
					diagnosticsEnabled,
					writeDelayMs,
				)
			} else {
				// Original behavior with diff view
				// Show diff view before asking for approval
				task.diffViewProvider.editType = "modify"
				await task.diffViewProvider.open(relPath)
				await task.diffViewProvider.update(diffResult.content, true)
				task.diffViewProvider.scrollToFirstDiff()

				const completeMessage = JSON.stringify({
					...sharedMessageProps,
					diff: diffContent,
					content: unifiedPatch,
					diffStats,
					isProtected: isWriteProtected,
				} satisfies ClineSayTool)

				let toolProgressStatus

				if (task.diffStrategy && task.diffStrategy.getProgressStatus) {
					const block: ToolUse<"apply_diff"> = {
						type: "tool_use",
						name: "apply_diff",
						params: { path: relPath, diff: diffContent },
						partial: false,
					}
					toolProgressStatus = task.diffStrategy.getProgressStatus(block, diffResult)
				}

				const didApprove = await askApproval("tool", completeMessage, toolProgressStatus, isWriteProtected)

				if (!didApprove) {
					await task.diffViewProvider.revertChanges()
					task.processQueuedMessages()
					return
				}

				// Call saveChanges to update the DiffViewProvider properties
				await task.diffViewProvider.saveChanges(diagnosticsEnabled, writeDelayMs)
			}

			// Track file edit operation
			if (relPath) {
				await task.fileContextTracker.trackFileContext(relPath, "roo_edited" as RecordSource)
			}

			// Used to determine if we should wait for busy terminal to update before sending api request
			task.didEditFile = true
			let partFailHint = ""

			if (diffResult.failParts && diffResult.failParts.length > 0) {
				partFailHint = `But unable to apply all diff parts to file: ${absolutePath}. Use the read_file tool to check the newest file version and re-apply diffs.\n`
			}

			// Get the formatted response message
			const message = await task.diffViewProvider.pushToolWriteResult(task, task.cwd, !fileExists)

			// Check for single SEARCH/REPLACE block warning
			const searchBlocks = (diffContent.match(/<<<<<<< SEARCH/g) || []).length
			const singleBlockNotice =
				searchBlocks === 1
					? "\n<notice>Making multiple related changes in a single apply_diff is more efficient. If other changes are needed in this file, please include them as additional SEARCH/REPLACE blocks.</notice>"
					: ""

			if (partFailHint) {
				pushToolResult(partFailHint + message + singleBlockNotice)
			} else {
				pushToolResult(message + singleBlockNotice)
			}

			await task.diffViewProvider.reset()
			this.resetPartialState()

			// Process any queued messages after file edit completes
			task.processQueuedMessages()

			return
		} catch (error) {
			await handleError("applying diff", error as Error)
			await task.diffViewProvider.reset()
			this.resetPartialState()
			task.processQueuedMessages()
			return
		}
	}

	override async handlePartial(task: Task, block: ToolUse<"apply_diff">): Promise<void> {
		const relPath: string | undefined = block.params.path
		const diffContent: string | undefined = block.params.diff

		// Wait for path to stabilize before showing UI (prevents truncated paths)
		if (!this.hasPathStabilized(relPath)) {
			return
		}

		const sharedMessageProps: ClineSayTool = {
			tool: "appliedDiff",
			path: getReadablePath(task.cwd, relPath),
			diff: diffContent,
		}

		let toolProgressStatus

		if (task.diffStrategy && task.diffStrategy.getProgressStatus) {
			toolProgressStatus = task.diffStrategy.getProgressStatus(block)
		}

		if (toolProgressStatus && Object.keys(toolProgressStatus).length === 0) {
			return
		}

		await task.ask("tool", JSON.stringify(sharedMessageProps), block.partial, toolProgressStatus).catch(() => { })
	}
}

export const applyDiffTool = new ApplyDiffTool()
