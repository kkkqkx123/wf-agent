import { z } from "zod"
import type OpenAI from "openai"

import { createOpenAITool } from "./base"

// ─── Schema Definitions ────────────────────────────────────────────────────────

/**
 * Schema for skill tool parameters.
 */
export const SkillParamsSchema = z.object({
	skill: z
		.string()
		.describe("Name of the skill to load (e.g., create-mcp-server, create-mode). Must match a skill name from the available skills list."),
	args: z
		.string()
		.nullable()
		.describe("Optional context or arguments to pass to the skill"),
})

// ─── Type Exports ──────────────────────────────────────────────────────────────

export type SkillParams = z.infer<typeof SkillParamsSchema>

// ─── Tool Creation ──────────────────────────────────────────────────────────────

const SKILL_DESCRIPTION = `Load and execute a skill by name. Skills provide specialized instructions for common tasks like creating MCP servers or custom modes.

Use this tool when you need to follow specific procedures documented in a skill. Available skills are listed in the AVAILABLE SKILLS section of the system prompt.`

/**
 * Creates the skill tool definition.
 *
 * @returns Native tool definition for skill
 */
export function createSkillTool(): OpenAI.Chat.ChatCompletionTool {
	return createOpenAITool({
		name: "skill",
		description: SKILL_DESCRIPTION,
		schema: SkillParamsSchema,
		strict: true,
	})
}

/**
 * Default skill tool definition.
 */
export const skillTool = createSkillTool()
