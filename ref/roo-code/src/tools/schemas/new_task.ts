import { z } from "zod"
import type OpenAI from "openai"

import { createOpenAITool } from "./base"

// ─── Schema Definitions ────────────────────────────────────────────────────────

/**
 * Schema for new_task tool parameters.
 */
export const NewTaskParamsSchema = z.object({
	mode: z
		.string()
		.describe("The mode to start the new task in (e.g., 'code', 'architect', 'ask')"),
	message: z
		.string()
		.describe("The task description or message for the new task"),
	todos: z
		.string()
		.optional()
		.describe("Optional initial todo list in markdown format with [ ], [x], [-] for statuses"),
})

// ─── Type Exports ──────────────────────────────────────────────────────────────

export type NewTaskParams = z.infer<typeof NewTaskParamsSchema>

// ─── Tool Creation ──────────────────────────────────────────────────────────────

const NEW_TASK_DESCRIPTION = `Start a new task with a fresh context. This tool allows you to begin a new task independently of the current task, useful for parallel work or starting fresh.

Parameters:
- mode: (required) The mode to start the new task in (e.g., 'code', 'architect', 'ask')
- message: (required) The task description or message for the new task
- todos: (optional) Optional initial todo list in markdown format with [ ], [x], [-] for statuses

Example: Starting a new coding task
{ "mode": "code", "message": "Create a new user authentication module", "todos": "[ ] Design authentication flow\\n[ ] Implement login\\n[ ] Implement logout\\n[ ] Add tests" }

Example: Starting a new planning task
{ "mode": "architect", "message": "Design the microservices architecture for the new project" }

Note: New tasks start with a clean slate and don't inherit context from the current task. Use this when you need to work on something completely different or want to start fresh.`

/**
 * Creates the new_task tool definition.
 *
 * @returns Native tool definition for new_task
 */
export function createNewTaskTool(): OpenAI.Chat.ChatCompletionTool {
	return createOpenAITool({
		name: "new_task",
		description: NEW_TASK_DESCRIPTION,
		schema: NewTaskParamsSchema,
		strict: true,
	})
}

/**
 * Default new_task tool definition.
 */
export const newTaskTool = createNewTaskTool()
