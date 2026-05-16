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

import type { TriggerTemplateRegistry } from "@sdk/core/registry/trigger-template-registry.js";
import type { WorkflowRegistry } from "@sdk/workflow/stores/workflow-registry.js";
import type { ToolRegistry } from "@sdk/core/registry/tool-registry.js";
import { createContextualLogger } from "@sdk/utils/contextual-logger.js";

// Import from submodules
import {
  registerPredefinedTriggers,
  unregisterPredefinedTriggers,
  type PredefinedTriggersOptions,
} from "./trigger/index.js";

import {
  registerPredefinedWorkflows,
  unregisterPredefinedWorkflows,
  type PredefinedWorkflowsOptions,
} from "./workflow/index.js";

import { registerPredefinedTools, unregisterPredefinedTools } from "./tools/registration.js";

const logger = createContextualLogger({ component: "PredefinedRegistration" });

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
    triggers?: PredefinedTriggersOptions & { enabled?: boolean };
    workflows?: PredefinedWorkflowsOptions & { enabled?: boolean };
    tools?: {
      enabled?: boolean;
      config?: Parameters<typeof registerPredefinedTools>[1];
    };
    skipIfExists?: boolean;
  },
): {
  triggers: {
    success: string[];
    failures: Array<{ triggerName: string; error: string }>;
  };
  workflows: {
    success: string[];
    failures: Array<{ workflowId: string; error: string }>;
  };
  tools: {
    success: string[];
    failures: Array<{ toolId: string; error: string }>;
  };
} {
  const skipIfExists = options?.skipIfExists ?? true;
  const results = {
    triggers: {
      success: [] as string[],
      failures: [] as Array<{ triggerName: string; error: string }>,
    },
    workflows: {
      success: [] as string[],
      failures: [] as Array<{ workflowId: string; error: string }>,
    },
    tools: {
      success: [] as string[],
      failures: [] as Array<{ toolId: string; error: string }>,
    },
  };

  // Register predefined triggers
  if (options?.triggers?.enabled !== false) {
    try {
      results.triggers = registerPredefinedTriggers(
        triggerRegistry,
        options?.triggers,
        skipIfExists,
      );
      logger.info("Predefined triggers registered");
    } catch (error) {
      logger.error("Failed to register predefined triggers", { error });
    }
  }

  // Register predefined workflows
  if (options?.workflows?.enabled !== false) {
    try {
      results.workflows = registerPredefinedWorkflows(
        workflowRegistry,
        options?.workflows,
        skipIfExists,
      );
      logger.info("Predefined workflows registered");
    } catch (error) {
      logger.error("Failed to register predefined workflows", { error });
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
 * Unregister all predefined content.
 */
export function unregisterAllPredefinedContent(
  triggerRegistry: TriggerTemplateRegistry,
  workflowRegistry: WorkflowRegistry,
  toolService: ToolRegistry,
  options?: {
    triggerNames?: string[];
    workflowIds?: string[];
    toolIds?: string[];
  },
): {
  triggers: {
    success: string[];
    failures: Array<{ triggerName: string; error: string }>;
  };
  workflows: {
    success: string[];
    failures: Array<{ workflowId: string; error: string }>;
  };
  tools: {
    success: string[];
    failures: Array<{ toolId: string; error: string }>;
  };
} {
  const results = {
    triggers: {
      success: [] as string[],
      failures: [] as Array<{ triggerName: string; error: string }>,
    },
    workflows: {
      success: [] as string[],
      failures: [] as Array<{ workflowId: string; error: string }>,
    },
    tools: {
      success: [] as string[],
      failures: [] as Array<{ toolId: string; error: string }>,
    },
  };

  // Unregister predefined triggers
  try {
    results.triggers = unregisterPredefinedTriggers(triggerRegistry, options?.triggerNames);
    logger.info("Predefined triggers unregistered");
  } catch (error) {
    logger.error("Failed to unregister predefined triggers", { error });
  }

  // Unregister predefined workflows
  try {
    results.workflows = unregisterPredefinedWorkflows(workflowRegistry, options?.workflowIds);
    logger.info("Predefined workflows unregistered");
  } catch (error) {
    logger.error("Failed to unregister predefined workflows", { error });
  }

  // Unregister predefined tools (delegated to the tools module).
  try {
    results.tools = unregisterPredefinedTools(toolService, options?.toolIds);
    logger.info("Predefined tools unregistered");
  } catch (error) {
    logger.error("Failed to unregister predefined tools", { error });
  }

  return results;
}
