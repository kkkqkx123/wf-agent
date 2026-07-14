/**
 * Unified Registration Entry for Predefined Content
 *
 * This module provides a unified registration interface for predefined content,
 * responsible for coordinating registration operations across various modules.
 *
 * Specific registration logic resides in respective module directories:
 * - trigger/: Trigger template registration
 * - workflow/: Workflow registration
 * - tools/: Tool registration
 *
 * Note: No rollback mechanism is needed because:
 * 1. Each sub-registration is independent (single resource granularity)
 * 2. skipIfExists=true ensures idempotency
 * 3. Partial success is an acceptable state (no strong consistency constraints)
 */

import type { TriggerTemplateRegistry } from "@sdk/shared/registry/trigger-template-registry.js";
import type { WorkflowRegistry } from "@sdk/workflow/registry/workflow-registry.js";
import type { ToolRegistry } from "@sdk/shared/registry/tool-registry.js";
import type { PresetsConfig } from "@wf-agent/sdk/resources";
import { createContextualLogger } from "@sdk/utils/contextual-logger.js";
import { LLM_SUMMARY_WORKFLOW_ID } from "./workflow/llm-summary.js";
import { CONTEXT_COMPRESSION_TRIGGER_NAME } from "./trigger/context-compression.js";

import { registerPredefinedTriggers, unregisterPredefinedTriggers } from "./trigger/index.js";
import { registerPredefinedWorkflows, unregisterPredefinedWorkflows } from "./workflow/index.js";
import { registerPredefinedTools, unregisterPredefinedTools } from "./tools/registration.js";

const logger = createContextualLogger({ component: "PredefinedRegistration" });

export interface PredefinedRegistrationResult {
  triggers: {
    success: string[];
    failures: Array<{ id: string; error: string }>;
  };
  workflows: {
    success: string[];
    failures: Array<{ id: string; error: string }>;
  };
  tools: {
    success: string[];
    failures: Array<{ id: string; error: string }>;
  };
}

/**
 * Register predefined content.
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

  // 1. Register predefined workflows (context compression)
  if (presets?.contextCompression?.enabled !== false) {
    try {
      const cc = presets?.contextCompression;
      results.workflows = registerPredefinedWorkflows(
        workflowRegistry,
        cc ? { config: { llmSummary: { compressionPrompt: cc.prompt, timeout: cc.timeout, maxTriggers: cc.maxTriggers } } } : undefined,
        skipIfExists,
      );
      logger.info("Predefined workflows registered", { count: results.workflows.success.length });
    } catch (error) {
      logger.error("Failed to register predefined workflows", { error });
    }
  }

  // 2. Register predefined triggers (context compression)
  // Only register triggers if the referenced workflow is registered
  if (presets?.contextCompression?.enabled !== false) {
    try {
      if (workflowRegistry.has(LLM_SUMMARY_WORKFLOW_ID)) {
        const cc = presets?.contextCompression;
        results.triggers = registerPredefinedTriggers(
          triggerRegistry,
          cc ? { config: { llmSummary: { compressionPrompt: cc.prompt, timeout: cc.timeout, maxTriggers: cc.maxTriggers } } } : undefined,
          skipIfExists,
        );
        logger.info("Predefined triggers registered", { count: results.triggers.success.length });
      } else {
        logger.warn("Skipped trigger registration: referenced workflow not registered", {
          workflowId: LLM_SUMMARY_WORKFLOW_ID,
          triggerName: CONTEXT_COMPRESSION_TRIGGER_NAME,
        });
      }
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
      logger.info("Predefined tools registered", { count: results.tools.success.length });
    } catch (error) {
      logger.error("Failed to register predefined tools", { error });
    }
  }

  return results;
}

/**
 * Unregister predefined content.
 *
 * Note: Unregistration is best-effort. Partial failure is acceptable.
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
  const results: PredefinedRegistrationResult = {
    triggers: {
      success: [] as string[],
      failures: [] as Array<{ id: string; error: string }>,
    },
    workflows: {
      success: [] as string[],
      failures: [] as Array<{ id: string; error: string }>,
    },
    tools: {
      success: [] as string[],
      failures: [] as Array<{ id: string; error: string }>,
    },
  };

  // 1. Unregister predefined triggers first (they reference workflows)
  try {
    results.triggers = await unregisterPredefinedTriggers(triggerRegistry, options?.triggerNames);
    logger.info("Predefined triggers unregistered");
  } catch (error) {
    logger.error("Failed to unregister predefined triggers", { error });
  }

  // 2. Unregister predefined workflows
  try {
    results.workflows = await unregisterPredefinedWorkflows(workflowRegistry, options?.workflowIds);
    logger.info("Predefined workflows unregistered");
  } catch (error) {
    logger.error("Failed to unregister predefined workflows", { error });
  }

  // 3. Unregister predefined tools
  try {
    results.tools = await unregisterPredefinedTools(toolService, options?.toolIds);
    logger.info("Predefined tools unregistered");
  } catch (error) {
    logger.error("Failed to unregister predefined tools", { error });
  }

  return results;
}
