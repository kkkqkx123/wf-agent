import { z } from "zod"
import type OpenAI from "openai"

import { createOpenAITool } from "./base"

// ─── Schema Definitions ────────────────────────────────────────────────────────

/**
 * Schema for apply_diff tool parameters.
 */
export const ApplyDiffParamsSchema = z.object({
	path: z
		.string()
		.describe("The path of the file to edit (relative to the current workspace directory)"),
	diff: z
		.string()
		.describe(
			"The diff content to apply. Should follow unified diff format with context lines to uniquely identify the changes.",
		),
})

// ─── Type Exports ──────────────────────────────────────────────────────────────

export type ApplyDiffParams = z.infer<typeof ApplyDiffParamsSchema>

// ─── Tool Creation ──────────────────────────────────────────────────────────────

const APPLY_DIFF_DESCRIPTION = `Apply a diff to a file. This tool applies changes to a file using a unified diff format, which shows exactly what lines to remove and add with context to uniquely identify the changes.

Parameters:
- path: (required) The path of the file to edit (relative to the current workspace directory)
- diff: (required) The diff content to apply. Should follow unified diff format with context lines to uniquely identify the changes.

Example: Applying a simple diff
{ "path": "src/app.ts", "diff": "--- a/src/app.ts\\n+++ b/src/app.ts\\n@@ -10,7 +10,7 @@\\n function greet() {\\n-    return 'Hello';\\n+    return 'Hello, World!';\\n }" }

Note: The diff should include sufficient context (typically 3 lines) to uniquely identify the location of the change. Use this tool for precise edits when you know the exact content to change.`

/**
 * Creates the apply_diff tool definition.
 *
 * @returns Native tool definition for apply_diff
 */
export function createApplyDiffTool(): OpenAI.Chat.ChatCompletionTool {
	return createOpenAITool({
		name: "apply_diff",
		description: APPLY_DIFF_DESCRIPTION,
		schema: ApplyDiffParamsSchema,
		strict: true,
	})
}

/**
 * Default apply_diff tool definition.
 */
export const applyDiffTool = createApplyDiffTool()
