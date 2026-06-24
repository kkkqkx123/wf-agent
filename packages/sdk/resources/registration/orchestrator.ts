/**
 * Unified Resources Registration Orchestrator
 *
 * Coordinates registration of resources from all three pipelines:
 * 1. Predefined resources (SDK built-in)
 * 2. Custom resources (user-provided via config files)
 * 3. Application resources (runtime-defined, reserved)
 *
 * Orchestration order:
 *   1. Register predefined resources (workflows must precede triggers)
 *   2. Register custom resources (after predefined to avoid conflicts)
 *   3. Register application resources (when needed)
 */

import { createContextualLogger } from "@sdk/utils/contextual-logger.js";
import type { TriggerTemplateRegistry } from "@sdk/shared/registry/trigger-template-registry.js";
import type { WorkflowRegistry } from "@sdk/workflow/stores/workflow-registry.js";
import type { ToolRegistry } from "@sdk/shared/registry/tool-registry.js";
import type { PresetsConfig, CustomResources } from "@wf-agent/sdk/resources";
import type { UnifiedRegistrationResult } from "./types.js";

const logger = createContextualLogger({ component: "UnifiedResourcesOrchestrator" });

/**
 * Register all resources from all three pipelines
 *
 * This is the main entry point for resource registration. It coordinates
 * the registration of predefined, custom, and application resources
 * in the correct order, ensuring no conflicts and proper error handling.
 *
 * @param triggerRegistry Trigger template registry
 * @param workflowRegistry Workflow registry
 * @param toolRegistry Tool service/registry
 * @param presets Presets configuration (predefined resources)
 * @param customResources Custom resources loaded from config files
 * @param applicationResources Application resources (reserved for future use)
 * @param skipIfExists Whether to skip if already registered
 * @returns Unified result with outcomes from all three pipelines
 */
export async function registerAllResources(
  triggerRegistry: TriggerTemplateRegistry,
  workflowRegistry: WorkflowRegistry,
  toolRegistry: ToolRegistry,
  presets?: PresetsConfig,
  customResources?: CustomResources,
  applicationResources?: unknown,
  skipIfExists: boolean = true,
): Promise<UnifiedRegistrationResult> {
  logger.info("Starting unified resources registration");

  const result: UnifiedRegistrationResult = {
    predefined: {
      tools: { success: [], failures: [] },
      triggers: { success: [], failures: [] },
      workflows: { success: [], failures: [] },
    },
    custom: {
      tools: { success: [], failures: [] },
      triggers: { success: [], failures: [] },
      prompts: { success: [], failures: [] },
    },
  };

  // =====================================================================
  // Pipeline 1: Register Predefined Resources
  // =====================================================================
  try {
    logger.debug("Starting predefined resources registration pipeline");
    const { registerPredefinedContent } = await import(
      "../predefined/registration.js"
    );
    const predefinedResult = await registerPredefinedContent(
      triggerRegistry,
      workflowRegistry,
      toolRegistry,
      presets,
      skipIfExists,
    );
    result.predefined = predefinedResult;
    logger.info("Predefined resources registration completed", {
      tools: predefinedResult.tools.success.length,
      triggers: predefinedResult.triggers.success.length,
      workflows: predefinedResult.workflows.success.length,
    });
  } catch (error) {
    logger.error("Failed during predefined resources registration", { error });
  }

  // =====================================================================
  // Pipeline 2: Register Custom Resources (after predefined)
  // =====================================================================
  if (customResources) {
    try {
      logger.debug("Starting custom resources registration pipeline");
      const { registerCustomResources } = await import(
        "../custom/registration.js"
      );
      const customResult = registerCustomResources(
        {
          toolRegistry,
          triggerRegistry,
        },
        customResources,
      );
      result.custom = customResult;
      logger.info("Custom resources registration completed", {
        tools: customResult.tools.success.length,
        triggers: customResult.triggers.success.length,
        prompts: customResult.prompts.success.length,
      });
    } catch (error) {
      logger.error("Failed during custom resources registration", { error });
    }
  }

  // =====================================================================
  // Pipeline 3: Register Application Resources (reserved)
  // =====================================================================
  if (applicationResources) {
    try {
      logger.debug("Starting application resources registration pipeline");
      // TODO: Implement application resource registration
      logger.info("Application resources registered");
    } catch (error) {
      logger.error("Failed during application resources registration", { error });
    }
  }

  logger.info("Unified resources registration completed");
  return result;
}
