import { z } from "zod"
import type OpenAI from "openai"

import { createOpenAITool } from "./base"

// ─── Schema Definitions ────────────────────────────────────────────────────────

/**
 * Schema for search_files tool parameters.
 */
export const SearchFilesParamsSchema = z.object({
	path: z
		.string()
		.describe("Directory to search recursively, relative to the workspace"),
	regex: z
		.string()
		.describe("Rust-compatible regular expression pattern to match"),
	file_pattern: z
		.string()
		.nullable()
		.describe("Optional glob to limit which files are searched (e.g., *.ts)"),
})

// ─── Type Exports ──────────────────────────────────────────────────────────────

export type SearchFilesParams = z.infer<typeof SearchFilesParamsSchema>

// ─── Tool Creation ──────────────────────────────────────────────────────────────

const SEARCH_FILES_DESCRIPTION = `Request to perform a regex search across files in a specified directory, providing context-rich results. This tool searches for patterns or specific content across multiple files, displaying each match with encapsulating context.

Craft your regex patterns carefully to balance specificity and flexibility. Use this tool to find code patterns, TODO comments, function definitions, or any text-based information across the project. The results include surrounding context, so analyze the surrounding code to better understand the matches. Leverage this tool in combination with other tools for more comprehensive analysis.

**Important:** This tool automatically filters out files matching patterns in .gitignore and .rooignore to improve performance and avoid searching build artifacts. If you need to access build artifacts or other ignored files, use execute_command with your own scripts or commands to view them directly.

Parameters:
- path: (required) The path of the directory to search in (relative to the current workspace directory). This directory will be recursively searched.
- regex: (required) The regular expression pattern to search for. Uses Rust regex syntax.
- file_pattern: (optional) Glob pattern to filter files (e.g., '*.ts' for TypeScript files). If not provided, it will search all files (*).

Example: Searching for all .ts files in the current directory
{ "path": ".", "regex": ".*", "file_pattern": "*.ts" }

Example: Searching for function definitions in JavaScript files
{ "path": "src", "regex": "function\\s+\\w+", "file_pattern": "*.js" }`

/**
 * Creates the search_files tool definition.
 *
 * @returns Native tool definition for search_files
 */
export function createSearchFilesTool(): OpenAI.Chat.ChatCompletionTool {
	return createOpenAITool({
		name: "search_files",
		description: SEARCH_FILES_DESCRIPTION,
		schema: SearchFilesParamsSchema,
		strict: true,
	})
}

/**
 * Default search_files tool definition.
 */
export const searchFilesTool = createSearchFilesTool()
