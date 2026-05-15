/**
 * Unified Registration Entry for Predefined Content
 *
 * This module provides a unified registration interface for predefined content,
 * responsible for coordinating registration operations across various modules.
 * Coordinator Layer Responsibilities:
 * - Provide unified registration entry API
 * - Coordinate registration sequence of submodules
 * - Aggregate results
 * - Contain no specific business logic
 *
 * Specific registration logic resides in respective module directories:
 * - trigger/: Trigger template registration
 * - workflow/: Workflow registration
 * - tools/: Tool registration
 */

import type { TriggerTemplateRegistry } from "../../core/registry/trigger-template-registry.js";
import type { WorkflowRegistry } from "../../workflow/stores/workflow-registry.js";
import type { ToolRegistry } from "../../core/registry/tool-registry.js";
import { createContextualLogger } from "../../utils/contextual-logger.js";

// Import from submodules
import {
  registerContextCompressionTrigger,
  unregisterContextCompressionTrigger,
  isContextCompressionTriggerRegistered,
  type ContextCompressionConfig as TriggerContextCompressionConfig,
} from "./trigger/index.js";

import {
  registerContextCompressionWorkflow,
  unregisterContextCompressionWorkflow,
  isContextCompressionWorkflowRegistered,
  type ContextCompressionConfig as WorkflowContextCompressionConfig,
} from "./workflow/index.js";

import { registerPredefinedTools, unregisterPredefinedTools } from "./tools/registration.js";

const logger = createContextualLogger({ component: "PredefinedRegistration" });

/**
 * Context Compression Configuration (unified type)
 */
export type ContextCompressionConfig = TriggerContextCompressionConfig &
  WorkflowContextCompressionConfig;

/**
 * Register context compression (both trigger and workflow).
 *
 * Note: The workflow must be registered first, as the trigger references the workflow ID.
 */
function registerContextCompression(
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
 * Unregister context compression (both trigger and workflow).
 */
function unregisterContextCompression(
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
function isContextCompressionRegistered(
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

/**
 * Register all predefined content
 *
 * @param triggerRegistry Trigger template registry
 * @param workflowRegistry Workflow registry
 * @param toolService Tool service
 * @param options Registration options
 * @returns Registration results
 */
export function registerAllPredefinedContent(
  triggerRegistry: TriggerTemplateRegistry,
  workflowRegistry: WorkflowRegistry,
  toolService: ToolRegistry,
  options?: {
    contextCompression?: {
      enabled?: boolean;
      config?: ContextCompressionConfig;
    };
    tools?: {
      enabled?: boolean;
      config?: Parameters<typeof registerPredefinedTools>[1];
    };
    skipIfExists?: boolean;
  },
): {
  contextCompression: {
    triggerRegistered: boolean;
    workflowRegistered: boolean;
  };
  tools: {
    success: string[];
    failures: Array<{ toolId: string; error: string }>;
  };
} {
  const skipIfExists = options?.skipIfExists ?? true;
  const results = {
    contextCompression: {
      triggerRegistered: false,
      workflowRegistered: false,
    },
    tools: {
      success: [] as string[],
      failures: [] as Array<{ toolId: string; error: string }>,
    },
  };

  // Register context compression (delegated to the context-compression module)
  if (options?.contextCompression?.enabled !== false) {
    try {
      results.contextCompression = registerContextCompression(
        triggerRegistry,
        workflowRegistry,
        options?.contextCompression?.config,
        skipIfExists,
      );
      logger.info("Context compression registered");
    } catch (error) {
      logger.error("Failed to register context compression", { error });
    }
  }

  // Register predefined tools (delegated to the tools module)
  if (options?.tools?.enabled !== false) {
    try {
      results.tools = registerPredefinedTools(toolService, options?.tools?.config, skipIfExists);
      logger.info("Predefined tools registered");
    } catch (error) {
      logger.error("Failed to register predefined tools", { error });
    }
  }

  return results;
}

/**
 * Cancel all predefined content.
 */
export function unregisterAllPredefinedContent(
  triggerRegistry: TriggerTemplateRegistry,
  workflowRegistry: WorkflowRegistry,
  toolService: ToolRegistry,
): {
  contextCompression: {
    triggerUnregistered: boolean;
    workflowUnregistered: boolean;
  };
  tools: {
    success: string[];
    failures: Array<{ toolId: string; error: string }>;
  };
} {
  const results = {
    contextCompression: {
      triggerUnregistered: false,
      workflowUnregistered: false,
    },
    tools: {
      success: [] as string[],
      failures: [] as Array<{ toolId: string; error: string }>,
    },
  };

  // Cancel context compression (delegated to the context-compression module)
  try {
    results.contextCompression = unregisterContextCompression(triggerRegistry, workflowRegistry);
    logger.info("Context compression unregistered");
  } catch (error) {
    logger.error("Failed to unregister context compression", { error });
  }

  // Cancel the predefined tool (delegated to the tools module).
  try {
    results.tools = unregisterPredefinedTools(toolService);
    logger.info("Predefined tools unregistered");
  } catch (error) {
    logger.error("Failed to unregister predefined tools", { error });
  }

  return results;
}

// Reexport the interfaces of the submodules to maintain API compatibility.
export {
  registerContextCompression,
  unregisterContextCompression,
  isContextCompressionRegistered,
};
