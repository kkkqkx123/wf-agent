/**
 * Predefined Tool Descriptions
 *
 * This module exports all predefined tool description data.
 * Registration is handled by the unified registration module:
 * - registration/tool-descriptions-registration.ts
 */

// Stateless - Filesystem tools
export { READ_FILE_TOOL_DESCRIPTION } from "./stateless/filesystem/read-file/index.js";
export { WRITE_FILE_TOOL_DESCRIPTION } from "./stateless/filesystem/write-file/index.js";
export { APPLY_PATCH_TOOL_DESCRIPTION } from "./stateless/filesystem/apply-patch/index.js";
export { APPLY_DIFF_TOOL_DESCRIPTION } from "./stateless/filesystem/apply-diff/index.js";
export { EDIT_TOOL_DESCRIPTION } from "./stateless/filesystem/edit/index.js";
export { LIST_FILES_TOOL_DESCRIPTION } from "./stateless/filesystem/list-files/index.js";
export { GREP_TOOL_DESCRIPTION } from "./stateless/filesystem/grep/index.js";
export { GLOB_TOOL_DESCRIPTION } from "./stateless/filesystem/glob/index.js";

// Stateless - Shell tools
export { RUN_SHELL_TOOL_DESCRIPTION } from "./stateless/shell/run-shell/index.js";

// Knowledge tools
export { SKILL_TOOL_DESCRIPTION } from "./builtin/skill/index.js";
// Utility tools
export { UPDATE_TODO_LIST_TOOL_DESCRIPTION } from "./stateless/utility/update-todo-list/index.js";
// Integration tools
export { USE_MCP_TOOL_DESCRIPTION } from "./builtin/use-mcp/index.js";

// Stateful - Memory tools
export {
  RECORD_NOTE_TOOL_DESCRIPTION,
  RECALL_NOTES_TOOL_DESCRIPTION,
} from "./stateful/memory/session-note/index.js";

// Stateful - Shell tools
export {
  BACKEND_SHELL_TOOL_DESCRIPTION,
  SHELL_OUTPUT_TOOL_DESCRIPTION,
  SHELL_KILL_TOOL_DESCRIPTION,
} from "./stateful/shell/backend-shell/index.js";

// Builtin - Workflow tools
export { EXECUTE_WORKFLOW_TOOL_DESCRIPTION } from "./builtin/workflow/execute-workflow/index.js";
export { QUERY_WORKFLOW_STATUS_TOOL_DESCRIPTION } from "./builtin/workflow/query-workflow-status/index.js";
export { CANCEL_WORKFLOW_TOOL_DESCRIPTION } from "./builtin/workflow/cancel-workflow/index.js";
