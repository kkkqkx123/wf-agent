import fs from "fs/promises"
import path from "path"

import { type ClineSayTool, DEFAULT_WRITE_DELAY_MS } from "@coder/types"

import { getReadablePath } from "../../utils/path"
import { isPathOutsideWorkspace } from "../../utils/pathUtils"
import { Task } from "../task/Task"
import { formatResponse } from "../prompts/responses"
import { RecordSource } from "../context/tracking/FileContextTrackerTypes"
import { fileExistsAtPath, createDirectoriesForFile } from "../../utils/fs"
import { EXPERIMENT_IDS, experiments } from "../../shared/experiments"
import { sanitizeUnifiedDiff, computeDiffStats } from "../diff/stats"
import { BaseTool, ToolCallbacks } from "./core/BaseTool"
import type { ToolUse } from "../../shared/tools"
import {
	parsePatch,
	processAllHunks,
	PatchErrorCode,
	PatchError as PatchErrorType,
} from "./apply-patch"
import type { ApplyPatchFileChange, ApplyPatchResult, ApplyPatchFileResult, ApplyPatchSummary } from "./apply-patch"
import {
	MissingParameterError,
	PatchParseError,
	FileNotFoundToolError,
	RooIgnoreViolationError,
	FileAlreadyExistsError,
	PermissionDeniedToolError,
	createMutableToolResult,
	type ToolExecutionResult,
	type ToolError,
} from "../errors/tools/index.js"

interface ApplyPatchParams {
	patch: string
}

export class ApplyPatchTool extends BaseTool<"apply_patch"> {
	readonly name = "apply_patch" as const

	private static readonly FILE_HEADER_MARKERS = ["*** Add File: ", "*** Delete File: ", "*** Update File: "] as const

	private extractFirstPathFromPatch(patch: string | undefined): string | undefined {
		if (!patch) {
			return undefined
		}

		const lines = patch.split("\n")
		const hasTrailingNewline = patch.endsWith("\n")
		const completeLines = hasTrailingNewline ? lines : lines.slice(0, -1)

		for (const rawLine of completeLines) {
			const line = rawLine.trim()

			for (const marker of ApplyPatchTool.FILE_HEADER_MARKERS) {
				if (!line.startsWith(marker)) {
					continue
				}

				const candidatePath = line.substring(marker.length).trim()
				if (candidatePath.length > 0) {
					return candidatePath
				}
			}
		}

		return undefined
	}

	/**
	 * Create a structured result object.
	 */
	private createResult(
		success: boolean,
		results: ApplyPatchFileResult[],
		error?: string,
	): ApplyPatchResult {
		const summary: ApplyPatchSummary = {
			total: results.length,
			succeeded: results.filter((r) => r.success).length,
			failed: results.filter((r) => !r.success).length,
		}

		return {
			success,
			results,
			summary,
			error,
		}
	}

	/**
	 * Create a file result object.
	 */
	private createFileResult(
		path: string,
		operation: "add" | "delete" | "update" | "rename",
		success: boolean,
		error?: string,
		errorCode?: PatchErrorCode,
		oldPath?: string,
		newPath?: string,
		diffStats?: { additions: number; deletions: number },
	): ApplyPatchFileResult {
		return {
			path,
			operation,
			success,
			error,
			errorCode,
			oldPath,
			newPath,
			diffStats,
		}
	}

	async execute(params: ApplyPatchParams, task: Task, callbacks: ToolCallbacks): Promise<void> {
		const { patch } = params
		const { askApproval, handleError, pushToolResult } = callbacks

		// All paths are relative to the workspace root
		const cwd = task.cwd

		try {
			// Validate required parameters using structured errors
			if (!patch) {
				task.didToolFailInCurrentTurn = true
				const error = new MissingParameterError("apply_patch", "patch")
				task.recordToolError("apply_patch", error.toLogEntry())
				const result = this.createResult(false, [], error.message)
				pushToolResult(JSON.stringify(result))
				return
			}

			// Parse the patch
			let parsedPatch
			try {
				parsedPatch = parsePatch(patch)
			} catch (error) {
				task.didToolFailInCurrentTurn = true
				const patchError = new PatchParseError(
					"apply_patch",
					error instanceof PatchErrorType ? error.message : `Failed to parse patch: ${error instanceof Error ? error.message : String(error)}`
				)
				task.recordToolError("apply_patch", patchError.toLogEntry())
				const result = this.createResult(
					false,
					[this.createFileResult("patch", "update", false, patchError.message, PatchErrorCode.INVALID_FORMAT)],
					patchError.message,
				)
				pushToolResult(JSON.stringify(result))
				return
			}

			if (parsedPatch.hunks.length === 0) {
				const result = this.createResult(
					true,
					[],
					"No file operations found in patch.",
				)
				pushToolResult(JSON.stringify(result))
				return
			}

			// Process each hunk
			const readFile = async (filePath: string): Promise<string> => {
				const absolutePath = path.resolve(cwd, filePath)
				return await fs.readFile(absolutePath, "utf8")
			}

			let changes: ApplyPatchFileChange[]
			try {
				changes = await processAllHunks(parsedPatch.hunks, readFile)
			} catch (error) {
				task.didToolFailInCurrentTurn = true
				const errorMessage = `Failed to process patch: ${error instanceof Error ? error.message : String(error)}`
				const patchError = new PatchParseError("apply_patch", errorMessage)
				task.recordToolError("apply_patch", patchError.toLogEntry())
				const result = this.createResult(
					false,
					[this.createFileResult("patch", "update", false, errorMessage, PatchErrorCode.HUNK_APPLY_FAILED)],
					undefined,
				)
				pushToolResult(JSON.stringify(result))
				return
			}

			// Use ToolExecutionResult for batch file processing
			interface FileOpSuccess {
				path: string
				operation: "add" | "delete" | "update" | "rename"
				diffStats?: { additions: number; deletions: number }
			}

			const toolResult = createMutableToolResult<FileOpSuccess>()
			const results: ApplyPatchFileResult[] = []

			for (const change of changes) {
				const relPath = change.path
				const absolutePath = path.resolve(cwd, relPath)

				try {
					// Check access permissions
					const accessAllowed = task.rooIgnoreController?.validateAccess(relPath)
					if (!accessAllowed) {
						await task.say("rooignore_error", relPath)
						const error = new RooIgnoreViolationError("apply_patch", relPath)
						toolResult.addError(error)
						results.push(this.createFileResult(relPath, change.type, false, error.message, PatchErrorCode.ROOIGNORE_VIOLATION))
						continue
					}

					// Check if file is write-protected
					const isWriteProtected = task.rooProtectedController?.isWriteProtected(relPath) || false

					if (change.type === "add") {
						const result = await this.handleAddFile(
							change,
							absolutePath,
							relPath,
							task,
							callbacks,
							isWriteProtected,
							cwd,
						)
						results.push(result)
						if (result.success) {
							toolResult.addSuccess({
								path: relPath,
								operation: "add",
								diffStats: result.diffStats,
							})
						} else if (result.error) {
							// Map error codes to ToolError types
							const toolError = this.mapToToolError(relPath, "add", result.error, result.errorCode)
							toolResult.addError(toolError)
						}
					} else if (change.type === "delete") {
						const result = await this.handleDeleteFile(
							absolutePath,
							relPath,
							task,
							callbacks,
							isWriteProtected,
						)
						results.push(result)
						if (result.success) {
							toolResult.addSuccess({ path: relPath, operation: "delete" })
						} else if (result.error) {
							const toolError = this.mapToToolError(relPath, "delete", result.error, result.errorCode)
							toolResult.addError(toolError)
						}
					} else if (change.type === "update") {
						const result = await this.handleUpdateFile(
							change,
							absolutePath,
							relPath,
							task,
							callbacks,
							isWriteProtected,
							cwd,
						)
						results.push(result)
						if (result.success) {
							toolResult.addSuccess({
								path: relPath,
								operation: result.operation,
								diffStats: result.diffStats,
							})
						} else if (result.error) {
							const toolError = this.mapToToolError(relPath, result.operation, result.error, result.errorCode)
							toolResult.addError(toolError)
						}
					}
				} catch (error) {
					// Capture error for this specific file
					const errorCode = error instanceof PatchErrorType ? error.code : PatchErrorCode.UNEXPECTED_ERROR
					const errorMessage = error instanceof Error ? error.message : String(error)
					results.push(
						this.createFileResult(relPath, change.type, false, errorMessage, errorCode),
					)
					const toolError = this.mapToToolError(relPath, change.type, errorMessage, errorCode)
					toolResult.addError(toolError)
				}
			}

			// Record all errors to telemetry using ToolExecutionResult
			if (toolResult.hasErrors()) {
				task.didToolFailInCurrentTurn = true
				toolResult.errors.forEach((e) => task.recordToolError("apply_patch", e.toLogEntry()))
			}

			// Check if all operations succeeded
			const allSucceeded = toolResult.hasSuccesses() && !toolResult.hasErrors()

			if (allSucceeded) {
				task.recordToolUsage("apply_patch")
			}

			// Build enhanced result with ToolExecutionResult summary
			const result = this.createResult(allSucceeded, results, undefined)
			pushToolResult(this.buildEnhancedResult(result, toolResult))
		} catch (error) {
			// Handle unexpected errors
			const errorMessage = error instanceof Error ? error.message : String(error)
			const result = this.createResult(
				false,
				[this.createFileResult("unknown", "update", false, errorMessage)],
				errorMessage,
			)
			pushToolResult(JSON.stringify(result))
			await handleError("apply patch", error as Error)
			await task.diffViewProvider.reset()
		}
	}

	/**
	 * Map error codes to appropriate ToolError types.
	 */
	private mapToToolError(
		path: string,
		operation: "add" | "delete" | "update" | "rename",
		errorMessage: string,
		errorCode?: PatchErrorCode
	): ToolError {
		switch (errorCode) {
			case PatchErrorCode.FILE_NOT_FOUND:
				return new FileNotFoundToolError("apply_patch", path)
			case PatchErrorCode.FILE_ALREADY_EXISTS:
				return new FileAlreadyExistsError("apply_patch", path)
			case PatchErrorCode.ROOIGNORE_VIOLATION:
				return new RooIgnoreViolationError("apply_patch", path)
			case PatchErrorCode.WRITE_PROTECTED:
			case PatchErrorCode.PERMISSION_DENIED:
				return new PermissionDeniedToolError("apply_patch", path, "write_protected")
			case PatchErrorCode.INVALID_FORMAT:
			case PatchErrorCode.HUNK_APPLY_FAILED:
				return new PatchParseError("apply_patch", errorMessage)
			default: {
				// Create a generic execution error
				const execError = new (class extends Error {
					toLLMMessage() {
						return {
							status: "error" as const,
							type: "execution_error",
							message: errorMessage,
							path,
						}
					}
					toLogEntry() {
						return {
							level: "error" as const,
							category: "tool_execution",
							tool: "apply_patch",
							message: errorMessage,
							path,
							timestamp: Date.now(),
						}
					}
				})(errorMessage)
				execError.name = "PatchExecutionError"
				return execError as unknown as ToolError
			}
		}
	}

	/**
	 * Build enhanced result JSON with ToolExecutionResult summary.
	 */
	private buildEnhancedResult(
		result: ApplyPatchResult,
		toolResult: ToolExecutionResult<{ path: string; operation: string; diffStats?: { additions: number; deletions: number } }>
	): string {
		// Include ToolExecutionResult summary in the response
		const enhancedResult = {
			...result,
			summary: {
				...result.summary,
				successRate: toolResult.successRate(),
			},
			// Add error report if there are errors
			...(toolResult.hasErrors() && {
				errorReport: toolResult.toLLMSummary(),
			}),
		}
		return JSON.stringify(enhancedResult)
	}

	private async handleAddFile(
		change: ApplyPatchFileChange,
		absolutePath: string,
		relPath: string,
		task: Task,
		callbacks: ToolCallbacks,
		isWriteProtected: boolean,
		cwd: string,
	): Promise<ApplyPatchFileResult> {
		const { askApproval, pushToolResult } = callbacks

		// Check if file already exists
		const fileExists = await fileExistsAtPath(absolutePath)
		if (fileExists) {
			task.didToolFailInCurrentTurn = true
			const errorMessage = `File already exists: ${relPath}. Use Update File instead.`
			await task.say("error", errorMessage)
			return this.createFileResult(
				relPath,
				"add",
				false,
				errorMessage,
				PatchErrorCode.FILE_ALREADY_EXISTS,
			)
		}

		// Create parent directories if they don't exist
		try {
			await createDirectoriesForFile(absolutePath, cwd)
		} catch (error) {
			task.didToolFailInCurrentTurn = true
			const errorDetails = error instanceof Error ? error.message : String(error)
			const errorMessage = `Failed to create directories for file: ${relPath}\n\n<error_details>\n${errorDetails}\n</error_details>`
			await task.say("error", errorMessage)
			return this.createFileResult(
				relPath,
				"add",
				false,
				errorMessage,
				PatchErrorCode.PARENT_DIR_CREATE_FAILED,
			)
		}

		const newContent = change.newContent || ""
		const isOutsideWorkspace = isPathOutsideWorkspace(absolutePath)

		// Initialize diff view for new file
		task.diffViewProvider.editType = "create"
		task.diffViewProvider.originalContent = undefined

		const diff = formatResponse.createPrettyPatch(relPath, "", newContent)

		// Check experiment settings
		const provider = task.providerRef.deref()
		const state = await provider?.configurationService.getState()
		const diagnosticsEnabled = state?.diagnosticsEnabled ?? true
		const writeDelayMs = state?.writeDelayMs ?? DEFAULT_WRITE_DELAY_MS
		const isPreventFocusDisruptionEnabled = experiments.isEnabled(
			state?.experiments ?? {},
			EXPERIMENT_IDS.PREVENT_FOCUS_DISRUPTION,
		)

		const sanitizedDiff = sanitizeUnifiedDiff(diff || "")
		const diffStats = computeDiffStats(sanitizedDiff) || undefined

		const sharedMessageProps: ClineSayTool = {
			tool: "appliedDiff",
			path: getReadablePath(cwd, relPath),
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
			if (!isPreventFocusDisruptionEnabled) {
				await task.diffViewProvider.revertChanges()
			}
			await task.diffViewProvider.reset()
			return this.createFileResult(relPath, "add", false, "Changes were rejected by the user.")
		}

		// Save the changes
		if (isPreventFocusDisruptionEnabled) {
			await task.diffViewProvider.saveDirectly(relPath, newContent, true, diagnosticsEnabled, writeDelayMs)
		} else {
			await task.diffViewProvider.saveChanges(diagnosticsEnabled, writeDelayMs)
		}

		// Track file edit operation
		await task.fileContextTracker.trackFileContext(relPath, "roo_edited" as RecordSource)
		task.didEditFile = true

		const message = await task.diffViewProvider.pushToolWriteResult(task, cwd, true)
		await task.diffViewProvider.reset()
		task.processQueuedMessages()

		const convertedDiffStats = diffStats ? { additions: diffStats.added, deletions: diffStats.removed } : undefined
		return this.createFileResult(relPath, "add", true, undefined, undefined, undefined, undefined, convertedDiffStats)
	}

	private async handleDeleteFile(
		absolutePath: string,
		relPath: string,
		task: Task,
		callbacks: ToolCallbacks,
		isWriteProtected: boolean,
	): Promise<ApplyPatchFileResult> {
		const { askApproval } = callbacks

		// Check if file exists
		const fileExists = await fileExistsAtPath(absolutePath)
		if (!fileExists) {
			task.didToolFailInCurrentTurn = true
			const errorMessage = `File not found: ${relPath}. Cannot delete a non-existent file.`
			await task.say("error", errorMessage)
			return this.createFileResult(
				relPath,
				"delete",
				false,
				errorMessage,
				PatchErrorCode.FILE_NOT_FOUND,
			)
		}

		const isOutsideWorkspace = isPathOutsideWorkspace(absolutePath)

		const sharedMessageProps: ClineSayTool = {
			tool: "appliedDiff",
			path: getReadablePath(task.cwd, relPath),
			diff: `File will be deleted: ${relPath}`,
			isOutsideWorkspace,
		}

		const completeMessage = JSON.stringify({
			...sharedMessageProps,
			content: `Delete file: ${relPath}`,
			isProtected: isWriteProtected,
		} satisfies ClineSayTool)

		const didApprove = await askApproval("tool", completeMessage, undefined, isWriteProtected)

		if (!didApprove) {
			return this.createFileResult(relPath, "delete", false, "Delete operation was rejected by the user.")
		}

		// Delete the file
		try {
			await fs.unlink(absolutePath)
		} catch (error) {
			const errorMessage = `Failed to delete file '${relPath}': ${error instanceof Error ? error.message : String(error)}`
			await task.say("error", errorMessage)
			return this.createFileResult(
				relPath,
				"delete",
				false,
				errorMessage,
				PatchErrorCode.DELETE_FAILED,
			)
		}

		task.didEditFile = true
		task.processQueuedMessages()
		return this.createFileResult(relPath, "delete", true)
	}

	private async handleUpdateFile(
		change: ApplyPatchFileChange,
		absolutePath: string,
		relPath: string,
		task: Task,
		callbacks: ToolCallbacks,
		isWriteProtected: boolean,
		cwd: string,
	): Promise<ApplyPatchFileResult> {
		const { askApproval } = callbacks

		// Check if file exists
		const fileExists = await fileExistsAtPath(absolutePath)
		if (!fileExists) {
			task.didToolFailInCurrentTurn = true
			const errorMessage = `File not found: ${relPath}. Cannot update a non-existent file.`
			await task.say("error", errorMessage)
			return this.createFileResult(
				relPath,
				"update",
				false,
				errorMessage,
				PatchErrorCode.FILE_NOT_FOUND,
			)
		}

		const originalContent = change.originalContent || ""
		const newContent = change.newContent || ""
		const isOutsideWorkspace = isPathOutsideWorkspace(absolutePath)

		// Initialize diff view
		task.diffViewProvider.editType = "modify"
		task.diffViewProvider.originalContent = originalContent

		// Generate and validate diff
		const diff = formatResponse.createPrettyPatch(relPath, originalContent, newContent)
		if (!diff) {
			await task.diffViewProvider.reset()
			return this.createFileResult(relPath, "update", true, undefined, undefined, undefined, undefined, { additions: 0, deletions: 0 })
		}

		// Check experiment settings
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

		const sharedMessageProps: ClineSayTool = {
			tool: "appliedDiff",
			path: getReadablePath(cwd, relPath),
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
			if (!isPreventFocusDisruptionEnabled) {
				await task.diffViewProvider.revertChanges()
			}
			await task.diffViewProvider.reset()
			return this.createFileResult(relPath, "update", false, "Changes were rejected by the user.")
		}

		// Handle file move if specified
		if (change.movePath) {
			const moveAbsolutePath = path.resolve(cwd, change.movePath)

			// Validate: destination doesn't already exist
			const destinationExists = await fileExistsAtPath(moveAbsolutePath)
			if (destinationExists) {
				const errorMessage = `Cannot rename: destination path already exists: ${change.movePath}`
				await task.say("error", errorMessage)
				await task.diffViewProvider.reset()
				return this.createFileResult(
					relPath,
					"rename",
					false,
					errorMessage,
					PatchErrorCode.DESTINATION_EXISTS,
					relPath,
					change.movePath,
				)
			}

			// Validate destination path access permissions
			const moveAccessAllowed = task.rooIgnoreController?.validateAccess(change.movePath)
			if (!moveAccessAllowed) {
				await task.say("rooignore_error", change.movePath)
				await task.diffViewProvider.reset()
				return this.createFileResult(
					relPath,
					"rename",
					false,
					undefined,
					PatchErrorCode.ROOIGNORE_VIOLATION,
					relPath,
					change.movePath,
				)
			}

			// Check if destination path is write-protected
			const isMovePathWriteProtected = task.rooProtectedController?.isWriteProtected(change.movePath) || false
			if (isMovePathWriteProtected) {
				task.didToolFailInCurrentTurn = true
				task.recordToolError("apply_patch")
				const errorMessage = `Cannot move file to write-protected path: ${change.movePath}`
				await task.say("error", errorMessage)
				await task.diffViewProvider.reset()
				return this.createFileResult(
					relPath,
					"rename",
					false,
					errorMessage,
					PatchErrorCode.WRITE_PROTECTED,
					relPath,
					change.movePath,
				)
			}

			// Check if destination path is outside workspace
			const isMoveOutsideWorkspace = isPathOutsideWorkspace(moveAbsolutePath)
			if (isMoveOutsideWorkspace) {
				task.didToolFailInCurrentTurn = true
				task.recordToolError("apply_patch")
				const errorMessage = `Cannot move file to path outside workspace: ${change.movePath}`
				await task.say("error", errorMessage)
				await task.diffViewProvider.reset()
				return this.createFileResult(
					relPath,
					"rename",
					false,
					errorMessage,
					PatchErrorCode.PERMISSION_DENIED,
					relPath,
					change.movePath,
				)
			}

			// Save new content to the new path
			if (isPreventFocusDisruptionEnabled) {
				await task.diffViewProvider.saveDirectly(
					change.movePath,
					newContent,
					false,
					diagnosticsEnabled,
					writeDelayMs,
				)
			} else {
				// Write to new path and delete old file
				const parentDir = path.dirname(moveAbsolutePath)
				await fs.mkdir(parentDir, { recursive: true })
				await fs.writeFile(moveAbsolutePath, newContent, "utf8")
			}

			// Delete the original file
			try {
				await fs.unlink(absolutePath)
			} catch (error) {
				console.error(`Failed to delete original file after move: ${error}`)
			}

			await task.fileContextTracker.trackFileContext(change.movePath, "roo_edited" as RecordSource)

			await task.diffViewProvider.reset()
			task.processQueuedMessages()
			const convertedDiffStats = diffStats ? { additions: diffStats.added, deletions: diffStats.removed } : undefined
			return this.createFileResult(
				relPath,
				"rename",
				true,
				undefined,
				undefined,
				relPath,
				change.movePath,
				convertedDiffStats,
			)
		} else {
			// Save changes to the same file
			if (isPreventFocusDisruptionEnabled) {
				await task.diffViewProvider.saveDirectly(relPath, newContent, false, diagnosticsEnabled, writeDelayMs)
			} else {
				await task.diffViewProvider.saveChanges(diagnosticsEnabled, writeDelayMs)
			}

			await task.fileContextTracker.trackFileContext(relPath, "roo_edited" as RecordSource)

			task.didEditFile = true

			const message = await task.diffViewProvider.pushToolWriteResult(task, cwd, false)
			await task.diffViewProvider.reset()
			task.processQueuedMessages()
			const convertedDiffStats = diffStats ? { additions: diffStats.added, deletions: diffStats.removed } : undefined
			return this.createFileResult(relPath, "update", true, undefined, undefined, undefined, undefined, convertedDiffStats)
		}
	}

	override async handlePartial(task: Task, block: ToolUse<"apply_patch">): Promise<void> {
		const patch: string | undefined = block.params.patch
		const candidateRelPath = this.extractFirstPathFromPatch(patch)
		const fallbackDisplayPath = path.basename(task.cwd) || "workspace"
		const resolvedRelPath = candidateRelPath ?? ""
		const absolutePath = path.resolve(task.cwd, resolvedRelPath)
		const displayPath = candidateRelPath ? getReadablePath(task.cwd, candidateRelPath) : fallbackDisplayPath

		let patchPreview: string | undefined
		if (patch) {
			// Show first few lines of the patch
			const lines = patch.split("\n").slice(0, 5)
			patchPreview = lines.join("\n") + (patch.split("\n").length > 5 ? "\n..." : "")
		}

		const sharedMessageProps: ClineSayTool = {
			tool: "appliedDiff",
			path: displayPath || path.basename(task.cwd) || "workspace",
			diff: patchPreview || "Parsing patch...",
			isOutsideWorkspace: isPathOutsideWorkspace(absolutePath),
		}

		await task.ask("tool", JSON.stringify(sharedMessageProps), block.partial).catch(() => {})
	}
}

export const applyPatchTool = new ApplyPatchTool()
