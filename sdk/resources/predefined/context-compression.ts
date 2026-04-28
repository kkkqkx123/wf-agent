/**
 * Context Compression Coordination Module
 *
 * Handles dependencies between triggers and workflows
 * The workflow must be registered first, as the trigger references the workflow ID.
 */

import type { TriggerTemplateRegistry } from "../../core/registry/trigger-template-registry.js";
import type { WorkflowRegistry } from "../../graph/stores/workflow-registry.js";
import {
  registerContextCompressionTrigger,
  unregisterContextCompressionTrigger,
  isContextCompressionTriggerRegistered,
  type ContextCompressionConfig,
} from "./trigger/index.js";
import {
  registerContextCompressionWorkflow,
  unregisterContextCompressionWorkflow,
  isContextCompressionWorkflowRegistered,
} from "./workflow/index.js";

/**
 * Register both the context compression trigger and the workflow simultaneously.
 *
 * Note: The workflow must be registered first, as the trigger references the workflow ID.
 */
export function registerContextCompression(
  triggerRegistry: TriggerTemplateRegistry,
  workflowRegistry: WorkflowRegistry,
  config?: ContextCompressionConfig,
  skipIfExists: boolean = true,
): {
  triggerRegistered: boolean;
  workflowRegistered: boolean;
} {
  // The workflow must be registered first, as the trigger references the workflow ID.
  const workflowRegistered = registerContextCompressionWorkflow(
    workflowRegistry,
    config,
    skipIfExists,
  );
  const triggerRegistered = registerContextCompressionTrigger(
    triggerRegistry,
    config,
    skipIfExists,
  );

  return {
    triggerRegistered,
    workflowRegistered,
  };
}

/**
 * Also cancel the context compression trigger and the workflow.
 */
export function unregisterContextCompression(
  triggerRegistry: TriggerTemplateRegistry,
  workflowRegistry: WorkflowRegistry,
): {
  triggerUnregistered: boolean;
  workflowUnregistered: boolean;
} {
  const triggerUnregistered = unregisterContextCompressionTrigger(triggerRegistry);
  const workflowUnregistered = unregisterContextCompressionWorkflow(workflowRegistry);

  return {
    triggerUnregistered,
    workflowUnregistered,
  };
}

/**
 * Check whether context compression is registered.
 */
export function isContextCompressionRegistered(
  triggerRegistry: TriggerTemplateRegistry,
  workflowRegistry: WorkflowRegistry,
): {
  triggerRegistered: boolean;
  workflowRegistered: boolean;
} {
  return {
    triggerRegistered: isContextCompressionTriggerRegistered(triggerRegistry),
    workflowRegistered: isContextCompressionWorkflowRegistered(workflowRegistry),
  };
}

// Reexport the configuration type.
export { type ContextCompressionConfig };
