import { z } from "zod"
import type OpenAI from "openai"

import { createOpenAITool } from "./base"

// ─── Schema Definitions ────────────────────────────────────────────────────────

/**
 * Schema for edit_file tool parameters.
 */
export const EditFileParamsSchema = z.object({
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
	expected_replacements: z
		.number()
		.optional()
		.describe(
			"Optional expected number of replacements. If specified, the tool will verify that exactly this many replacements were made.",
		),
})

// ─── Type Exports ──────────────────────────────────────────────────────────────

export type EditFileParams = z.infer<typeof EditFileParamsSchema>

// ─── Tool Creation ──────────────────────────────────────────────────────────────

const EDIT_FILE_DESCRIPTION = `Edit a file by searching for and replacing a specific string. This tool performs exact string matching and replacement, with optional validation of the number of replacements.

Parameters:
- file_path: (required) The path of the file to edit (relative to the current workspace directory)
- old_string: (required) The exact string to search for and replace. This must match exactly in the file.
- new_string: (required) The new string to replace the old_string with. This will be inserted in place of old_string.
- expected_replacements: (optional) Optional expected number of replacements. If specified, the tool will verify that exactly this many replacements were made.

Example: Simple replacement
{ "file_path": "src/app.ts", "old_string": "const x = 1;", "new_string": "const x = 2;" }

Example: With expected replacements
{ "file_path": "src/app.ts", "old_string": "console.log", "new_string": "logger.info", "expected_replacements": 3 }

Note: This tool performs exact matching. Ensure old_string matches exactly, including whitespace and case sensitivity. Use expected_replacements when you know exactly how many times the string should appear, to catch unexpected duplicates.`

/**
 * Creates the edit_file tool definition.
 *
 * @returns Native tool definition for edit_file
 */
export function createEditFileTool(): OpenAI.Chat.ChatCompletionTool {
	return createOpenAITool({
		name: "edit_file",
		description: EDIT_FILE_DESCRIPTION,
		schema: EditFileParamsSchema,
		strict: true,
	})
}

/**
 * Default edit_file tool definition.
 */
export const editFileTool = createEditFileTool()
