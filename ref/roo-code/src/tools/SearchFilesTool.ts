import path from "path"

import { type ClineSayTool } from "@coder/types"

import { Task } from "../task/Task"
import { formatResponse } from "../prompts/responses"
import { getReadablePath } from "../../utils/path"
import { isPathOutsideWorkspace } from "../../utils/pathUtils"
import { regexSearchFiles } from "../../services/ripgrep"
import type { ToolUse } from "../../shared/tools"

import { BaseTool, ToolCallbacks } from "./core/BaseTool"
import {
	MissingParameterError,
	DirectoryNotFoundToolError,
	PermissionDeniedToolError,
	RooIgnoreViolationError,
} from "../errors/tools/index.js"

interface SearchFilesParams {
	path: string
	regex: string
	file_pattern?: string | null
}

/**
 * SearchFilesTool - Search for files matching a regex pattern.
 * 
 * Note: This tool currently supports single directory search.
 * Future enhancement: Multi-directory search could use ToolExecutionResult
 * to collect errors from each directory independently.
 * 
 * Example multi-directory implementation:
 * ```typescript
 * const result = createMutableToolResult<SearchResult>()
 * for (const dirPath of directories) {
 *   try {
 *     const matches = await regexSearchFiles(cwd, dirPath, regex, pattern, rooIgnore)
 *     result.addSuccess({ path: dirPath, matches })
 *   } catch (error) {
 *     result.addError(new DirectoryNotFoundToolError("search_files", dirPath))
 *   }
 * }
 * if (result.hasErrors()) {
 *   result.errors.forEach(e => task.recordToolError("search_files", e.toLogEntry()))
 * }
 * pushToolResult(result.toLLMReport())
 * ```
 */
export class SearchFilesTool extends BaseTool<"search_files"> {
	readonly name = "search_files" as const

	async execute(params: SearchFilesParams, task: Task, callbacks: ToolCallbacks): Promise<void> {
		const { askApproval, handleError, pushToolResult } = callbacks

		const relDirPath = params.path
		const regex = params.regex
		const filePattern = params.file_pattern || undefined

		// Validate required parameters using structured errors
		if (!relDirPath) {
			const error = new MissingParameterError("search_files", "path")
			task.recordToolError("search_files", error.toLogEntry())
			task.didToolFailInCurrentTurn = true
			pushToolResult(formatResponse.toolErrorFromInstance(error.toLLMMessage()))
			return
		}

		if (!regex) {
			const error = new MissingParameterError("search_files", "regex")
			task.recordToolError("search_files", error.toLogEntry())
			task.didToolFailInCurrentTurn = true
			pushToolResult(formatResponse.toolErrorFromInstance(error.toLLMMessage()))
			return
		}

		const absolutePath = path.resolve(task.cwd, relDirPath)
		const isOutsideWorkspace = isPathOutsideWorkspace(absolutePath)

		// Check RooIgnore access
		const accessAllowed = task.rooIgnoreController?.validateAccess(relDirPath)
		if (!accessAllowed) {
			const error = new RooIgnoreViolationError("search_files", relDirPath)
			await task.say("rooignore_error", relDirPath)
			task.recordToolError("search_files", error.toLogEntry())
			task.didToolFailInCurrentTurn = true
			pushToolResult(formatResponse.toolErrorFromInstance(error.toLLMMessage()))
			return
		}

		const sharedMessageProps: ClineSayTool = {
			tool: "searchFiles",
			path: getReadablePath(task.cwd, relDirPath),
			regex: regex,
			filePattern: filePattern,
			isOutsideWorkspace,
		}

		try {
			const results = await regexSearchFiles(task.cwd, absolutePath, regex, filePattern, task.rooIgnoreController)

			const completeMessage = JSON.stringify({ ...sharedMessageProps, content: results } satisfies ClineSayTool)
			const didApprove = await askApproval("tool", completeMessage)

			if (!didApprove) {
				return
			}

			pushToolResult(results)
		} catch (error) {
			// Map error to appropriate ToolError type
			const errorMessage = error instanceof Error ? error.message : String(error)
			const toolError = this.mapToToolError(relDirPath, errorMessage)
			task.recordToolError("search_files", toolError.toLogEntry())
			task.didToolFailInCurrentTurn = true
			pushToolResult(formatResponse.toolErrorFromInstance(toolError.toLLMMessage()))
		}
	}

	/**
	 * Map generic errors to appropriate ToolError types.
	 */
	private mapToToolError(path: string, errorMessage: string) {
		if (errorMessage.includes("ENOENT") || errorMessage.includes("no such file or directory")) {
			return new DirectoryNotFoundToolError("search_files", path)
		}
		if (errorMessage.includes("EACCES") || errorMessage.includes("permission denied")) {
			return new PermissionDeniedToolError("search_files", path, "system_permission")
		}
		// Default to generic error
		return new DirectoryNotFoundToolError("search_files", path)
	}

	override async handlePartial(task: Task, block: ToolUse<"search_files">): Promise<void> {
		const relDirPath = block.params.path
		const regex = block.params.regex
		const filePattern = block.params.file_pattern

		const absolutePath = relDirPath ? path.resolve(task.cwd, relDirPath) : task.cwd
		const isOutsideWorkspace = isPathOutsideWorkspace(absolutePath)

		const sharedMessageProps: ClineSayTool = {
			tool: "searchFiles",
			path: getReadablePath(task.cwd, relDirPath ?? ""),
			regex: regex ?? "",
			filePattern: filePattern ?? "",
			isOutsideWorkspace,
		}

		const partialMessage = JSON.stringify({ ...sharedMessageProps, content: "" } satisfies ClineSayTool)
		await task.ask("tool", partialMessage, block.partial).catch(() => { })
	}
}

export const searchFilesTool = new SearchFilesTool()
