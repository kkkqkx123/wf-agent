import { z } from "zod"
import type OpenAI from "openai"

import { createOpenAITool } from "./base"

// ─── Schema Definitions ────────────────────────────────────────────────────────

/**
 * Schema for apply_patch tool parameters.
 */
export const ApplyPatchParamsSchema = z.object({
	patch: z
		.string()
		.describe(
			"The complete patch text in the apply_patch format, starting with '*** Begin Patch' and ending with '*** End Patch'.",
		),
})

// ─── Type Exports ──────────────────────────────────────────────────────────────

export type ApplyPatchParams = z.infer<typeof ApplyPatchParamsSchema>

// ─── Tool Creation ──────────────────────────────────────────────────────────────

const APPLY_PATCH_DESCRIPTION = `Apply patches to files.
Best for batch operations, refactoring, and multi-file changes including creating, deleting, renaming, and updating multiple files in a single operation. Automatically creates parent directories when adding new files.

format:

*** Begin Patch
[ one or more file sections ]
*** End Patch

Each file section starts with one of three headers:
- *** Add File: <path> - Create a new file. Every following line is a + line (the initial contents).
- *** Delete File: <path> - Remove an existing file. Nothing follows.
- *** Update File: <path> - Patch an existing file in place.

For Update File operations:
- May be immediately followed by *** Move to: <new path> if you want to rename the file.
- Then one or more "hunks", each introduced by @@ (optionally followed by context like a class or function name).
- Within a hunk each line starts with:
  - ' ' (space) for context lines (unchanged)
  - '-' for lines to remove
  - '+' for lines to add
- May optionally end with *** End of File for clarity

Context guidelines:
- Show 3 lines of code above and below each change.
- If a change is within 3 lines of a previous change, do NOT duplicate context lines.
- Use @@ with a class/function name if 3 lines of context is insufficient to uniquely identify the location.

Example patch:
*** Begin Patch
*** Add File: hello.txt
+Hello world
+Second line
*** Update File: src/app.py
*** Move to: src/main.py
@@ def greet():
 def greet():
-    print("Hi")
+    print("Hello, world!")
    return
*** End of File
*** Update File: src/utils.py
@@
 import os
-import sys
+import sys, json
 def helper():
*** Delete File: obsolete.txt
*** End Patch`

/**
 * Creates the apply_patch tool definition.
 *
 * @returns Native tool definition for apply_patch
 */
export function createApplyPatchTool(): OpenAI.Chat.ChatCompletionTool {
	return createOpenAITool({
		name: "apply_patch",
		description: APPLY_PATCH_DESCRIPTION,
		schema: ApplyPatchParamsSchema,
		strict: true,
	})
}

/**
 * Default apply_patch tool definition.
 */
export const applyPatchTool = createApplyPatchTool()
