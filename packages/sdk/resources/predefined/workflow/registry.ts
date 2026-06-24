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
import { isResourceDisabled } from "../utils.js";

/**
 * Create a list of predefined workflow templates
 */
export function createPredefinedWorkflows(
  options?: PredefinedWorkflowsOptions,
): WorkflowTemplate[] {
  const workflows: WorkflowTemplate[] = [];
  const config = options?.config;

  // llm_summary_workflow
  if (!isResourceDisabled(LLM_SUMMARY_WORKFLOW_ID, options)) {
    const workflow = config?.llmSummary
      ? createCustomLlmSummaryWorkflow(config.llmSummary)
      : createLlmSummaryWorkflow();
    workflows.push(workflow);
  }

  return workflows;
}
