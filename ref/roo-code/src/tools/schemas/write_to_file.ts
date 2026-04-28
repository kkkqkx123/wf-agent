import { z } from "zod"
import type OpenAI from "openai"

import { createOpenAITool } from "./base"

// ─── Schema Definitions ────────────────────────────────────────────────────────

/**
 * Schema for write_to_file tool parameters.
 */
export const WriteToFileParamsSchema = z.object({
	path: z
		.string()
		.describe("The path of the file to write to (relative to the current workspace directory)"),
	content: z
		.string()
		.describe(
			"The content to write to the file. ALWAYS provide the COMPLETE intended content of the file, without any truncation or omissions. You MUST include ALL parts of the file, even if they haven't been modified. Do NOT include line numbers in the content.",
		),
})

// ─── Type Exports ──────────────────────────────────────────────────────────────

export type WriteToFileParams = z.infer<typeof WriteToFileParamsSchema>

// ─── Tool Creation ──────────────────────────────────────────────────────────────

const WRITE_TO_FILE_DESCRIPTION = `Write content to create or overwrite a file. This tool is primarily used for creating new files or a complete rewrite of an existing file. This tool will automatically create any directories needed to write the file.

When using this tool, use it directly with the desired content. You do not need to display the content before using the tool. ALWAYS provide the COMPLETE file content in your response. This is NON-NEGOTIABLE. Partial updates or placeholders like '// rest of code unchanged' are STRICTLY FORBIDDEN. Failure to do so will result in incomplete or broken code.

Example: Writing a configuration file
{ "path": "frontend-config.json", "content": "{\\n  \\"apiEndpoint\\": \\"https://api.example.com\\",\\n  \\"theme\\": {\\n    \\"primaryColor\\": \\"#007bff\\"\\n  }\\n}" }`

/**
 * Creates the write_to_file tool definition.
 *
 * @returns Native tool definition for write_to_file
 */
export function createWriteToFileTool(): OpenAI.Chat.ChatCompletionTool {
	return createOpenAITool({
		name: "write_to_file",
		description: WRITE_TO_FILE_DESCRIPTION,
		schema: WriteToFileParamsSchema,
		strict: true,
	})
}

/**
 * Default write_to_file tool definition.
 */
export const writeToFileTool = createWriteToFileTool()
