import * as path from "path"

import { type ClineSayTool } from "@coder/types"

import { Task } from "../task/Task"
import { formatResponse } from "../prompts/responses"
import { listFiles } from "../../services/glob/list-files"
import { getReadablePath } from "../../utils/path"
import { isPathOutsideWorkspace } from "../../utils/pathUtils"
import type { ToolUse } from "../../shared/tools"

import { BaseTool, ToolCallbacks } from "./core/BaseTool"
import {
	MissingParameterError,
	DirectoryNotFoundToolError,
	PermissionDeniedToolError,
	RooIgnoreViolationError,
} from "../errors/tools/index.js"

interface ListFilesParams {
	path: string
	recursive?: boolean
}

/**
 * ListFilesTool - List files in a directory.
 * 
 * Note: This tool currently supports single directory listing.
 * Future enhancement: Multi-directory listing could use ToolExecutionResult
 * to collect errors from each directory independently.
 * 
 * Example multi-directory implementation:
 * ```typescript
 * const result = createMutableToolResult<ListResult>()
 * for (const dirPath of directories) {
 *   try {
 *     const [files, didHitLimit] = await listFiles(dirPath, recursive, limit)
 *     result.addSuccess({ path: dirPath, files, didHitLimit })
 *   } catch (error) {
 *     result.addError(new DirectoryNotFoundToolError("list_files", dirPath))
 *   }
 * }
 * if (result.hasErrors()) {
 *   result.errors.forEach(e => task.recordToolError("list_files", e.toLogEntry()))
 * }
 * pushToolResult(result.toLLMReport())
 * ```
 */
export class ListFilesTool extends BaseTool<"list_files"> {
	readonly name = "list_files" as const

	async execute(params: ListFilesParams, task: Task, callbacks: ToolCallbacks): Promise<void> {
		const { path: relDirPath, recursive } = params
		const { askApproval, pushToolResult } = callbacks

		// Validate required parameters using structured errors
		if (!relDirPath) {
			const error = new MissingParameterError("list_files", "path")
			task.recordToolError("list_files", error.toLogEntry())
			task.didToolFailInCurrentTurn = true
			pushToolResult(formatResponse.toolErrorFromInstance(error.toLLMMessage()))
			return
		}

		const absolutePath = path.resolve(task.cwd, relDirPath)
		const isOutsideWorkspace = isPathOutsideWorkspace(absolutePath)

		// Check RooIgnore access
		const accessAllowed = task.rooIgnoreController?.validateAccess(relDirPath)
		if (!accessAllowed) {
			const error = new RooIgnoreViolationError("list_files", relDirPath)
			await task.say("rooignore_error", relDirPath)
			task.recordToolError("list_files", error.toLogEntry())
			task.didToolFailInCurrentTurn = true
			pushToolResult(formatResponse.toolErrorFromInstance(error.toLLMMessage()))
			return
		}

		try {
			const [files, didHitLimit] = await listFiles(absolutePath, recursive || false, 200)

			const result = formatResponse.formatFilesList(
				absolutePath,
				files,
				didHitLimit,
				task.rooIgnoreController,
				task.rooProtectedController,
			)

			const sharedMessageProps: ClineSayTool = {
				tool: !recursive ? "listFilesTopLevel" : "listFilesRecursive",
				path: getReadablePath(task.cwd, relDirPath),
				isOutsideWorkspace,
			}

			const completeMessage = JSON.stringify({ ...sharedMessageProps, content: result } satisfies ClineSayTool)
			const didApprove = await askApproval("tool", completeMessage)

			if (!didApprove) {
				return
			}

			pushToolResult(result)
		} catch (error) {
			// Map error to appropriate ToolError type
			const errorMessage = error instanceof Error ? error.message : String(error)
			const toolError = this.mapToToolError(relDirPath, errorMessage)
			task.recordToolError("list_files", toolError.toLogEntry())
			task.didToolFailInCurrentTurn = true
			pushToolResult(formatResponse.toolErrorFromInstance(toolError.toLLMMessage()))
		}
	}

	/**
	 * Map generic errors to appropriate ToolError types.
	 */
	private mapToToolError(path: string, errorMessage: string) {
		if (errorMessage.includes("ENOENT") || errorMessage.includes("no such file or directory")) {
			return new DirectoryNotFoundToolError("list_files", path)
		}
		if (errorMessage.includes("EACCES") || errorMessage.includes("permission denied")) {
			return new PermissionDeniedToolError("list_files", path, "system_permission")
		}
		// Default to generic error
		return new DirectoryNotFoundToolError("list_files", path)
	}

	override async handlePartial(task: Task, block: ToolUse<"list_files">): Promise<void> {
		const relDirPath: string | undefined = block.params.path
		const recursiveRaw: string | undefined = block.params.recursive
		const recursive = recursiveRaw?.toLowerCase() === "true"

		const absolutePath = relDirPath ? path.resolve(task.cwd, relDirPath) : task.cwd
		const isOutsideWorkspace = isPathOutsideWorkspace(absolutePath)

		const sharedMessageProps: ClineSayTool = {
			tool: !recursive ? "listFilesTopLevel" : "listFilesRecursive",
			path: getReadablePath(task.cwd, relDirPath ?? ""),
			isOutsideWorkspace,
		}

		const partialMessage = JSON.stringify({ ...sharedMessageProps, content: "" } satisfies ClineSayTool)
		await task.ask("tool", partialMessage, block.partial).catch(() => { })
	}
}

export const listFilesTool = new ListFilesTool()
