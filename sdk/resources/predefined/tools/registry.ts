/**
 * Predefined Tool Registry
 *
 * Responsible for creating and registering predefined tools with the tool service.
 */

import type { ToolDefinitionLike } from "@sdk/services/executors/tools/utils.js";
import type { Tool } from "@wf-agent/types";
import { renderToolDescription } from "../prompt-templates/tool-description-renderer.js";
import type { PredefinedToolsOptions } from "./types.js";

// Import a stateless tool
import {
  readFileSchema,
  createReadFileHandler,
  READ_FILE_TOOL_DESCRIPTION,
} from "./stateless/filesystem/read-file/index.js";
import {
  writeFileSchema,
  createWriteFileHandler,
  WRITE_FILE_TOOL_DESCRIPTION,
} from "./stateless/filesystem/write-file/index.js";
import {
  runShellSchema,
  createRunShellHandler,
  RUN_SHELL_TOOL_DESCRIPTION,
} from "./stateless/shell/run-shell/index.js";

// Import new filesystem tools
import {
  applyPatchSchema,
  createApplyPatchHandler,
  APPLY_PATCH_TOOL_DESCRIPTION,
} from "./stateless/filesystem/apply-patch/index.js";
import {
  applyDiffSchema,
  createApplyDiffHandler,
  APPLY_DIFF_TOOL_DESCRIPTION,
} from "./stateless/filesystem/apply-diff/index.js";
import {
  editSchema,
  createEditHandler,
  EDIT_TOOL_DESCRIPTION,
} from "./stateless/filesystem/edit/index.js";
import {
  listFilesSchema,
  createListFilesHandler,
  LIST_FILES_TOOL_DESCRIPTION,
} from "./stateless/filesystem/list-files/index.js";
import {
  grepSchema,
  createGrepHandler,
  GREP_TOOL_DESCRIPTION,
} from "./stateless/filesystem/grep/index.js";

// Import interaction tools
import {
  skillSchema,
  createSkillHandler,
  SKILL_TOOL_DESCRIPTION,
} from "./builtin/interaction/skill/index.js";
import {
  updateTodoListSchema,
  createUpdateTodoListHandler,
  UPDATE_TODO_LIST_TOOL_DESCRIPTION,
} from "./stateless/interaction/update-todo-list/index.js";
import {
  useMcpSchema,
  createLazyUseMcpHandler,
  USE_MCP_TOOL_DESCRIPTION,
} from "./builtin/interaction/use-mcp/index.js";
import { getMcpManager } from "../../../services/mcp/server-registry.js";

// Importing a stateful tool
import {
  recordNoteSchema,
  recallNotesSchema,
  createRecordNoteFactory,
  createRecallNotesFactory,
  RECORD_NOTE_TOOL_DESCRIPTION,
  RECALL_NOTES_TOOL_DESCRIPTION,
} from "./stateful/memory/session-note/index.js";
import {
  backendShellSchema,
  shellOutputSchema,
  shellKillSchema,
  createBackendShellFactory,
  createShellOutputFactory,
  createShellKillFactory,
  BACKEND_SHELL_TOOL_DESCRIPTION,
  SHELL_OUTPUT_TOOL_DESCRIPTION,
  SHELL_KILL_TOOL_DESCRIPTION,
} from "./stateful/shell/backend-shell/index.js";

// Import builtin tools
import { createBuiltinTools } from "./builtin/index.js";
import { DEFAULT_SHELL_POLICY } from "../../../services/sandbox/default-policy.js";

/**
 * Check if the tool is disabled.
 */
function isDisabled(toolId: string, options?: PredefinedToolsOptions): boolean {
  if (!options) return false;

  // If a whitelist is set, only the tools listed in the whitelist will be enabled.
  if (options.allowList && options.allowList.length > 0) {
    return !options.allowList.includes(toolId);
  }

  // If a blacklist is set, the tools listed in the blacklist will be disabled.
  if (options.blockList && options.blockList.length > 0) {
    return options.blockList.includes(toolId);
  }

  return false;
}

/**
 * Create a list of predefined tool definitions
 */
export function createPredefinedTools(options?: PredefinedToolsOptions): ToolDefinitionLike[] {
  const tools: ToolDefinitionLike[] = [];
  const config = options?.config;

  // read_file
  if (!isDisabled("read_file", options)) {
    tools.push({
      id: "read_file",
      type: "STATELESS",
      description: renderToolDescription(READ_FILE_TOOL_DESCRIPTION),
      parameters: readFileSchema,
      execute: createReadFileHandler(config?.readFile ?? { workspaceDir: process.cwd() }),
    });
  }

  // write_file
  if (!isDisabled("write_file", options)) {
    tools.push({
      id: "write_file",
      type: "STATELESS",
      description: renderToolDescription(WRITE_FILE_TOOL_DESCRIPTION),
      parameters: writeFileSchema,
      execute: createWriteFileHandler(config?.writeFile ?? { workspaceDir: process.cwd() }),
    });
  }

  // run_shell
  if (!isDisabled("run_shell", options)) {
    tools.push({
      id: "run_shell",
      type: "STATELESS",
      description: renderToolDescription(RUN_SHELL_TOOL_DESCRIPTION),
      parameters: runShellSchema,
      execute: createRunShellHandler({
        ...config?.runShell,
        shellPolicy: config?.runShell?.shellPolicy ?? DEFAULT_SHELL_POLICY,
      }),
    });
  }

  // record_note
  if (!isDisabled("record_note", options)) {
    tools.push({
      id: "record_note",
      type: "STATEFUL",
      description: renderToolDescription(RECORD_NOTE_TOOL_DESCRIPTION),
      parameters: recordNoteSchema,
      factory: createRecordNoteFactory(
        config?.sessionNote ?? { workspaceDir: process.cwd(), memoryFile: ".session-notes.json" },
      ),
    });
  }

  // recall_notes
  if (!isDisabled("recall_notes", options)) {
    tools.push({
      id: "recall_notes",
      type: "STATEFUL",
      description: renderToolDescription(RECALL_NOTES_TOOL_DESCRIPTION),
      parameters: recallNotesSchema,
      factory: createRecallNotesFactory(
        config?.sessionNote ?? { workspaceDir: process.cwd(), memoryFile: ".session-notes.json" },
      ),
    });
  }

  // backend_shell
  if (!isDisabled("backend_shell", options)) {
    tools.push({
      id: "backend_shell",
      type: "STATEFUL",
      description: renderToolDescription(BACKEND_SHELL_TOOL_DESCRIPTION),
      parameters: backendShellSchema,
      factory: createBackendShellFactory({
        ...config?.backendShell,
        shellPolicy: config?.backendShell?.shellPolicy ?? DEFAULT_SHELL_POLICY,
      }),
    });
  }

  // shell_output
  if (!isDisabled("shell_output", options)) {
    tools.push({
      id: "shell_output",
      type: "STATEFUL",
      description: renderToolDescription(SHELL_OUTPUT_TOOL_DESCRIPTION),
      parameters: shellOutputSchema,
      factory: createShellOutputFactory(),
    });
  }

  // shell_kill
  if (!isDisabled("shell_kill", options)) {
    tools.push({
      id: "shell_kill",
      type: "STATEFUL",
      description: renderToolDescription(SHELL_KILL_TOOL_DESCRIPTION),
      parameters: shellKillSchema,
      factory: createShellKillFactory(),
    });
  }

  // apply_patch
  if (!isDisabled("apply_patch", options)) {
    tools.push({
      id: "apply_patch",
      type: "STATELESS",
      description: renderToolDescription(APPLY_PATCH_TOOL_DESCRIPTION),
      parameters: applyPatchSchema,
      execute: createApplyPatchHandler(config?.readFile ?? { workspaceDir: process.cwd() }),
    });
  }

  // apply_diff
  if (!isDisabled("apply_diff", options)) {
    tools.push({
      id: "apply_diff",
      type: "STATELESS",
      description: renderToolDescription(APPLY_DIFF_TOOL_DESCRIPTION),
      parameters: applyDiffSchema,
      execute: createApplyDiffHandler(config?.readFile ?? { workspaceDir: process.cwd() }),
    });
  }

  // edit
  if (!isDisabled("edit", options)) {
    tools.push({
      id: "edit",
      type: "STATELESS",
      description: renderToolDescription(EDIT_TOOL_DESCRIPTION),
      parameters: editSchema,
      execute: createEditHandler(config?.readFile ?? { workspaceDir: process.cwd() }),
    });
  }

  // list_files
  if (!isDisabled("list_files", options)) {
    tools.push({
      id: "list_files",
      type: "STATELESS",
      description: renderToolDescription(LIST_FILES_TOOL_DESCRIPTION),
      parameters: listFilesSchema,
      execute: createListFilesHandler(config?.readFile ?? { workspaceDir: process.cwd() }),
    });
  }

  // grep
  if (!isDisabled("grep", options)) {
    tools.push({
      id: "grep",
      type: "STATELESS",
      description: renderToolDescription(GREP_TOOL_DESCRIPTION),
      parameters: grepSchema,
      execute: createGrepHandler(config?.readFile ?? { workspaceDir: process.cwd() }),
    });
  }

  // skill
  if (!isDisabled("skill", options)) {
    tools.push({
      id: "skill",
      type: "STATELESS",
      description: renderToolDescription(SKILL_TOOL_DESCRIPTION),
      parameters: skillSchema,
      execute: createSkillHandler(config?.skill),
    });
  }

  // update_todo_list
  if (!isDisabled("update_todo_list", options)) {
    tools.push({
      id: "update_todo_list",
      type: "STATELESS",
      description: renderToolDescription(UPDATE_TODO_LIST_TOOL_DESCRIPTION),
      parameters: updateTodoListSchema,
      execute: createUpdateTodoListHandler(),
    });
  }

  // use_mcp
  if (!isDisabled("use_mcp", options)) {
    tools.push({
      id: "use_mcp",
      type: "STATELESS",
      description: renderToolDescription(USE_MCP_TOOL_DESCRIPTION),
      parameters: useMcpSchema,
      execute: createLazyUseMcpHandler(() => getMcpManager()),
    });
  }

  return tools;
}

/**
 * Create a list of all predefined tools including builtin tools
 * This returns Tool[] which supports all tool types including BUILTIN
 */
export function createAllPredefinedTools(options?: PredefinedToolsOptions): Tool[] {
  const tools: Tool[] = createPredefinedTools(options) as Tool[];

  // Add builtin tools
  const builtinTools = createBuiltinTools(options?.builtin);
  tools.push(...builtinTools);

  return tools;
}
