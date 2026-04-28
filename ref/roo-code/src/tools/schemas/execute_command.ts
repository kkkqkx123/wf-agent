import { z } from "zod"
import type OpenAI from "openai"

import { createOpenAITool } from "./base"

// ─── Schema Definitions ────────────────────────────────────────────────────────

/**
 * Schema for execute_command tool parameters.
 */
export const ExecuteCommandParamsSchema = z.object({
	command: z.string().describe("Shell command to execute"),
	cwd: z
		.string()
		.nullable()
		.describe("Optional working directory for the command, relative or absolute"),
})

// ─── Type Exports ──────────────────────────────────────────────────────────────

export type ExecuteCommandParams = z.infer<typeof ExecuteCommandParamsSchema>

// ─── Tool Creation ──────────────────────────────────────────────────────────────

const EXECUTE_COMMAND_DESCRIPTION = `Execute a CLI command on the system. You must provide a clear explanation of what the command does when using this tool. Prefer relative commands and paths that avoid location sensitivity for terminal consistency, e.g: "New-Item ./testdata/example.file", "dir ./examples/model1/data/yaml", or "go test ./cmd/front --config ./cmd/front/config.yml". Always use powershell format. Never use format that only supported by bash, like "head", "grep". Never use command that will cause suspend or only work with human, like "more".

Parameters:
- command: (required) The CLI command to execute. This should be valid for the current operating system. Ensure the command is properly formatted and does not contain any harmful instructions.
- cwd: (optional) The working directory to execute the command in

Example: Executing npm run dev
{ "command": "npm run dev", "cwd": null }`

/**
 * Creates the execute_command tool definition.
 *
 * @returns Native tool definition for execute_command
 */
export function createExecuteCommandTool(): OpenAI.Chat.ChatCompletionTool {
	return createOpenAITool({
		name: "execute_command",
		description: EXECUTE_COMMAND_DESCRIPTION,
		schema: ExecuteCommandParamsSchema,
		strict: true,
	})
}

/**
 * Default execute_command tool definition.
 */
export const executeCommandTool = createExecuteCommandTool()
