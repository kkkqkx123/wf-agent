/**
 * Context Compression Workflow Registration
 *
 * Registration layer: Handles workflow registration logic (with side effects)
 */

import type { WorkflowRegistry } from "@sdk/workflow/stores/workflow-registry.js";
import { createContextualLogger } from "@sdk/utils/contextual-logger.js";
import {
  CONTEXT_COMPRESSION_WORKFLOW_ID,
  createContextCompressionWorkflow,
  createCustomContextCompressionWorkflow,
  type ContextCompressionConfig,
} from "./registry.js";

const logger = createContextualLogger({ component: "ContextCompressionWorkflow" });

/**
 * Register the context compression workflow
 */
export function registerContextCompressionWorkflow(
  registry: WorkflowRegistry,
  config?: ContextCompressionConfig,
  skipIfExists: boolean = true,
): boolean {
  try {
    const workflow = config?.compressionPrompt
      ? createCustomContextCompressionWorkflow(config)
      : createContextCompressionWorkflow();

    // Check if the workflow already exists
    if (registry.has(workflow.id)) {
      if (skipIfExists) {
        logger.info("Context compression workflow already exists, skipping registration");
        return false;
      }
    }

    registry.register(workflow, { skipIfExists });
    logger.info("Registered context compression workflow");
    return true;
  } catch (error) {
    logger.error("Failed to register context compression workflow", { error });
    return false;
  }
}

/**
 * Unregister the context compression workflow.
 */
export function unregisterContextCompressionWorkflow(registry: WorkflowRegistry): boolean {
  try {
    if (registry.has(CONTEXT_COMPRESSION_WORKFLOW_ID)) {
      registry.unregister(CONTEXT_COMPRESSION_WORKFLOW_ID, { force: true });
      logger.info("Unregistered context compression workflow");
      return true;
    }
    return false;
  } catch (error) {
    logger.error("Failed to unregister context compression workflow", { error });
    return false;
  }
}

/**
 * Check whether the context compression workflow has been registered.
 */
export function isContextCompressionWorkflowRegistered(registry: WorkflowRegistry): boolean {
  return registry.has(CONTEXT_COMPRESSION_WORKFLOW_ID);
}
