import type { ZodType } from "zod"
import type OpenAI from "openai"
import type { ToolName, ToolGroup } from "@coder/types"

import {
	createReadFileTool,
	ReadFileParamsSchema,
	type ReadFileParams,
	type ReadFileToolOptions,
} from "./read_file"
import {
	createWriteToFileTool,
	WriteToFileParamsSchema,
	type WriteToFileParams,
} from "./write_to_file"
import {
	createExecuteCommandTool,
	ExecuteCommandParamsSchema,
	type ExecuteCommandParams,
} from "./execute_command"
import {
	createAskFollowupQuestionTool,
	AskFollowupQuestionParamsSchema,
	type AskFollowupQuestionParams,
} from "./ask_followup_question"
import {
	createAttemptCompletionTool,
	AttemptCompletionParamsSchema,
	type AttemptCompletionParams,
} from "./attempt_completion"
import {
	createCodebaseSearchTool,
	CodebaseSearchParamsSchema,
	type CodebaseSearchParams,
} from "./codebase_search"
import {
	createListFilesTool,
	ListFilesParamsSchema,
	type ListFilesParams,
} from "./list_files"
import {
	createSearchFilesTool,
	SearchFilesParamsSchema,
	type SearchFilesParams,
} from "./search_files"
import {
	createApplyPatchTool,
	ApplyPatchParamsSchema,
	type ApplyPatchParams,
} from "./apply_patch"
import {
	createUpdateTodoListTool,
	UpdateTodoListParamsSchema,
	type UpdateTodoListParams,
} from "./update_todo_list"
import {
	createUseMcpTool,
	UseMcpParamsSchema,
	type UseMcpParams,
} from "./use_mcp"
import {
	createRunSlashCommandTool,
	RunSlashCommandParamsSchema,
	type RunSlashCommandParams,
} from "./run_slash_command"
import {
	createSkillTool,
	SkillParamsSchema,
	type SkillParams,
} from "./skill"
import {
	createReadCommandOutputTool,
	ReadCommandOutputParamsSchema,
	type ReadCommandOutputParams,
} from "./read_command_output"
import {
	createApplyDiffTool,
	ApplyDiffParamsSchema,
	type ApplyDiffParams,
} from "./apply_diff"
import {
	createEditTool,
	EditParamsSchema,
	type EditParams,
} from "./edit"
import {
	createSearchReplaceTool,
	SearchReplaceParamsSchema,
	type SearchReplaceParams,
} from "./search_replace"
import {
	createEditFileTool,
	EditFileParamsSchema,
	type EditFileParams,
} from "./edit_file"
import {
	createSwitchModeTool,
	SwitchModeParamsSchema,
	type SwitchModeParams,
} from "./switch_mode"
import {
	createNewTaskTool,
	NewTaskParamsSchema,
	type NewTaskParams,
} from "./new_task"
import {
	createGenerateImageTool,
	GenerateImageParamsSchema,
	type GenerateImageParams,
} from "./generate_image"

// ─── Tool Definition Interface ──────────────────────────────────────────────────

/**
 * Interface for a tool definition in the registry.
 */
export interface ToolDefinition<TSchema extends ZodType, TOptions = unknown> {
	/** Tool name */
	name: ToolName
	/** Zod schema for parameter validation */
	schema: TSchema
	/** Tool description */
	description: string
	/** Optional aliases for the tool */
	aliases?: string[]
	/** Tool group for categorization */
	group?: ToolGroup
	/** Factory function to create the OpenAI tool definition */
	createTool: (options?: TOptions) => OpenAI.Chat.ChatCompletionTool
}

// ─── Tool Registry ──────────────────────────────────────────────────────────────

/**
 * Tool registry that manages all native tool definitions.
 * Provides unified access to schemas, types, and OpenAI tool definitions.
 */
export const ToolRegistry = {
	read_file: {
		name: "read_file" as const,
		schema: ReadFileParamsSchema,
		description: "Read a file and return its contents with line numbers",
		group: "read" as ToolGroup,
		createTool: (options?: ReadFileToolOptions) => createReadFileTool(options),
	},

	write_to_file: {
		name: "write_to_file" as const,
		schema: WriteToFileParamsSchema,
		description: "Write content to create or overwrite a file",
		group: "edit" as ToolGroup,
		createTool: () => createWriteToFileTool(),
	},

	execute_command: {
		name: "execute_command" as const,
		schema: ExecuteCommandParamsSchema,
		description: "Execute a CLI command on the system",
		group: "command" as ToolGroup,
		createTool: () => createExecuteCommandTool(),
	},

	read_command_output: {
		name: "read_command_output" as const,
		schema: ReadCommandOutputParamsSchema,
		description: "Read the output of a previously executed command",
		group: "command" as ToolGroup,
		createTool: () => createReadCommandOutputTool(),
	},

	ask_followup_question: {
		name: "ask_followup_question" as const,
		schema: AskFollowupQuestionParamsSchema,
		description: "Ask the user a question to gather additional information",
		group: "modes" as ToolGroup,
		createTool: () => createAskFollowupQuestionTool(),
	},

	attempt_completion: {
		name: "attempt_completion" as const,
		schema: AttemptCompletionParamsSchema,
		description: "Present the result of completed work to the user",
		group: "modes" as ToolGroup,
		createTool: () => createAttemptCompletionTool(),
	},

	codebase_search: {
		name: "codebase_search" as const,
		schema: CodebaseSearchParamsSchema,
		description: "Semantic search to find relevant code based on meaning",
		group: "read" as ToolGroup,
		createTool: () => createCodebaseSearchTool(),
	},

	list_files: {
		name: "list_files" as const,
		schema: ListFilesParamsSchema,
		description: "List files and directories within a specified directory",
		group: "read" as ToolGroup,
		createTool: () => createListFilesTool(),
	},

	search_files: {
		name: "search_files" as const,
		schema: SearchFilesParamsSchema,
		description: "Perform a regex search across files in a directory",
		group: "read" as ToolGroup,
		createTool: () => createSearchFilesTool(),
	},

	apply_diff: {
		name: "apply_diff" as const,
		schema: ApplyDiffParamsSchema,
		description: "Apply a diff to a file using unified diff format",
		group: "edit" as ToolGroup,
		createTool: () => createApplyDiffTool(),
	},

	edit: {
		name: "edit" as const,
		schema: EditParamsSchema,
		description: "Edit a file by searching for and replacing a specific string",
		group: "edit" as ToolGroup,
		createTool: () => createEditTool(),
	},

	search_replace: {
		name: "search_replace" as const,
		schema: SearchReplaceParamsSchema,
		description: "Perform a single search and replace operation in a file",
		group: "edit" as ToolGroup,
		createTool: () => createSearchReplaceTool(),
	},

	edit_file: {
		name: "edit_file" as const,
		schema: EditFileParamsSchema,
		description: "Edit a file with optional validation of replacement count",
		group: "edit" as ToolGroup,
		createTool: () => createEditFileTool(),
	},

	apply_patch: {
		name: "apply_patch" as const,
		schema: ApplyPatchParamsSchema,
		description: "Apply patches to files for batch operations",
		group: "edit" as ToolGroup,
		createTool: () => createApplyPatchTool(),
	},

	update_todo_list: {
		name: "update_todo_list" as const,
		schema: UpdateTodoListParamsSchema,
		description: "Replace the entire TODO list with an updated checklist",
		group: "modes" as ToolGroup,
		createTool: () => createUpdateTodoListTool(),
	},

	use_mcp: {
		name: "use_mcp" as const,
		schema: UseMcpParamsSchema,
		description: "Use a capability (tool or resource) provided by a connected MCP server",
		group: "mcp" as ToolGroup,
		createTool: () => createUseMcpTool(),
	},

	run_slash_command: {
		name: "run_slash_command" as const,
		schema: RunSlashCommandParamsSchema,
		description: "Execute a slash command to get specific instructions",
		group: "modes" as ToolGroup,
		createTool: () => createRunSlashCommandTool(),
	},

	skill: {
		name: "skill" as const,
		schema: SkillParamsSchema,
		description: "Load and execute a skill by name",
		group: "modes" as ToolGroup,
		createTool: () => createSkillTool(),
	},

	switch_mode: {
		name: "switch_mode" as const,
		schema: SwitchModeParamsSchema,
		description: "Switch to a different mode for specialized task handling",
		group: "modes" as ToolGroup,
		createTool: () => createSwitchModeTool(),
	},

	new_task: {
		name: "new_task" as const,
		schema: NewTaskParamsSchema,
		description: "Start a new task with a fresh context",
		group: "modes" as ToolGroup,
		createTool: () => createNewTaskTool(),
	},

	generate_image: {
		name: "generate_image" as const,
		schema: GenerateImageParamsSchema,
		description: "Generate an image using AI image generation models",
		group: "edit" as ToolGroup,
		createTool: () => createGenerateImageTool(),
	},
} as const

// ─── Type Utilities ──────────────────────────────────────────────────────────────

/**
 * Get the schema type for a specific tool.
 */
export type ToolSchemaFor<TName extends keyof typeof ToolRegistry> =
	(typeof ToolRegistry)[TName]["schema"]

/**
 * Get the parameter type for a specific tool.
 */
export type ToolParamsFor<TName extends keyof typeof ToolRegistry> =
	TName extends keyof typeof ToolRegistry
	? (typeof ToolRegistry)[TName]["schema"] extends ZodType<infer T>
	? T
	: never
	: never

// ─── Registry Functions ──────────────────────────────────────────────────────────

/**
 * Get a tool definition by name.
 *
 * @param name - Tool name
 * @returns Tool definition or undefined if not found
 */
export function getToolDefinition<TName extends keyof typeof ToolRegistry>(
	name: TName,
): (typeof ToolRegistry)[TName] | undefined {
	return ToolRegistry[name]
}

/**
 * Get all registered tool names.
 *
 * @returns Array of tool names
 */
export function getToolNames(): (keyof typeof ToolRegistry)[] {
	return Object.keys(ToolRegistry) as (keyof typeof ToolRegistry)[]
}

/**
 * Get all tools in a specific group.
 *
 * @param group - Tool group
 * @returns Array of tool definitions in the group
 */
export function getToolsByGroup(group: ToolGroup): typeof ToolRegistry[keyof typeof ToolRegistry][] {
	return Object.values(ToolRegistry).filter((tool) => tool.group === group)
}

/**
 * Get all native tools as OpenAI tool definitions.
 *
 * @param options - Optional configuration for tool creation
 * @returns Array of OpenAI tool definitions
 */
export function getAllNativeTools(options?: {
	supportsImages?: boolean
}): OpenAI.Chat.ChatCompletionTool[] {
	const { supportsImages = false } = options || {}

	return Object.entries(ToolRegistry).map(([name, tool]) => {
		// Pass options to tools that support them
		if (name === "read_file") {
			return tool.createTool({ supportsImages })
		}
		return tool.createTool()
	})
}

/**
 * Check if a tool name is registered.
 *
 * @param name - Tool name to check
 * @returns True if the tool is registered
 */
export function isToolRegistered(name: string): name is keyof typeof ToolRegistry {
	return name in ToolRegistry
}

/**
 * Get the schema for a tool by name.
 *
 * @param name - Tool name
 * @returns Zod schema or undefined if not found
 */
export function getToolSchema<TName extends keyof typeof ToolRegistry>(
	name: TName,
): (typeof ToolRegistry)[TName]["schema"] | undefined {
	return ToolRegistry[name]?.schema
}

// ─── Parameter Type Map ─────────────────────────────────────────────────────────

/**
 * Map of tool names to their parameter types.
 * This can be used for type-safe parameter access.
 */
export interface ToolParamsMap {
	read_file: ReadFileParams
	write_to_file: WriteToFileParams
	execute_command: ExecuteCommandParams
	read_command_output: ReadCommandOutputParams
	ask_followup_question: AskFollowupQuestionParams
	attempt_completion: AttemptCompletionParams
	codebase_search: CodebaseSearchParams
	list_files: ListFilesParams
	search_files: SearchFilesParams
	apply_diff: ApplyDiffParams
	edit: EditParams
	search_replace: SearchReplaceParams
	edit_file: EditFileParams
	apply_patch: ApplyPatchParams
	update_todo_list: UpdateTodoListParams
	use_mcp: UseMcpParams
	run_slash_command: RunSlashCommandParams
	skill: SkillParams
	switch_mode: SwitchModeParams
	new_task: NewTaskParams
	generate_image: GenerateImageParams
}
