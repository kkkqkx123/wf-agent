import { z } from "zod"
import type OpenAI from "openai"

import { createOpenAITool } from "./base"

// ─── Schema Definitions ────────────────────────────────────────────────────────

/**
 * Schema for attempt_completion tool parameters.
 */
export const AttemptCompletionParamsSchema = z.object({
	result: z
		.string()
		.describe("Final result message to deliver to the user once the task is complete"),
})

// ─── Type Exports ──────────────────────────────────────────────────────────────

export type AttemptCompletionParams = z.infer<typeof AttemptCompletionParamsSchema>

// ─── Tool Creation ──────────────────────────────────────────────────────────────

const ATTEMPT_COMPLETION_DESCRIPTION = `Once you can confirm that the task is complete, use this tool to present the result of your work to the user. 

IMPORTANT NOTE: This tool CANNOT be used until you've confirmed from the user that any previous tool uses were successful. 

Parameters:
- result: (required) The result of the task. Formulate this result in a way that is final and does not require further input from the user. Don't end your result with questions or offers for further assistance.

Example: Completing after updating CSS
{ "result": "I've updated the CSS to use flexbox layout for better responsiveness" }`

/**
 * Creates the attempt_completion tool definition.
 *
 * @returns Native tool definition for attempt_completion
 */
export function createAttemptCompletionTool(): OpenAI.Chat.ChatCompletionTool {
	return createOpenAITool({
		name: "attempt_completion",
		description: ATTEMPT_COMPLETION_DESCRIPTION,
		schema: AttemptCompletionParamsSchema,
		strict: true,
	})
}

/**
 * Default attempt_completion tool definition.
 */
export const attemptCompletionTool = createAttemptCompletionTool()
