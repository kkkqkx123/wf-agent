import { z } from "zod"
import type OpenAI from "openai"

import { createOpenAITool } from "./base"

// ─── Schema Definitions ────────────────────────────────────────────────────────

/**
 * Schema for read_command_output tool parameters.
 */
export const ReadCommandOutputParamsSchema = z.object({
	artifact_id: z
		.string()
		.describe("The artifact ID of the command output to read"),
	search: z
		.string()
		.optional()
		.describe("Optional search pattern to filter the output (e.g., grep-like search)"),
	offset: z
		.number()
		.optional()
		.describe("1-based line offset to start reading from (default: 1)"),
	limit: z
		.number()
		.optional()
		.describe("Maximum number of lines to return (default: 2000)"),
})

// ─── Type Exports ──────────────────────────────────────────────────────────────

export type ReadCommandOutputParams = z.infer<typeof ReadCommandOutputParamsSchema>

// ─── Tool Creation ──────────────────────────────────────────────────────────────

const READ_COMMAND_OUTPUT_DESCRIPTION = `Read the output of a previously executed command. This tool allows you to retrieve command output that was saved as an artifact.

Parameters:
- artifact_id: (required) The artifact ID of the command output to read
- search: (optional) Optional search pattern to filter the output (e.g., grep-like search)
- offset: (optional) 1-based line offset to start reading from (default: 1)
- limit: (optional) Maximum number of lines to return (default: 2000)

Example: Reading command output
{ "artifact_id": "cmd-123", "offset": 1, "limit": 100 }

Example: Reading with search filter
{ "artifact_id": "cmd-123", "search": "error", "limit": 50 }`

/**
 * Creates the read_command_output tool definition.
 *
 * @returns Native tool definition for read_command_output
 */
export function createReadCommandOutputTool(): OpenAI.Chat.ChatCompletionTool {
	return createOpenAITool({
		name: "read_command_output",
		description: READ_COMMAND_OUTPUT_DESCRIPTION,
		schema: ReadCommandOutputParamsSchema,
		strict: true,
	})
}

/**
 * Default read_command_output tool definition.
 */
export const readCommandOutputTool = createReadCommandOutputTool()
