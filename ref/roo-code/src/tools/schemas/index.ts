/**
 * Tool Schema Unified Export
 *
 * This module provides a unified interface for all tool schemas.
 * It exports:
 * - Individual tool schemas and types
 * - Tool registry for centralized access
 * - Utility functions for schema operations
 */

// ─── Base Utilities ──────────────────────────────────────────────────────────────

export {
	coerceOptionalNumber,
	coerceOptionalBoolean,
	coercedNumber,
	coercedBoolean,
	createOpenAITool,
	schemaToJsonSchema,
	safeParseArgs,
	type CreateToolOptions,
	type PartialParseOptions,
} from "./base"

// ─── Tool Schemas and Types ──────────────────────────────────────────────────────

// read_file
export {
	ReadFileParamsSchema,
	ReadFileModeSchema,
	IndentationParamsSchema,
	createReadFileTool,
	readFileTool,
	DEFAULT_LINE_LIMIT,
	MAX_LINE_LENGTH,
	DEFAULT_MAX_LEVELS,
	type ReadFileParams,
	type ReadFileMode,
	type IndentationParams,
	type ReadFileToolOptions,
} from "./read_file"

// write_to_file
export {
	WriteToFileParamsSchema,
	createWriteToFileTool,
	writeToFileTool,
	type WriteToFileParams,
} from "./write_to_file"

// execute_command
export {
	ExecuteCommandParamsSchema,
	createExecuteCommandTool,
	executeCommandTool,
	type ExecuteCommandParams,
} from "./execute_command"

// read_command_output
export {
	ReadCommandOutputParamsSchema,
	createReadCommandOutputTool,
	readCommandOutputTool,
	type ReadCommandOutputParams,
} from "./read_command_output"

// ask_followup_question
export {
	AskFollowupQuestionParamsSchema,
	FollowUpOptionSchema,
	createAskFollowupQuestionTool,
	askFollowupQuestionTool,
	type AskFollowupQuestionParams,
	type FollowUpOption,
} from "./ask_followup_question"

// attempt_completion
export {
	AttemptCompletionParamsSchema,
	createAttemptCompletionTool,
	attemptCompletionTool,
	type AttemptCompletionParams,
} from "./attempt_completion"

// codebase_search
export {
	CodebaseSearchParamsSchema,
	SearchQuerySchema,
	createCodebaseSearchTool,
	codebaseSearchTool,
	type CodebaseSearchParams,
	type SearchQuery,
} from "./codebase_search"

// list_files
export {
	ListFilesParamsSchema,
	createListFilesTool,
	listFilesTool,
	type ListFilesParams,
} from "./list_files"

// search_files
export {
	SearchFilesParamsSchema,
	createSearchFilesTool,
	searchFilesTool,
	type SearchFilesParams,
} from "./search_files"

// apply_diff
export {
	ApplyDiffParamsSchema,
	createApplyDiffTool,
	applyDiffTool,
	type ApplyDiffParams,
} from "./apply_diff"

// edit
export {
	EditParamsSchema,
	createEditTool,
	editTool,
	type EditParams,
} from "./edit"

// search_replace
export {
	SearchReplaceParamsSchema,
	createSearchReplaceTool,
	searchReplaceTool,
	type SearchReplaceParams,
} from "./search_replace"

// edit_file
export {
	EditFileParamsSchema,
	createEditFileTool,
	editFileTool,
	type EditFileParams,
} from "./edit_file"

// apply_patch
export {
	ApplyPatchParamsSchema,
	createApplyPatchTool,
	applyPatchTool,
	type ApplyPatchParams,
} from "./apply_patch"

// update_todo_list
export {
	UpdateTodoListParamsSchema,
	createUpdateTodoListTool,
	updateTodoListTool,
	type UpdateTodoListParams,
} from "./update_todo_list"

// use_mcp (unified MCP tool)
export {
	UseMcpParamsSchema,
	createUseMcpTool,
	useMcpTool,
	type UseMcpParams,
} from "./use_mcp"

// run_slash_command
export {
	RunSlashCommandParamsSchema,
	createRunSlashCommandTool,
	runSlashCommandTool,
	type RunSlashCommandParams,
} from "./run_slash_command"

// skill
export {
	SkillParamsSchema,
	createSkillTool,
	skillTool,
	type SkillParams,
} from "./skill"

// switch_mode
export {
	SwitchModeParamsSchema,
	createSwitchModeTool,
	switchModeTool,
	type SwitchModeParams,
} from "./switch_mode"

// new_task
export {
	NewTaskParamsSchema,
	createNewTaskTool,
	newTaskTool,
	type NewTaskParams,
} from "./new_task"

// generate_image
export {
	GenerateImageParamsSchema,
	createGenerateImageTool,
	generateImageTool,
	type GenerateImageParams,
} from "./generate_image"

// ─── Tool Registry ──────────────────────────────────────────────────────────────

export {
	ToolRegistry,
	getToolDefinition,
	getToolNames,
	getToolsByGroup,
	getAllNativeTools,
	isToolRegistered,
	getToolSchema,
	type ToolDefinition,
	type ToolSchemaFor,
	type ToolParamsFor,
	type ToolParamsMap,
} from "./registry"
