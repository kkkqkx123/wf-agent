/**
 * Tools Module - Unified Export and Registration
 *
 * This module provides:
 * - Centralized tool executor registry
 * - Automatic registration of all built-in tools
 * - Unified exports for tool-related types and utilities
 *
 * Usage:
 * ```typescript
 * import { toolExecutorRegistry } from "../tools"
 *
 * const executor = toolExecutorRegistry.get("read_file")
 * if (executor) {
 *   await executor.handle(task, block, callbacks)
 * }
 * ```
 */

// ─── Registry ────────────────────────────────────────────────────────────────

export { toolExecutorRegistry, type ToolCallbacks } from "./ToolExecutorRegistry"

// ─── Base Classes ─────────────────────────────────────────────────────────────

export { BaseTool } from "./core/BaseTool"

// ─── Schema Exports ───────────────────────────────────────────────────────────

export * from "./schemas"

// ─── Tool Executors ───────────────────────────────────────────────────────────
//
// Import all tool executors and register them with the registry.
// This ensures all built-in tools are available for dispatch.
// ──────────────────────────────────────────────────────────────────────────────

import { toolExecutorRegistry } from "./ToolExecutorRegistry"

// Import tool executors
import { readFileTool } from "./ReadFileTool"
import { writeToFileTool } from "./WriteToFileTool"
import { executeCommandTool } from "./ExecuteCommandTool"
import { readCommandOutputTool } from "./ReadCommandOutputTool"
import { listFilesTool } from "./ListFilesTool"
import { searchFilesTool } from "./SearchFilesTool"
import { codebaseSearchTool } from "./CodebaseSearchTool"
import { applyDiffTool } from "./ApplyDiffTool"
import { editTool } from "./EditTool"
import { searchReplaceTool } from "./SearchReplaceTool"
import { editFileTool } from "./EditFileTool"
import { applyPatchTool } from "./ApplyPatchTool"
import { askFollowupQuestionTool } from "./AskFollowupQuestionTool"
import { attemptCompletionTool } from "./AttemptCompletionTool"
import { updateTodoListTool } from "./UpdateTodoListTool"
import { switchModeTool } from "./SwitchModeTool"
import { newTaskTool } from "./NewTaskTool"
import { runSlashCommandTool } from "./RunSlashCommandTool"
import { skillTool } from "./SkillTool"
import { useMcpTool } from "./UseMcpTool"
import { generateImageTool } from "./GenerateImageTool"

// ─── Register All Built-in Tools ──────────────────────────────────────────────

// Read group
toolExecutorRegistry.register("read_file", readFileTool)
toolExecutorRegistry.register("list_files", listFilesTool)
toolExecutorRegistry.register("search_files", searchFilesTool)
toolExecutorRegistry.register("codebase_search", codebaseSearchTool)

// Edit group
toolExecutorRegistry.register("write_to_file", writeToFileTool)
//toolExecutorRegistry.register("apply_diff", applyDiffTool)
//toolExecutorRegistry.register("edit", editTool)
//toolExecutorRegistry.register("search_replace", searchReplaceTool)
//toolExecutorRegistry.register("edit_file", editFileTool)
toolExecutorRegistry.register("apply_patch", applyPatchTool)

// Command group
toolExecutorRegistry.register("execute_command", executeCommandTool)
toolExecutorRegistry.register("read_command_output", readCommandOutputTool)

// Modes group
toolExecutorRegistry.register("ask_followup_question", askFollowupQuestionTool)
toolExecutorRegistry.register("attempt_completion", attemptCompletionTool)
toolExecutorRegistry.register("update_todo_list", updateTodoListTool)
//toolExecutorRegistry.register("switch_mode", switchModeTool)
//toolExecutorRegistry.register("new_task", newTaskTool)
toolExecutorRegistry.register("run_slash_command", runSlashCommandTool)
//toolExecutorRegistry.register("skill", skillTool)

// MCP group
toolExecutorRegistry.register("use_mcp", useMcpTool)

// Image generation group
toolExecutorRegistry.register("generate_image", generateImageTool)

// ─── Re-export individual tools for backward compatibility ────────────────────

export { readFileTool } from "./ReadFileTool"
export { writeToFileTool } from "./WriteToFileTool"
export { executeCommandTool } from "./ExecuteCommandTool"
export { readCommandOutputTool } from "./ReadCommandOutputTool"
export { listFilesTool } from "./ListFilesTool"
export { searchFilesTool } from "./SearchFilesTool"
export { codebaseSearchTool } from "./CodebaseSearchTool"
export { applyDiffTool } from "./ApplyDiffTool"
export { editTool } from "./EditTool"
export { searchReplaceTool } from "./SearchReplaceTool"
export { editFileTool } from "./EditFileTool"
export { applyPatchTool } from "./ApplyPatchTool"
export { askFollowupQuestionTool } from "./AskFollowupQuestionTool"
export { attemptCompletionTool } from "./AttemptCompletionTool"
export { updateTodoListTool } from "./UpdateTodoListTool"
export { switchModeTool } from "./SwitchModeTool"
export { newTaskTool } from "./NewTaskTool"
export { runSlashCommandTool } from "./RunSlashCommandTool"
export { skillTool } from "./SkillTool"
export { useMcpTool } from "./UseMcpTool"
export { generateImageTool } from "./GenerateImageTool"
