/**
 * Predefined Tool Descriptions Registration
 *
 * This module exports all predefined tool descriptions and provides
 * a function to register them with the tool description registry.
 *
 * Usage:
 * ```ts
 * import { registerAllPredefinedToolDescriptions } from "./tool-descriptions.js";
 *
 * // Register all predefined tool descriptions
 * registerAllPredefinedToolDescriptions();
 * ```
 */

import { toolDescriptionRegistry } from "../../../core/utils/tools/tool-description-registry.js";

// Stateless - Filesystem tools
export { READ_FILE_TOOL_DESCRIPTION } from "./stateless/filesystem/read-file/index.js";
export { WRITE_FILE_TOOL_DESCRIPTION } from "./stateless/filesystem/write-file/index.js";
export { APPLY_PATCH_TOOL_DESCRIPTION } from "./stateless/filesystem/apply-patch/index.js";
export { APPLY_DIFF_TOOL_DESCRIPTION } from "./stateless/filesystem/apply-diff/index.js";
export { EDIT_TOOL_DESCRIPTION } from "./stateless/filesystem/edit/index.js";
export { LIST_FILES_TOOL_DESCRIPTION } from "./stateless/filesystem/list-files/index.js";
export { GREP_TOOL_DESCRIPTION } from "./stateless/filesystem/grep/index.js";

// Stateless - Shell tools
export { RUN_SHELL_TOOL_DESCRIPTION } from "./stateless/shell/run-shell/index.js";

// Stateless - Interaction tools
export { ASK_FOLLOWUP_QUESTION_TOOL_DESCRIPTION } from "./stateless/interaction/ask-followup-question/index.js";
export { RUN_SLASH_COMMAND_TOOL_DESCRIPTION } from "./stateless/interaction/run-slash-command/index.js";
export { SKILL_TOOL_DESCRIPTION } from "./stateless/interaction/skill/index.js";
export { UPDATE_TODO_LIST_TOOL_DESCRIPTION } from "./stateless/interaction/update-todo-list/index.js";
export { USE_MCP_TOOL_DESCRIPTION } from "./stateless/interaction/use-mcp/index.js";

// Stateful - Memory tools
export { RECORD_NOTE_TOOL_DESCRIPTION, RECALL_NOTES_TOOL_DESCRIPTION } from "./stateful/memory/session-note/index.js";

// Stateful - Shell tools
export { BACKEND_SHELL_TOOL_DESCRIPTION, SHELL_OUTPUT_TOOL_DESCRIPTION, SHELL_KILL_TOOL_DESCRIPTION } from "./stateful/shell/backend-shell/index.js";

// Builtin - Workflow tools
export { EXECUTE_WORKFLOW_TOOL_DESCRIPTION } from "./builtin/workflow/execute-workflow/index.js";
export { QUERY_WORKFLOW_STATUS_TOOL_DESCRIPTION } from "./builtin/workflow/query-workflow-status/index.js";
export { CANCEL_WORKFLOW_TOOL_DESCRIPTION } from "./builtin/workflow/cancel-workflow/index.js";

// Import all descriptions for registration
import { READ_FILE_TOOL_DESCRIPTION } from "./stateless/filesystem/read-file/index.js";
import { WRITE_FILE_TOOL_DESCRIPTION } from "./stateless/filesystem/write-file/index.js";
import { APPLY_PATCH_TOOL_DESCRIPTION } from "./stateless/filesystem/apply-patch/index.js";
import { APPLY_DIFF_TOOL_DESCRIPTION } from "./stateless/filesystem/apply-diff/index.js";
import { EDIT_TOOL_DESCRIPTION } from "./stateless/filesystem/edit/index.js";
import { LIST_FILES_TOOL_DESCRIPTION } from "./stateless/filesystem/list-files/index.js";
import { GREP_TOOL_DESCRIPTION } from "./stateless/filesystem/grep/index.js";
import { RUN_SHELL_TOOL_DESCRIPTION } from "./stateless/shell/run-shell/index.js";
import { ASK_FOLLOWUP_QUESTION_TOOL_DESCRIPTION } from "./stateless/interaction/ask-followup-question/index.js";
import { RUN_SLASH_COMMAND_TOOL_DESCRIPTION } from "./stateless/interaction/run-slash-command/index.js";
import { SKILL_TOOL_DESCRIPTION } from "./stateless/interaction/skill/index.js";
import { UPDATE_TODO_LIST_TOOL_DESCRIPTION } from "./stateless/interaction/update-todo-list/index.js";
import { USE_MCP_TOOL_DESCRIPTION } from "./stateless/interaction/use-mcp/index.js";
import { RECORD_NOTE_TOOL_DESCRIPTION, RECALL_NOTES_TOOL_DESCRIPTION } from "./stateful/memory/session-note/index.js";
import { BACKEND_SHELL_TOOL_DESCRIPTION, SHELL_OUTPUT_TOOL_DESCRIPTION, SHELL_KILL_TOOL_DESCRIPTION } from "./stateful/shell/backend-shell/index.js";
import { EXECUTE_WORKFLOW_TOOL_DESCRIPTION } from "./builtin/workflow/execute-workflow/index.js";
import { QUERY_WORKFLOW_STATUS_TOOL_DESCRIPTION } from "./builtin/workflow/query-workflow-status/index.js";
import { CANCEL_WORKFLOW_TOOL_DESCRIPTION } from "./builtin/workflow/cancel-workflow/index.js";

/**
 * All predefined tool descriptions
 */
export const ALL_PREDEFINED_TOOL_DESCRIPTIONS = [
  // Filesystem tools
  READ_FILE_TOOL_DESCRIPTION,
  WRITE_FILE_TOOL_DESCRIPTION,
  APPLY_PATCH_TOOL_DESCRIPTION,
  APPLY_DIFF_TOOL_DESCRIPTION,
  EDIT_TOOL_DESCRIPTION,
  LIST_FILES_TOOL_DESCRIPTION,
  GREP_TOOL_DESCRIPTION,

  // Shell tools
  RUN_SHELL_TOOL_DESCRIPTION,

  // Interaction tools
  ASK_FOLLOWUP_QUESTION_TOOL_DESCRIPTION,
  RUN_SLASH_COMMAND_TOOL_DESCRIPTION,
  SKILL_TOOL_DESCRIPTION,
  UPDATE_TODO_LIST_TOOL_DESCRIPTION,
  USE_MCP_TOOL_DESCRIPTION,

  // Stateful - Memory tools
  RECORD_NOTE_TOOL_DESCRIPTION,
  RECALL_NOTES_TOOL_DESCRIPTION,

  // Stateful - Shell tools
  BACKEND_SHELL_TOOL_DESCRIPTION,
  SHELL_OUTPUT_TOOL_DESCRIPTION,
  SHELL_KILL_TOOL_DESCRIPTION,

  // Builtin - Workflow tools
  EXECUTE_WORKFLOW_TOOL_DESCRIPTION,
  QUERY_WORKFLOW_STATUS_TOOL_DESCRIPTION,
  CANCEL_WORKFLOW_TOOL_DESCRIPTION,
];

/**
 * Register all predefined tool descriptions to the registry
 *
 * This function should be called once during application initialization,
 * before generating tool descriptions for system prompts.
 */
export function registerAllPredefinedToolDescriptions(): void {
  toolDescriptionRegistry.registerAll(ALL_PREDEFINED_TOOL_DESCRIPTIONS);
}

/**
 * Check if all predefined tool descriptions have been registered
 */
export function arePredefinedToolDescriptionsRegistered(): boolean {
  return ALL_PREDEFINED_TOOL_DESCRIPTIONS.every(desc => toolDescriptionRegistry.has(desc.id));
}

/**
 * Initialize tool descriptions (convenience function)
 *
 * Alias for registerAllPredefinedToolDescriptions()
 */
export function initializeToolDescriptions(): void {
  registerAllPredefinedToolDescriptions();
}
