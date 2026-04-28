import { z } from "zod"
import type OpenAI from "openai"

import { createOpenAITool } from "./base"

// ─── Schema Definitions ────────────────────────────────────────────────────────

/**
 * Schema for run_slash_command tool parameters.
 */
export const RunSlashCommandParamsSchema = z.object({
	command: z
		.string()
		.describe("Name of the slash command to run (e.g., init, test, deploy)"),
	args: z
		.string()
		.nullable()
		.describe("Optional additional context or arguments for the command"),
})

// ─── Type Exports ──────────────────────────────────────────────────────────────

export type RunSlashCommandParams = z.infer<typeof RunSlashCommandParamsSchema>

// ─── Tool Creation ──────────────────────────────────────────────────────────────

const RUN_SLASH_COMMAND_DESCRIPTION = `Execute a slash command to get specific instructions or content. Slash commands are predefined templates that provide detailed guidance for common tasks.`

/**
 * Creates the run_slash_command tool definition.
 *
 * @returns Native tool definition for run_slash_command
 */
export function createRunSlashCommandTool(): OpenAI.Chat.ChatCompletionTool {
	return createOpenAITool({
		name: "run_slash_command",
		description: RUN_SLASH_COMMAND_DESCRIPTION,
		schema: RunSlashCommandParamsSchema,
		strict: true,
	})
}

/**
 * Default run_slash_command tool definition.
 */
export const runSlashCommandTool = createRunSlashCommandTool()
