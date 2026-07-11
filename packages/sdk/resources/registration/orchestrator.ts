/**
 * Resources Registration Orchestrator
 *
 * Coordinates registration of resources from all three pipelines:
 * 1. Predefined resources (SDK built-in)
 * 2. Custom resources (user-provided via config files)
 * 3. Application resources (runtime-defined, reserved)
 *
 * Orchestration order:
 *   1. Register predefined resources (fragments → prompts → toolDescriptions → workflows → triggers → tools)
 *   2. Register custom resources (after predefined to avoid conflicts)
 *   3. Register application resources (when needed)
 */

import type { PresetsConfig, CustomResources } from "@wf-agent/sdk/resources";
import type { RegistrationResult, ResourceRegistries } from "./types.js";
import { createContextualLogger } from "../../utils/contextual-logger.js";
import { registerPredefinedContent } from "../predefined/registration.js";
import { registerAllPredefinedPrompts } from "./prompts-registration.js";
import { registerAllPredefinedToolDescriptions } from "./tool-descriptions-registration.js";
import { registerCustomResources } from "../custom/registration.js";

const logger = createContextualLogger({ component: "ResourcesOrchestrator" });

/**
 * Register all resources from all three pipelines
 *
 * This is the main entry point for resource registration. It coordinates
 * the registration of predefined, custom, and application resources
 * in the correct order, ensuring no conflicts and proper error handling.
 *
 * @param registries All registry instances required for registration
 * @param presets Presets configuration (predefined resources)
 * @param customResources Custom resources loaded from config files
 * @param applicationResources Application resources (reserved for future use)
 * @param skipIfExists Whether to skip if already registered
 * @returns Result with outcomes from all three pipelines
 */
export async function registerAllResources(
  registries: ResourceRegistries,
  presets?: PresetsConfig,
  customResources?: CustomResources,
  applicationResources?: unknown,
  skipIfExists: boolean = true,
): Promise<RegistrationResult> {
  logger.info("Starting resources registration");

  const result: RegistrationResult = {
    predefined: {
      prompts: { success: [], failures: [] },
      fragments: { success: [], failures: [] },
      toolDescriptions: { success: [], failures: [] },
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

    const promptsEnabled = presets?.predefinedPrompts?.enabled ?? true;
    const toolDescriptionsEnabled = presets?.predefinedToolDescriptions?.enabled ?? true;

    // 1.1 Register fragments and prompts first (prompts depend on fragments)
    if (promptsEnabled) {
      try {
        const promptResult = registerAllPredefinedPrompts(
          registries.promptTemplateRegistry,
          registries.fragmentRegistry,
          { skipIfExists },
        );
        result.predefined.fragments = promptResult.fragments;
        result.predefined.prompts = promptResult.templates;
        logger.info("Predefined prompts registered", {
          fragments: promptResult.fragments.success.length,
          templates: promptResult.templates.success.length,
        });
      } catch (error) {
        logger.error("Failed to register predefined prompts", { error });
      }
    }

    // 1.2 Register tool descriptions
    if (toolDescriptionsEnabled) {
      try {
        result.predefined.toolDescriptions = registerAllPredefinedToolDescriptions(
          registries.toolDescriptionRegistry,
          { skipIfExists },
        );
        logger.info("Predefined tool descriptions registered", {
          count: result.predefined.toolDescriptions.success.length,
        });
      } catch (error) {
        logger.error("Failed to register predefined tool descriptions", { error });
      }
    }

    // 1.3 Register workflows, triggers, tools (internal dependencies)
    const predefinedResult = await registerPredefinedContent(
      registries.triggerRegistry,
      registries.workflowRegistry,
      registries.toolRegistry,
      presets,
      skipIfExists,
    );
    result.predefined.workflows = predefinedResult.workflows;
    result.predefined.triggers = predefinedResult.triggers;
    result.predefined.tools = predefinedResult.tools;
    logger.info("Predefined content registration completed", {
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
      const customResult = registerCustomResources(
        {
          toolRegistry: registries.toolRegistry,
          triggerRegistry: registries.triggerRegistry,
          promptRegistry: registries.promptTemplateRegistry,
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
      logger.info("Application resources registered");
    } catch (error) {
      logger.error("Failed during application resources registration", { error });
    }
  }

  logger.info("Resources registration completed");
  return result;
}
