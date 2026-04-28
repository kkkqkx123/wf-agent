import { z } from "zod"
import type OpenAI from "openai"

import { createOpenAITool } from "./base"

// ─── Schema Definitions ────────────────────────────────────────────────────────

/**
 * Schema for a follow-up option with optional mode switch.
 */
export const FollowUpOptionSchema = z.object({
	text: z.string().describe("Suggested answer the user can pick"),
	mode: z
		.string()
		.nullable()
		.describe("Optional mode slug to switch to if this suggestion is chosen (e.g., code, architect)"),
})

/**
 * Schema for ask_followup_question tool parameters.
 */
export const AskFollowupQuestionParamsSchema = z.object({
	question: z
		.string()
		.describe("Clear, specific question that captures the missing information you need"),
	follow_up: z
		.array(FollowUpOptionSchema)
		.min(1)
		.max(4)
		.describe(
			"Required list of 2-4 suggested responses; each suggestion must be a complete, actionable answer and may include a mode switch",
		),
})

// ─── Type Exports ──────────────────────────────────────────────────────────────

export type AskFollowupQuestionParams = z.infer<typeof AskFollowupQuestionParamsSchema>
export type FollowUpOption = z.infer<typeof FollowUpOptionSchema>

// ─── Tool Creation ──────────────────────────────────────────────────────────────

const ASK_FOLLOWUP_QUESTION_DESCRIPTION = `Ask the user a question to gather additional information needed to complete the task. Use when you need clarification or more details to proceed effectively.

Parameters:
- question: (required) A clear, specific question addressing the information needed
- follow_up: (required) A list of 2-4 suggested answers. Suggestions must be complete, actionable answers without placeholders. Optionally include mode to switch modes (code/architect/etc.)

Example: Asking for file path
{ "question": "What is the path to the frontend-config.json file?", "follow_up": [{ "text": "./src/frontend-config.json", "mode": null }, { "text": "./config/frontend-config.json", "mode": null }, { "text": "./frontend-config.json", "mode": null }] }

Example: Asking with mode switch
{ "question": "Would you like me to implement this feature?", "follow_up": [{ "text": "Yes, implement it now", "mode": "code" }, { "text": "No, just plan it out", "mode": "architect" }] }`

/**
 * Creates the ask_followup_question tool definition.
 *
 * @returns Native tool definition for ask_followup_question
 */
export function createAskFollowupQuestionTool(): OpenAI.Chat.ChatCompletionTool {
	return createOpenAITool({
		name: "ask_followup_question",
		description: ASK_FOLLOWUP_QUESTION_DESCRIPTION,
		schema: AskFollowupQuestionParamsSchema,
		strict: true,
	})
}

/**
 * Default ask_followup_question tool definition.
 */
export const askFollowupQuestionTool = createAskFollowupQuestionTool()
