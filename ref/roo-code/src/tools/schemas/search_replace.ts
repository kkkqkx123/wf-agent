import { z } from "zod"
import type OpenAI from "openai"

import { createOpenAITool } from "./base"

// ─── Schema Definitions ────────────────────────────────────────────────────────

/**
 * Schema for search_replace tool parameters.
 */
export const SearchReplaceParamsSchema = z.object({
	file_path: z
		.string()
		.describe("The path of the file to edit (relative to the current workspace directory)"),
	old_string: z
		.string()
		.describe(
			"The exact string to search for and replace. This must match exactly in the file.",
		),
	new_string: z
		.string()
		.describe(
			"The new string to replace the old_string with. This will be inserted in place of old_string.",
		),
})

// ─── Type Exports ──────────────────────────────────────────────────────────────

export type SearchReplaceParams = z.infer<typeof SearchReplaceParamsSchema>

// ─── Tool Creation ──────────────────────────────────────────────────────────────

const SEARCH_REPLACE_DESCRIPTION = `Perform a single search and replace operation in a file. This tool performs exact string matching and replaces only the first occurrence.

Parameters:
- file_path: (required) The path of the file to edit (relative to the current workspace directory)
- old_string: (required) The exact string to search for and replace. This must match exactly in the file.
- new_string: (required) The new string to replace the old_string with. This will be inserted in place of old_string.

Example: Single replacement
{ "file_path": "src/app.ts", "old_string": "const x = 1;", "new_string": "const x = 2;" }

Note: This tool replaces only the first occurrence. To replace all occurrences, use the edit tool with replace_all=true. This tool performs exact matching. Ensure old_string matches exactly, including whitespace and case sensitivity.`

/**
 * Creates the search_replace tool definition.
 *
 * @returns Native tool definition for search_replace
 */
export function createSearchReplaceTool(): OpenAI.Chat.ChatCompletionTool {
	return createOpenAITool({
		name: "search_replace",
		description: SEARCH_REPLACE_DESCRIPTION,
		schema: SearchReplaceParamsSchema,
		strict: true,
	})
}

/**
 * Default search_replace tool definition.
 */
export const searchReplaceTool = createSearchReplaceTool()
