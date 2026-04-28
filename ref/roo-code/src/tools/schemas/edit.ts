import { z } from "zod"
import type OpenAI from "openai"

import { createOpenAITool } from "./base"

// ─── Schema Definitions ────────────────────────────────────────────────────────

/**
 * Schema for edit tool parameters.
 */
export const EditParamsSchema = z.object({
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
	replace_all: z
		.boolean()
		.optional()
		.describe(
			"If true, replace all occurrences of old_string in the file. If false or omitted, only replace the first occurrence (default: false).",
		),
})

// ─── Type Exports ──────────────────────────────────────────────────────────────

export type EditParams = z.infer<typeof EditParamsSchema>

// ─── Tool Creation ──────────────────────────────────────────────────────────────

const EDIT_DESCRIPTION = `Edit a file by searching for and replacing a specific string. This tool performs exact string matching and replacement.

Parameters:
- file_path: (required) The path of the file to edit (relative to the current workspace directory)
- old_string: (required) The exact string to search for and replace. This must match exactly in the file.
- new_string: (required) The new string to replace the old_string with. This will be inserted in place of old_string.
- replace_all: (optional) If true, replace all occurrences of old_string in the file. If false or omitted, only replace the first occurrence (default: false).

Example: Single replacement
{ "file_path": "src/app.ts", "old_string": "const x = 1;", "new_string": "const x = 2;" }

Example: Replace all occurrences
{ "file_path": "src/app.ts", "old_string": "console.log", "new_string": "logger.info", "replace_all": true }

Note: This tool performs exact matching. Ensure old_string matches exactly, including whitespace and case sensitivity. For more complex edits, consider using apply_patch or write_to_file.`

/**
 * Creates the edit tool definition.
 *
 * @returns Native tool definition for edit
 */
export function createEditTool(): OpenAI.Chat.ChatCompletionTool {
	return createOpenAITool({
		name: "edit",
		description: EDIT_DESCRIPTION,
		schema: EditParamsSchema,
		strict: true,
	})
}

/**
 * Default edit tool definition.
 */
export const editTool = createEditTool()
