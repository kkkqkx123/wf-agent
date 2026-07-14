/**
 * Predefined Tool Registry
 *
 * Responsible for creating and registering predefined tools with the tool service.
 */

import type { ToolDefinitionLike } from "@sdk/services/tools/utils.js";
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
  createListFilesDescription,
} from "./stateless/filesystem/list-files/index.js";
import {
  grepSchema,
  createGrepHandler,
  GREP_TOOL_DESCRIPTION,
} from "./stateless/filesystem/grep/index.js";
import {
  globSchema,
  createGlobHandler,
  createGlobDescription,
} from "./stateless/filesystem/glob/index.js";

// Import knowledge tools (skill)
import { skillSchema, createSkillHandler, SKILL_TOOL_DESCRIPTION } from "./builtin/skill/index.js";
import {
  updateTodoListSchema,
  createUpdateTodoListHandler,
  UPDATE_TODO_LIST_TOOL_DESCRIPTION,
} from "./stateless/utility/update-todo-list/index.js";
// Import integration tools (use-mcp)
import {
  useMcpSchema,
  createLazyUseMcpHandler,
  USE_MCP_TOOL_DESCRIPTION,
} from "./builtin/use-mcp/index.js";
import { getMcpManager } from "@sdk/services/executors/mcp/index.js";

// Importing a stateful tool
import {
  recordNoteSchema,
  recallNotesSchema,
  listCategoriesSchema,
  createRecordNoteFactory,
  createRecallNotesFactory,
  createListCategoriesFactory,
  RECORD_NOTE_TOOL_DESCRIPTION,
  RECALL_NOTES_TOOL_DESCRIPTION,
  LIST_CATEGORIES_TOOL_DESCRIPTION,
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

import { isResourceDisabled } from "../utils.js";

/**
 * Create a list of predefined tool definitions
 */
export function createPredefinedTools(options?: PredefinedToolsOptions): ToolDefinitionLike[] {
  const tools: ToolDefinitionLike[] = [];
  const config = options?.config;

  // read_file
  if (!isResourceDisabled("read_file", options)) {
    tools.push({
      id: "read_file",
      type: "STATELESS",
      description: renderToolDescription(READ_FILE_TOOL_DESCRIPTION),
      parameters: readFileSchema,
      execute: createReadFileHandler(config?.readFile ?? { workspaceDir: process.cwd() }),
    });
  }

  // write_file
  if (!isResourceDisabled("write_file", options)) {
    tools.push({
      id: "write_file",
      type: "STATELESS",
      description: renderToolDescription(WRITE_FILE_TOOL_DESCRIPTION),
      parameters: writeFileSchema,
      execute: createWriteFileHandler(config?.writeFile ?? { workspaceDir: process.cwd() }),
    });
  }

  // run_shell
  if (!isResourceDisabled("run_shell", options)) {
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
  if (!isResourceDisabled("record_note", options)) {
    tools.push({
      id: "record_note",
      type: "STATEFUL",
      description: renderToolDescription(RECORD_NOTE_TOOL_DESCRIPTION),
      parameters: recordNoteSchema,
      factory: createRecordNoteFactory(
        config?.sessionNote ?? { dbPath: "data/session-notes.db", maxNotes: 1000 },
      ),
    });
  }

  // recall_notes
  if (!isResourceDisabled("recall_notes", options)) {
    tools.push({
      id: "recall_notes",
      type: "STATEFUL",
      description: renderToolDescription(RECALL_NOTES_TOOL_DESCRIPTION),
      parameters: recallNotesSchema,
      factory: createRecallNotesFactory(
        config?.sessionNote ?? { dbPath: "data/session-notes.db", maxNotes: 1000 },
      ),
    });
  }

  // list_categories
  if (!isResourceDisabled("list_categories", options)) {
    tools.push({
      id: "list_categories",
      type: "STATEFUL",
      description: renderToolDescription(LIST_CATEGORIES_TOOL_DESCRIPTION),
      parameters: listCategoriesSchema,
      factory: createListCategoriesFactory(
        config?.sessionNote ?? { dbPath: "data/session-notes.db", maxNotes: 1000 },
      ),
    });
  }

  // backend_shell
  if (!isResourceDisabled("backend_shell", options)) {
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
  if (!isResourceDisabled("shell_output", options)) {
    tools.push({
      id: "shell_output",
      type: "STATEFUL",
      description: renderToolDescription(SHELL_OUTPUT_TOOL_DESCRIPTION),
      parameters: shellOutputSchema,
      factory: createShellOutputFactory(),
    });
  }

  // shell_kill
  if (!isResourceDisabled("shell_kill", options)) {
    tools.push({
      id: "shell_kill",
      type: "STATEFUL",
      description: renderToolDescription(SHELL_KILL_TOOL_DESCRIPTION),
      parameters: shellKillSchema,
      factory: createShellKillFactory(),
    });
  }

  // apply_patch
  if (!isResourceDisabled("apply_patch", options)) {
    tools.push({
      id: "apply_patch",
      type: "STATELESS",
      description: renderToolDescription(APPLY_PATCH_TOOL_DESCRIPTION),
      parameters: applyPatchSchema,
      execute: createApplyPatchHandler(config?.readFile ?? { workspaceDir: process.cwd() }),
    });
  }

  // apply_diff
  if (!isResourceDisabled("apply_diff", options)) {
    tools.push({
      id: "apply_diff",
      type: "STATELESS",
      description: renderToolDescription(APPLY_DIFF_TOOL_DESCRIPTION),
      parameters: applyDiffSchema,
      execute: createApplyDiffHandler(config?.readFile ?? { workspaceDir: process.cwd() }),
    });
  }

  // edit
  if (!isResourceDisabled("edit", options)) {
    tools.push({
      id: "edit",
      type: "STATELESS",
      description: renderToolDescription(EDIT_TOOL_DESCRIPTION),
      parameters: editSchema,
      execute: createEditHandler(config?.readFile ?? { workspaceDir: process.cwd() }),
    });
  }

  // list_files
  if (!isResourceDisabled("list_files", options)) {
    tools.push({
      id: "list_files",
      type: "STATELESS",
      description: renderToolDescription(createListFilesDescription(config?.listFiles)),
      parameters: listFilesSchema,
      execute: createListFilesHandler(config?.listFiles ?? { workspaceDir: process.cwd() }),
    });
  }

  // grep
  if (!isResourceDisabled("grep", options)) {
    tools.push({
      id: "grep",
      type: "STATELESS",
      description: renderToolDescription(GREP_TOOL_DESCRIPTION),
      parameters: grepSchema,
      execute: createGrepHandler(config?.readFile ?? { workspaceDir: process.cwd() }),
    });
  }

  // glob
  if (!isResourceDisabled("glob", options)) {
    tools.push({
      id: "glob",
      type: "STATELESS",
      description: renderToolDescription(createGlobDescription(config?.glob)),
      parameters: globSchema,
      execute: createGlobHandler(config?.glob ?? { workspaceDir: process.cwd() }),
    });
  }

  // skill
  if (!isResourceDisabled("skill", options)) {
    tools.push({
      id: "skill",
      type: "STATELESS",
      description: renderToolDescription(SKILL_TOOL_DESCRIPTION),
      parameters: skillSchema,
      execute: createSkillHandler(config?.skill),
    });
  }

  // update_todo_list
  if (!isResourceDisabled("update_todo_list", options)) {
    tools.push({
      id: "update_todo_list",
      type: "STATELESS",
      description: renderToolDescription(UPDATE_TODO_LIST_TOOL_DESCRIPTION),
      parameters: updateTodoListSchema,
      execute: createUpdateTodoListHandler(),
    });
  }

  // use_mcp
  if (!isResourceDisabled("use_mcp", options)) {
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
