import { z } from "zod"
import type OpenAI from "openai"

import { createOpenAITool } from "./base"

// ─── Schema Definitions ────────────────────────────────────────────────────────

/**
 * Schema for switch_mode tool parameters.
 */
export const SwitchModeParamsSchema = z.object({
	mode_slug: z
		.string()
		.describe("The slug of the mode to switch to (e.g., 'code', 'architect', 'ask')"),
	reason: z
		.string()
		.describe("The reason for switching to this mode. Explain why this mode is better suited for the current task."),
})

// ─── Type Exports ──────────────────────────────────────────────────────────────

export type SwitchModeParams = z.infer<typeof SwitchModeParamsSchema>

// ─── Tool Creation ──────────────────────────────────────────────────────────────

const SWITCH_MODE_DESCRIPTION = `Switch to a different mode for specialized task handling. Different modes have different capabilities and approaches to solving problems.

Parameters:
- mode_slug: (required) The slug of the mode to switch to (e.g., 'code', 'architect', 'ask')
- reason: (required) The reason for switching to this mode. Explain why this mode is better suited for the current task.

Available modes:
- 'code': Focuses on implementation, writing code, and technical tasks
- 'architect': Focuses on system design, architecture decisions, and high-level planning
- 'ask': Focuses on answering questions, providing explanations, and guidance

Example: Switching to architect mode
{ "mode_slug": "architect", "reason": "Need to design the overall system architecture before implementing" }

Example: Switching to code mode
{ "mode_slug": "code", "reason": "Ready to implement the designed architecture" }

Note: Mode switching allows you to leverage specialized approaches for different types of tasks. Choose the mode that best matches the current work needed.`

/**
 * Creates the switch_mode tool definition.
 *
 * @returns Native tool definition for switch_mode
 */
export function createSwitchModeTool(): OpenAI.Chat.ChatCompletionTool {
	return createOpenAITool({
		name: "switch_mode",
		description: SWITCH_MODE_DESCRIPTION,
		schema: SwitchModeParamsSchema,
		strict: true,
	})
}

/**
 * Default switch_mode tool definition.
 */
export const switchModeTool = createSwitchModeTool()
