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

import type { TriggerTemplateRegistry } from "@sdk/shared/registry/trigger-template-registry.js";
import type { WorkflowRegistry } from "@sdk/workflow/stores/workflow-registry.js";
import type { ToolRegistry } from "@sdk/shared/registry/tool-registry.js";
import type { PresetsConfig } from "@wf-agent/sdk/resources";
import { createContextualLogger } from "@sdk/utils/contextual-logger.js";

// Import from submodules
import { registerPredefinedTriggers, unregisterPredefinedTriggers } from "./trigger/index.js";

import { registerPredefinedWorkflows, unregisterPredefinedWorkflows } from "./workflow/index.js";

import { registerPredefinedTools, unregisterPredefinedTools } from "./tools/registration.js";

const logger = createContextualLogger({ component: "PredefinedRegistration" });

/**
 * Result type for predefined content registration
 */
export interface PredefinedRegistrationResult {
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
}

/**
 * Map context compression preset config to internal llmSummary config.
 */
function mapLlmSummaryConfig(cc?: {
  prompt?: string;
  timeout?: number;
  maxTriggers?: number;
}): { compressionPrompt?: string; timeout?: number; maxTriggers?: number } | undefined {
  if (!cc) return undefined;
  return {
    compressionPrompt: cc.prompt,
    timeout: cc.timeout,
    maxTriggers: cc.maxTriggers,
  };
}

/**
 * Register predefined content
 *
 * Accepts raw PresetsConfig and internally maps to sub-module configurations.
 * sdk-instance should not need to know internal key names.
 *
 * @param triggerRegistry Trigger template registry
 * @param workflowRegistry Workflow registry
 * @param toolService Tool service
 * @param presets Raw presets configuration (optional)
 * @param skipIfExists Whether to skip registration if already exists
 * @returns Registration results
 */
export async function registerPredefinedContent(
  triggerRegistry: TriggerTemplateRegistry,
  workflowRegistry: WorkflowRegistry,
  toolService: ToolRegistry,
  presets?: PresetsConfig,
  skipIfExists: boolean = true,
): Promise<PredefinedRegistrationResult> {
  const results: PredefinedRegistrationResult = {
    triggers: { success: [], failures: [] },
    workflows: { success: [], failures: [] },
    tools: { success: [], failures: [] },
  };

  // Track successfully registered items for rollback on failure.
  const rollbackState: { workflows: string[]; triggers: string[] } = {
    workflows: [],
    triggers: [],
  };

  // ====================================================================
  // Registration order: workflows → triggers → tools
  // Workflows must be registered first because triggers (e.g.
  // execute_triggered_subworkflow) reference workflow IDs.
  // ====================================================================

  // 1. Register predefined workflows (context compression)
  if (presets?.contextCompression?.enabled !== false) {
    try {
      const llmConfig = mapLlmSummaryConfig(presets?.contextCompression);
      results.workflows = registerPredefinedWorkflows(
        workflowRegistry,
        llmConfig ? { config: { llmSummary: llmConfig } } : undefined,
        skipIfExists,
      );
      rollbackState.workflows = results.workflows.success;
      logger.info("Predefined workflows registered");
    } catch (error) {
      logger.error("Failed to register predefined workflows", { error });
    }
  }

  // 2. Register predefined triggers (context compression)
  if (presets?.contextCompression?.enabled !== false) {
    try {
      const llmConfig = mapLlmSummaryConfig(presets?.contextCompression);
      results.triggers = registerPredefinedTriggers(
        triggerRegistry,
        llmConfig ? { config: { llmSummary: llmConfig } } : undefined,
        skipIfExists,
      );
      rollbackState.triggers = results.triggers.success;
      logger.info("Predefined triggers registered");
    } catch (error) {
      logger.error("Failed to register predefined triggers", { error });
    }
  }

  // 3. Register predefined tools
  if (presets?.predefinedTools?.enabled !== false) {
    try {
      results.tools = registerPredefinedTools(
        toolService,
        {
          allowList: presets?.predefinedTools?.allowList,
          blockList: presets?.predefinedTools?.blockList,
          config: presets?.predefinedTools?.config,
        },
        skipIfExists,
      );
      logger.info("Predefined tools registered");
    } catch (error) {
      logger.error("Failed to register predefined tools", { error });
      // Rollback: undo previously registered workflows and triggers
      // to avoid leaving the system in a partially-registered state.
      for (const id of rollbackState.workflows) {
        try {
          await workflowRegistry.unregister(id, { force: true });
          logger.warn(`Rolled back workflow: ${id}`);
        } catch (rollbackError) {
          logger.error(`Failed to rollback workflow: ${id}`, { error: rollbackError });
        }
      }
      for (const name of rollbackState.triggers) {
        try {
          await triggerRegistry.unregister(name);
          logger.warn(`Rolled back trigger: ${name}`);
        } catch (rollbackError) {
          logger.error(`Failed to rollback trigger: ${name}`, { error: rollbackError });
        }
      }
    }
  }

  return results;
}

/**
 * Unregister predefined content.
 */
export async function unregisterPredefinedContent(
  triggerRegistry: TriggerTemplateRegistry,
  workflowRegistry: WorkflowRegistry,
  toolService: ToolRegistry,
  options?: {
    triggerNames?: string[];
    workflowIds?: string[];
    toolIds?: string[];
  },
): Promise<PredefinedRegistrationResult> {
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
    results.triggers = await unregisterPredefinedTriggers(triggerRegistry, options?.triggerNames);
    logger.info("Predefined triggers unregistered");
  } catch (error) {
    logger.error("Failed to unregister predefined triggers", { error });
  }

  // Unregister predefined workflows
  try {
    results.workflows = await unregisterPredefinedWorkflows(workflowRegistry, options?.workflowIds);
    logger.info("Predefined workflows unregistered");
  } catch (error) {
    logger.error("Failed to unregister predefined workflows", { error });
  }

  // Unregister predefined tools (delegated to the tools module).
  try {
    results.tools = await unregisterPredefinedTools(toolService, options?.toolIds);
    logger.info("Predefined tools unregistered");
  } catch (error) {
    logger.error("Failed to unregister predefined tools", { error });
  }

  return results;
}
