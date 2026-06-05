/**
 * Builtin Tool Registry
 *
 * Creates builtin tool definitions for the tool service.
 */

import type { Tool } from "@wf-agent/types";
import { renderToolDescription } from "../../prompt-templates/tool-description-renderer.js";
import type { BuiltinToolsOptions } from "./types.js";

// Import workflow tools
import {
  executeWorkflowSchema,
  EXECUTE_WORKFLOW_TOOL_DESCRIPTION,
  generateExecuteWorkflowDescription,
  createExecuteWorkflowHandler,
} from "./workflow/index.js";
import {
  queryWorkflowStatusSchema,
  QUERY_WORKFLOW_STATUS_TOOL_DESCRIPTION,
  generateQueryWorkflowStatusDescription,
  createQueryWorkflowStatusHandler,
} from "./workflow/index.js";
import {
  cancelWorkflowSchema,
  CANCEL_WORKFLOW_TOOL_DESCRIPTION,
  generateCancelWorkflowDescription,
  createCancelWorkflowHandler,
} from "./workflow/index.js";

// Import agent tools
import {
  callAgentSchema,
  CALL_AGENT_TOOL_DESCRIPTION,
  generateCallAgentDescription,
  createCallAgentHandler,
} from "./agent/index.js";

// Import ask-followup-question tool
import {
  askFollowupQuestionSchema,
  ASK_FOLLOWUP_QUESTION_TOOL_DESCRIPTION,
  createAskFollowupQuestionHandler,
} from "./ask-followup-question/index.js";

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
    // Generate dynamic description when workflow config is available
    const workflowDesc = options?.workflow
      ? generateExecuteWorkflowDescription(options.workflow.loader.getAvailableWorkflows())
      : EXECUTE_WORKFLOW_TOOL_DESCRIPTION;

    tools.push({
      id: "builtin_execute_workflow",
      type: "BUILTIN",
      description: renderToolDescription(workflowDesc),
      parameters: executeWorkflowSchema,
      config: {
        execute: createExecuteWorkflowHandler(options?.workflow),
      },
    });
  }

  // query_workflow_status
  if (!isDisabled("query_workflow_status", options)) {
    const queryDesc = options?.workflow
      ? generateQueryWorkflowStatusDescription(options.workflow.loader.getAvailableWorkflows())
      : QUERY_WORKFLOW_STATUS_TOOL_DESCRIPTION;

    tools.push({
      id: "builtin_query_workflow_status",
      type: "BUILTIN",
      description: renderToolDescription(queryDesc),
      parameters: queryWorkflowStatusSchema,
      config: {
        execute: createQueryWorkflowStatusHandler(),
      },
    });
  }

  // cancel_workflow
  if (!isDisabled("cancel_workflow", options)) {
    const cancelDesc = options?.workflow
      ? generateCancelWorkflowDescription(options.workflow.loader.getAvailableWorkflows())
      : CANCEL_WORKFLOW_TOOL_DESCRIPTION;

    tools.push({
      id: "builtin_cancel_workflow",
      type: "BUILTIN",
      description: renderToolDescription(cancelDesc),
      parameters: cancelWorkflowSchema,
      config: {
        execute: createCancelWorkflowHandler(),
      },
    });
  }

  // call_agent
  if (!isDisabled("call_agent", options)) {
    // Generate dynamic description when agent config is available
    const agentDesc = options?.agent
      ? generateCallAgentDescription(options.agent.loader.getAvailableAgentProfiles())
      : CALL_AGENT_TOOL_DESCRIPTION;

    tools.push({
      id: "builtin_call_agent",
      type: "BUILTIN",
      description: renderToolDescription(agentDesc),
      parameters: callAgentSchema,
      config: {
        execute: createCallAgentHandler(options?.agent),
      },
    });
  }

  // ask_followup_question
  if (!isDisabled("ask_followup_question", options)) {
    tools.push({
      id: "ask_followup_question",
      type: "BUILTIN",
      description: renderToolDescription(ASK_FOLLOWUP_QUESTION_TOOL_DESCRIPTION),
      parameters: askFollowupQuestionSchema,
      config: {
        execute: createAskFollowupQuestionHandler(),
      },
      metadata: {
        category: "interaction",
        requiresUserInteraction: true,
        interactionType: "ASK_FOLLOWUP_QUESTION",
      },
    });
  }

  return tools;
}
