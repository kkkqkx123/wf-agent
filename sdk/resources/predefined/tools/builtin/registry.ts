/**
 * Builtin Tool Registry
 *
 * Creates builtin tool definitions for the tool service.
 */

import type { Tool } from "@wf-agent/types";
import { renderToolDescription } from "@wf-agent/prompt-templates";
import type { BuiltinToolsOptions } from "./types.js";

// Import workflow tools
import {
  executeWorkflowSchema,
  EXECUTE_WORKFLOW_TOOL_DESCRIPTION,
  createExecuteWorkflowHandler,
} from "./workflow/index.js";
import {
  queryWorkflowStatusSchema,
  QUERY_WORKFLOW_STATUS_TOOL_DESCRIPTION,
  createQueryWorkflowStatusHandler,
} from "./workflow/index.js";
import {
  cancelWorkflowSchema,
  CANCEL_WORKFLOW_TOOL_DESCRIPTION,
  createCancelWorkflowHandler,
} from "./workflow/index.js";

/**
 * Check if the tool is disabled
 */
function isDisabled(toolId: string, options?: BuiltinToolsOptions): boolean {
  if (!options) return false;

  // If allowlist is set, only tools in the allowlist are enabled
  if (options.allowList && options.allowList.length > 0) {
    return !options.allowList.includes(toolId);
  }

  // If blocklist is set, tools in the blocklist are disabled
  if (options.blockList && options.blockList.length > 0) {
    return options.blockList.includes(toolId);
  }

  return false;
}

/**
 * Create a list of builtin tool definitions
 */
export function createBuiltinTools(options?: BuiltinToolsOptions): Tool[] {
  const tools: Tool[] = [];

  // execute_workflow
  if (!isDisabled("execute_workflow", options)) {
    tools.push({
      id: "builtin_execute_workflow",
      name: "execute_workflow",
      type: "BUILTIN",
      description: renderToolDescription(EXECUTE_WORKFLOW_TOOL_DESCRIPTION),
      parameters: executeWorkflowSchema,
      config: {
        name: "execute_workflow",
        execute: createExecuteWorkflowHandler(),
      },
    });
  }

  // query_workflow_status
  if (!isDisabled("query_workflow_status", options)) {
    tools.push({
      id: "builtin_query_workflow_status",
      name: "query_workflow_status",
      type: "BUILTIN",
      description: renderToolDescription(QUERY_WORKFLOW_STATUS_TOOL_DESCRIPTION),
      parameters: queryWorkflowStatusSchema,
      config: {
        name: "query_workflow_status",
        execute: createQueryWorkflowStatusHandler(),
      },
    });
  }

  // cancel_workflow
  if (!isDisabled("cancel_workflow", options)) {
    tools.push({
      id: "builtin_cancel_workflow",
      name: "cancel_workflow",
      type: "BUILTIN",
      description: renderToolDescription(CANCEL_WORKFLOW_TOOL_DESCRIPTION),
      parameters: cancelWorkflowSchema,
      config: {
        name: "cancel_workflow",
        execute: createCancelWorkflowHandler(),
      },
    });
  }

  return tools;
}
