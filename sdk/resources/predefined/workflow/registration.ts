/**
 * Predefined Workflow Registration Entrance
 *
 * Responsible for the registration and deregistration of predefined workflows.
 */

import type { WorkflowRegistry } from "@sdk/workflow/stores/workflow-registry.js";
import { createContextualLogger } from "@sdk/utils/contextual-logger.js";
import { createPredefinedWorkflows } from "./registry.js";
import { LLM_SUMMARY_WORKFLOW_ID } from "./llm-summary.js";
import type { PredefinedWorkflowsOptions } from "./types.js";

const logger = createContextualLogger({ component: "PredefinedWorkflows" });

/**
 * Register predefined workflows
 *
 * @param registry: The workflow registry
 * @param options: Configuration options
 * @param skipIfExists: Whether to skip the registration if the workflow already exists (instead of reporting an error)
 * @returns: The registration result
 */
export function registerPredefinedWorkflows(
  registry: WorkflowRegistry,
  options?: PredefinedWorkflowsOptions,
  skipIfExists: boolean = true,
): {
  success: string[];
  failures: Array<{ workflowId: string; error: string }>;
} {
  const success: string[] = [];
  const failures: Array<{ workflowId: string; error: string }> = [];

  try {
    // Create predefined workflows
    const workflows = createPredefinedWorkflows(options);

    // Register with the workflow registry.
    for (const workflow of workflows) {
      try {
        // Check if it already exists.
        if (skipIfExists && registry.has(workflow.id)) {
          logger.info(`Workflow already registered, skipping: ${workflow.id}`);
          continue;
        }

        // Register the workflow.
        registry.register(workflow, { skipIfExists });
        success.push(workflow.id);
        logger.info(`Registered predefined workflow: ${workflow.id}`);
      } catch (error) {
        const errorMsg = error instanceof Error ? error.message : String(error);
        failures.push({ workflowId: workflow.id, error: errorMsg });
        logger.error(`Failed to register predefined workflow: ${workflow.id}`, { error: errorMsg });
      }
    }

    logger.info(
      `Predefined workflows registration completed: ${success.length} succeeded, ${failures.length} failed`,
    );
  } catch (error) {
    logger.error(`Failed to create predefined workflows`, { error });
  }

  return { success, failures };
}

/**
 * Unregister predefined workflows
 *
 * @param registry: Workflow registry
 * @param workflowIds: List of workflow IDs to be unregistered; if empty, all predefined workflows will be unregistered
 * @returns: Unregistration result
 */
export function unregisterPredefinedWorkflows(
  registry: WorkflowRegistry,
  workflowIds?: string[],
): {
  success: string[];
  failures: Array<{ workflowId: string; error: string }>;
} {
  const success: string[] = [];
  const failures: Array<{ workflowId: string; error: string }> = [];

  // If no workflow ID is specified, retrieve all predefined workflow IDs.
  const predefinedWorkflowIds = workflowIds || [LLM_SUMMARY_WORKFLOW_ID];

  for (const workflowId of predefinedWorkflowIds) {
    try {
      if (registry.has(workflowId)) {
        registry.unregister(workflowId, { force: true });
        success.push(workflowId);
        logger.info(`Unregistered predefined workflow: ${workflowId}`);
      }
    } catch (error) {
      const errorMsg = error instanceof Error ? error.message : String(error);
      failures.push({ workflowId, error: errorMsg });
      logger.error(`Failed to unregister predefined workflow: ${workflowId}`, { error: errorMsg });
    }
  }

  logger.info(
    `Predefined workflows unregistration completed: ${success.length} succeeded, ${failures.length} failed`,
  );
  return { success, failures };
}

/**
 * Check if a predefined workflow has been registered.
 */
export function isPredefinedWorkflowRegistered(
  registry: WorkflowRegistry,
  workflowId: string,
): boolean {
  return registry.has(workflowId);
}
