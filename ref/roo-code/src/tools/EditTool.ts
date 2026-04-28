import fs from "fs/promises"
import path from "path"

import { type ClineSayTool, DEFAULT_WRITE_DELAY_MS } from "@coder/types"

import { getReadablePath } from "../../utils/path"
import { isPathOutsideWorkspace } from "../../utils/pathUtils"
import { Task } from "../task/Task"
import { formatResponse } from "../prompts/responses"
import { RecordSource } from "../context/tracking/FileContextTrackerTypes"
import { fileExistsAtPath } from "../../utils/fs"
import { EXPERIMENT_IDS, experiments } from "../../shared/experiments"
import { sanitizeUnifiedDiff, computeDiffStats } from "../diff/stats"
import type { ToolUse } from "../../shared/tools"

import { BaseTool, ToolCallbacks } from "./core/BaseTool"
import {
	MissingParameterError,
	InvalidParameterError,
	FileNotFoundToolError,
	RooIgnoreViolationError,
	ContentNotFoundError,
	DuplicateMatchError,
} from "../errors/tools/index.js"

interface EditParams {
	file_path: string
	old_string: string
	new_string: string
	replace_all?: boolean
}

export class EditTool extends BaseTool<"edit"> {
	readonly name = "edit" as const

	async execute(params: EditParams, task: Task, callbacks: ToolCallbacks): Promise<void> {
		const { file_path: relPath, old_string: oldString, new_string: newString, replace_all: replaceAll } = params
		const { askApproval, handleError, pushToolResult } = callbacks

		try {
			// Validate required parameters using structured errors
			if (!relPath) {
				task.didToolFailInCurrentTurn = true
				const error = new MissingParameterError("edit", "file_path")
				task.recordToolError("edit", error.toLogEntry())
				pushToolResult(formatResponse.toolErrorFromInstance(error.toLLMMessage()))
				return
			}

			if (!oldString) {
				task.didToolFailInCurrentTurn = true
				const error = new MissingParameterError("edit", "old_string")
				task.recordToolError("edit", error.toLogEntry())
				pushToolResult(formatResponse.toolErrorFromInstance(error.toLLMMessage()))
				return
			}

			if (newString === undefined) {
				task.didToolFailInCurrentTurn = true
				const error = new MissingParameterError("edit", "new_string")
				task.recordToolError("edit", error.toLogEntry())
				pushToolResult(formatResponse.toolErrorFromInstance(error.toLLMMessage()))
				return
			}

			// Check old_string !== new_string
			if (oldString === newString) {
				task.didToolFailInCurrentTurn = true
				const error = new InvalidParameterError(
					"edit",
					"old_string",
					oldString,
					"'old_string' and 'new_string' are identical. No changes needed."
				)
				task.recordToolError("edit", error.toLogEntry())
				pushToolResult(formatResponse.toolErrorFromInstance(error.toLLMMessage()))
				return
			}

			// Check .rooignore access
			const accessAllowed = task.rooIgnoreController?.validateAccess(relPath)

			if (!accessAllowed) {
				const error = new RooIgnoreViolationError("edit", relPath)
				await task.say("rooignore_error", relPath)
				task.recordToolError("edit", error.toLogEntry())
				pushToolResult(formatResponse.toolErrorFromInstance(error.toLLMMessage()))
				return
			}

			// Check if file is write-protected
			const isWriteProtected = task.rooProtectedController?.isWriteProtected(relPath) || false

			const absolutePath = path.resolve(task.cwd, relPath)

			const fileExists = await fileExistsAtPath(absolutePath)
			if (!fileExists) {
				task.didToolFailInCurrentTurn = true
				const error = new FileNotFoundToolError("edit", relPath)
				task.recordToolError("edit", error.toLogEntry())
				pushToolResult(formatResponse.toolErrorFromInstance(error.toLLMMessage()))
				return
			}

			let fileContent: string
			try {
				fileContent = await fs.readFile(absolutePath, "utf8")
				// Normalize line endings to LF for consistent matching
				fileContent = fileContent.replace(/\r\n/g, "\n")
			} catch (error) {
				task.didToolFailInCurrentTurn = true
				const errorMessage = `Failed to read file '${relPath}'. Please verify file permissions and try again.`
				await handleError("reading file", new Error(errorMessage))
				return
			}

			// Normalize line endings in old_string/new_string to match file content
			const normalizedOld = oldString.replace(/\r\n/g, "\n")
			const normalizedNew = newString.replace(/\r\n/g, "\n")

			// Count occurrences of old_string in file content
			const matchCount = fileContent.split(normalizedOld).length - 1

			if (matchCount === 0) {
				task.didToolFailInCurrentTurn = true
				const error = new ContentNotFoundError("edit", relPath, normalizedOld)
				task.recordToolError("edit", error.toLogEntry())
				pushToolResult(formatResponse.toolErrorFromInstance(error.toLLMMessage()))
				return
			}

			// Uniqueness check when replace_all is not enabled
			if (!replaceAll && matchCount > 1) {
				task.didToolFailInCurrentTurn = true
				const error = new DuplicateMatchError("edit", relPath, normalizedOld, matchCount)
				task.recordToolError("edit", error.toLogEntry())
				pushToolResult(formatResponse.toolErrorFromInstance(error.toLLMMessage()))
				return
			}

			// Apply the replacement
			let newContent: string
			if (replaceAll) {
				// Replace all occurrences
				const searchPattern = new RegExp(escapeRegExp(normalizedOld), "g")
				newContent = fileContent.replace(searchPattern, () => normalizedNew)
			} else {
				// Replace single occurrence (already verified uniqueness above)
				newContent = fileContent.replace(normalizedOld, () => normalizedNew)
			}

			// Check if any changes were made
			if (newContent === fileContent) {
				pushToolResult(`No changes needed for '${relPath}'`)
				return
			}

			// Initialize diff view
			task.diffViewProvider.editType = "modify"
			task.diffViewProvider.originalContent = fileContent

			// Generate and validate diff
			const diff = formatResponse.createPrettyPatch(relPath, fileContent, newContent)
			if (!diff) {
				pushToolResult(`No changes needed for '${relPath}'`)
				await task.diffViewProvider.reset()
				return
			}

			// Check if preventFocusDisruption experiment is enabled
			const provider = task.providerRef.deref()
			const state = await provider?.configurationService.getState()
			const diagnosticsEnabled = state?.diagnosticsEnabled ?? true
			const writeDelayMs = state?.writeDelayMs ?? DEFAULT_WRITE_DELAY_MS
			const isPreventFocusDisruptionEnabled = experiments.isEnabled(
				state?.experiments ?? {},
				EXPERIMENT_IDS.PREVENT_FOCUS_DISRUPTION,
			)

			const sanitizedDiff = sanitizeUnifiedDiff(diff)
			const diffStats = computeDiffStats(sanitizedDiff) || undefined
			const isOutsideWorkspace = isPathOutsideWorkspace(absolutePath)

			const sharedMessageProps: ClineSayTool = {
				tool: "appliedDiff",
				path: getReadablePath(task.cwd, relPath),
				diff: sanitizedDiff,
				isOutsideWorkspace,
			}

			const completeMessage = JSON.stringify({
				...sharedMessageProps,
				content: sanitizedDiff,
				isProtected: isWriteProtected,
				diffStats,
			} satisfies ClineSayTool)

			// Show diff view if focus disruption prevention is disabled
			if (!isPreventFocusDisruptionEnabled) {
				await task.diffViewProvider.open(relPath)
				await task.diffViewProvider.update(newContent, true)
				task.diffViewProvider.scrollToFirstDiff()
			}

			const didApprove = await askApproval("tool", completeMessage, undefined, isWriteProtected)

			if (!didApprove) {
				// Revert changes if diff view was shown
				if (!isPreventFocusDisruptionEnabled) {
					await task.diffViewProvider.revertChanges()
				}
				pushToolResult("Changes were rejected by the user.")
				await task.diffViewProvider.reset()
				return
			}

			// Save the changes
			if (isPreventFocusDisruptionEnabled) {
				// Direct file write without diff view or opening the file
				await task.diffViewProvider.saveDirectly(relPath, newContent, false, diagnosticsEnabled, writeDelayMs)
			} else {
				// Call saveChanges to update the DiffViewProvider properties
				await task.diffViewProvider.saveChanges(diagnosticsEnabled, writeDelayMs)
			}

			// Track file edit operation
			if (relPath) {
				await task.fileContextTracker.trackFileContext(relPath, "roo_edited" as RecordSource)
			}

			task.didEditFile = true

			// Get the formatted response message
			const message = await task.diffViewProvider.pushToolWriteResult(task, task.cwd, false)
			pushToolResult(message)

			// Record successful tool usage and cleanup
			task.recordToolUsage("edit")
			await task.diffViewProvider.reset()
			this.resetPartialState()

			// Process any queued messages after file edit completes
			task.processQueuedMessages()
		} catch (error) {
			await handleError("edit", error as Error)
			await task.diffViewProvider.reset()
			this.resetPartialState()
		}
	}

	override async handlePartial(task: Task, block: ToolUse<"edit">): Promise<void> {
		const relPath: string | undefined = block.params.file_path

		// Wait for path to stabilize before showing UI (prevents truncated paths)
		if (!this.hasPathStabilized(relPath)) {
			return
		}

		// relPath is guaranteed non-null after hasPathStabilized
		const absolutePath = path.resolve(task.cwd, relPath!)
		const isOutsideWorkspace = isPathOutsideWorkspace(absolutePath)

		const sharedMessageProps: ClineSayTool = {
			tool: "appliedDiff",
			path: getReadablePath(task.cwd, relPath!),
			diff: block.params.old_string ? "1 edit operation" : undefined,
			isOutsideWorkspace,
		}

		await task.ask("tool", JSON.stringify(sharedMessageProps), block.partial).catch(() => { })
	}
}

/**
 * Escapes special regex characters in a string
 * @param input String to escape regex characters in
 * @returns Escaped string safe for regex pattern matching
 */
function escapeRegExp(input: string): string {
	return input.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")
}

export const editTool = new EditTool()
export const searchAndReplaceTool = editTool // alias for backward compat
