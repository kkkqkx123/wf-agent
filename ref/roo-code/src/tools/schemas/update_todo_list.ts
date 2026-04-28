import { z } from "zod"
import type OpenAI from "openai"

import { createOpenAITool } from "./base"

// ─── Schema Definitions ────────────────────────────────────────────────────────

/**
 * Schema for update_todo_list tool parameters.
 */
export const UpdateTodoListParamsSchema = z.object({
	todos: z
		.string()
		.describe(
			"Multi-line markdown checklist. Each line must be a separate todo item: [ ] for pending, [x] for completed, [-] for in progress.",
		),
})

// ─── Type Exports ──────────────────────────────────────────────────────────────

export type UpdateTodoListParams = z.infer<typeof UpdateTodoListParamsSchema>

// ─── Tool Creation ──────────────────────────────────────────────────────────────

const UPDATE_TODO_LIST_DESCRIPTION = `Replace the entire TODO list. Always provide the full list; the system will overwrite the previous one.

Format: Multi-line markdown checklist. Each line is one item:
[ ] pending task
[x] completed task
[-] in progress task`

/**
 * Creates the update_todo_list tool definition.
 *
 * @returns Native tool definition for update_todo_list
 */
export function createUpdateTodoListTool(): OpenAI.Chat.ChatCompletionTool {
	return createOpenAITool({
		name: "update_todo_list",
		description: UPDATE_TODO_LIST_DESCRIPTION,
		schema: UpdateTodoListParamsSchema,
		strict: true,
	})
}

/**
 * Default update_todo_list tool definition.
 */
export const updateTodoListTool = createUpdateTodoListTool()
