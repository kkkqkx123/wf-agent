/**
 * Predefined Workflow Registry
 *
 * Responsible for creating predefined workflow templates.
 */

import type { WorkflowTemplate } from "@wf-agent/types";
import type { PredefinedWorkflowsOptions } from "./types.js";
import {
  createLlmSummaryWorkflow,
  createCustomLlmSummaryWorkflow,
  LLM_SUMMARY_WORKFLOW_ID,
} from "./llm-summary.js";

/**
 * Check if the workflow is disabled.
 */
function isDisabled(workflowId: string, options?: PredefinedWorkflowsOptions): boolean {
  if (!options) return false;

  // If a whitelist is set, only the workflows listed in the whitelist will be enabled.
  if (options.allowList && options.allowList.length > 0) {
    return !options.allowList.includes(workflowId);
  }

  // If a blacklist is set, the workflows listed in the blacklist will be disabled.
  if (options.blockList && options.blockList.length > 0) {
    return options.blockList.includes(workflowId);
  }

  return false;
}

/**
 * Create a list of predefined workflow templates
 */
export function createPredefinedWorkflows(
  options?: PredefinedWorkflowsOptions,
): WorkflowTemplate[] {
  const workflows: WorkflowTemplate[] = [];
  const config = options?.config;

  // llm_summary_workflow
  if (!isDisabled(LLM_SUMMARY_WORKFLOW_ID, options)) {
    const workflow = config?.llmSummary
      ? createCustomLlmSummaryWorkflow(config.llmSummary)
      : createLlmSummaryWorkflow();
    workflows.push(workflow);
  }

  return workflows;
}
