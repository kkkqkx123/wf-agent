/**
 * Tool Description Registration Module
 *
 * Centralizes tool description registration.
 * This module should be used by the unified registration orchestrator.
 * Direct registration through tools/tool-descriptions.ts is deprecated.
 */

import type { ToolDescriptionData } from "@wf-agent/types";
import type { ToolDescriptionRegistry as ToolDescriptionRegistryType } from "../../shared/utils/tools/tool-description-registry.js";
import { createContextualLogger } from "../../utils/contextual-logger.js";
import type { ResourceRegistrationResult } from "./types.js";

const logger = createContextualLogger({ component: "ToolDescriptionRegistration" });

import { READ_FILE_TOOL_DESCRIPTION } from "../predefined/tools/stateless/filesystem/read-file/index.js";
import { WRITE_FILE_TOOL_DESCRIPTION } from "../predefined/tools/stateless/filesystem/write-file/index.js";
import { APPLY_PATCH_TOOL_DESCRIPTION } from "../predefined/tools/stateless/filesystem/apply-patch/index.js";
import { APPLY_DIFF_TOOL_DESCRIPTION } from "../predefined/tools/stateless/filesystem/apply-diff/index.js";
import { EDIT_TOOL_DESCRIPTION } from "../predefined/tools/stateless/filesystem/edit/index.js";
import { LIST_FILES_TOOL_DESCRIPTION } from "../predefined/tools/stateless/filesystem/list-files/index.js";
import { GREP_TOOL_DESCRIPTION } from "../predefined/tools/stateless/filesystem/grep/index.js";
import { GLOB_TOOL_DESCRIPTION } from "../predefined/tools/stateless/filesystem/glob/index.js";

import { RUN_SHELL_TOOL_DESCRIPTION } from "../predefined/tools/stateless/shell/run-shell/index.js";

import { SKILL_TOOL_DESCRIPTION } from "../predefined/tools/builtin/skill/index.js";
import { UPDATE_TODO_LIST_TOOL_DESCRIPTION } from "../predefined/tools/stateless/utility/update-todo-list/index.js";
import { USE_MCP_TOOL_DESCRIPTION } from "../predefined/tools/builtin/use-mcp/index.js";

import {
  RECORD_NOTE_TOOL_DESCRIPTION,
  RECALL_NOTES_TOOL_DESCRIPTION,
} from "../predefined/tools/stateful/memory/session-note/index.js";

import {
  BACKEND_SHELL_TOOL_DESCRIPTION,
  SHELL_OUTPUT_TOOL_DESCRIPTION,
  SHELL_KILL_TOOL_DESCRIPTION,
} from "../predefined/tools/stateful/shell/backend-shell/index.js";

import { EXECUTE_WORKFLOW_TOOL_DESCRIPTION } from "../predefined/tools/builtin/workflow/execute-workflow/index.js";
import { QUERY_WORKFLOW_STATUS_TOOL_DESCRIPTION } from "../predefined/tools/builtin/workflow/query-workflow-status/index.js";
import { CANCEL_WORKFLOW_TOOL_DESCRIPTION } from "../predefined/tools/builtin/workflow/cancel-workflow/index.js";

const ALL_PREDEFINED_TOOL_DESCRIPTIONS: ToolDescriptionData[] = [
  READ_FILE_TOOL_DESCRIPTION,
  WRITE_FILE_TOOL_DESCRIPTION,
  APPLY_PATCH_TOOL_DESCRIPTION,
  APPLY_DIFF_TOOL_DESCRIPTION,
  EDIT_TOOL_DESCRIPTION,
  LIST_FILES_TOOL_DESCRIPTION,
  GREP_TOOL_DESCRIPTION,
  GLOB_TOOL_DESCRIPTION,
  RUN_SHELL_TOOL_DESCRIPTION,
  SKILL_TOOL_DESCRIPTION,
  UPDATE_TODO_LIST_TOOL_DESCRIPTION,
  USE_MCP_TOOL_DESCRIPTION,
  RECORD_NOTE_TOOL_DESCRIPTION,
  RECALL_NOTES_TOOL_DESCRIPTION,
  BACKEND_SHELL_TOOL_DESCRIPTION,
  SHELL_OUTPUT_TOOL_DESCRIPTION,
  SHELL_KILL_TOOL_DESCRIPTION,
  EXECUTE_WORKFLOW_TOOL_DESCRIPTION,
  QUERY_WORKFLOW_STATUS_TOOL_DESCRIPTION,
  CANCEL_WORKFLOW_TOOL_DESCRIPTION,
];

export interface ToolDescriptionRegOptions {
  skipIfExists?: boolean;
}

/**
 * Register all predefined tool descriptions to the registry.
 */
export function registerAllPredefinedToolDescriptions(
  toolDescriptionRegistry: ToolDescriptionRegistryType,
  options: ToolDescriptionRegOptions = {},
): ResourceRegistrationResult {
  const skipIfExists = options.skipIfExists ?? true;
  const success: string[] = [];
  const failures: Array<{ id: string; error: string }> = [];

  for (const description of ALL_PREDEFINED_TOOL_DESCRIPTIONS) {
    try {
      if (skipIfExists && toolDescriptionRegistry.has(description.id)) {
        logger.debug(`Tool description already registered, skipping: ${description.id}`);
        continue;
      }
      toolDescriptionRegistry.register(description);
      success.push(description.id);
      logger.debug(`Registered tool description: ${description.id}`);
    } catch (error) {
      const errorMsg = error instanceof Error ? error.message : String(error);
      failures.push({ id: description.id, error: errorMsg });
      logger.error(`Failed to register tool description: ${description.id}`, { error: errorMsg });
    }
  }

  logger.info(`Tool descriptions registration completed: ${success.length} succeeded, ${failures.length} failed`);
  return { success, failures };
}

/**
 * Check if all predefined tool descriptions are registered.
 */
export function arePredefinedToolDescriptionsRegistered(
  toolDescriptionRegistry: ToolDescriptionRegistryType,
): boolean {
  return ALL_PREDEFINED_TOOL_DESCRIPTIONS.every(desc => toolDescriptionRegistry.has(desc.id));
}
