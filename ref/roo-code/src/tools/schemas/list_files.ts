import { z } from "zod"
import type OpenAI from "openai"

import { createOpenAITool } from "./base"

// ─── Schema Definitions ────────────────────────────────────────────────────────

/**
 * Schema for list_files tool parameters.
 */
export const ListFilesParamsSchema = z.object({
	path: z
		.string()
		.describe("Directory path to inspect, relative to the workspace"),
	recursive: z
		.boolean()
		.optional()
		.describe("Set true to list contents recursively; false to show only the top level (default: false)"),
})

// ─── Type Exports ──────────────────────────────────────────────────────────────

export type ListFilesParams = z.infer<typeof ListFilesParamsSchema>

// ─── Tool Creation ──────────────────────────────────────────────────────────────

const LIST_FILES_DESCRIPTION = `list files and directories within the specified directory. If recursive is true, it will list all files and directories recursively.

Parameters:
- path: (required) The path of the directory to list contents for (relative to the current workspace directory)
- recursive: (optional) Whether to list files recursively. Use true for recursive listing, false for top-level only (default: false).

Example: Listing all files in the current directory (top-level only)
{ "path": ".", "recursive": false }

Example: Listing all files recursively in src directory
{ "path": "src", "recursive": true }

Example: Using default (top-level only)
{ "path": "src" }`

/**
 * Creates the list_files tool definition.
 *
 * @returns Native tool definition for list_files
 */
export function createListFilesTool(): OpenAI.Chat.ChatCompletionTool {
	return createOpenAITool({
		name: "list_files",
		description: LIST_FILES_DESCRIPTION,
		schema: ListFilesParamsSchema,
		strict: true,
	})
}

/**
 * Default list_files tool definition.
 */
export const listFilesTool = createListFilesTool()
