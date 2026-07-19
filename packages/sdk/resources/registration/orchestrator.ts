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
import { StarterRegistry } from "../predefined/starter/starter-registry.js";
import { GoalReviewStarter } from "../predefined/starter/starters/goal-review-starter.js";

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
 * @param starterConfig Optional starter configuration (starter id + config pairs)
 * @returns Result with outcomes from all three pipelines
 */
export async function registerAllResources(
  registries: ResourceRegistries,
  presets?: PresetsConfig,
  customResources?: CustomResources,
  applicationResources?: unknown,
  skipIfExists: boolean = true,
  starterConfig?: Array<{ id: string; config: Record<string, unknown> }>,
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
      const customResult = await registerCustomResources(
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

  // =====================================================================
  // Pipeline 4: Activate Predefined Starters
  // =====================================================================
  if (starterConfig && starterConfig.length > 0) {
    try {
      logger.debug("Starting predefined starter activation pipeline");

      // Register built-in starters
      const registry = new StarterRegistry();
      registry.register(new GoalReviewStarter());

      // Build StarterRegistries from SDK registries
      const sdkRegistries = {
        workflowRegistry: {
          register: (wf: any) => registries.workflowRegistry.register(wf),
          unregister: (id: string) => registries.workflowRegistry.unregister(id),
        },
        agentLoopRegistry: {
          register: (loop: any) => registries.agentLoopRegistry.register(loop),
          unregister: (id: string) => registries.agentLoopRegistry.unregister(id),
        },
        nodeTemplateRegistry: {
          register: (nt: any) => registries.nodeTemplateRegistry.register(nt),
          unregister: (name: string) => registries.nodeTemplateRegistry.unregister(name),
        },
        triggerTemplateRegistry: {
          register: (tt: any) => registries.triggerRegistry.register(tt),
          unregister: (name: string) => registries.triggerRegistry.unregister(name),
        },
        hookTemplateRegistry: {
          register: (ht: any) => registries.hookTemplateRegistry.register(ht),
          unregister: (name: string) => registries.hookTemplateRegistry.unregister(name),
        },
        promptTemplateRegistry: {
          register: (key: string, pt: any) => registries.promptTemplateRegistry.register(key, pt),
          unregister: (key: string) => registries.promptTemplateRegistry.unregister(key),
        },
      };

      for (const sc of starterConfig) {
        try {
          await registry.activate(sc.id, sc.config, sdkRegistries);
          logger.info("Starter activated", { starterId: sc.id });
        } catch (error) {
          logger.error("Failed to activate starter", {
            starterId: sc.id,
            error: error instanceof Error ? error.message : String(error),
          });
        }
      }

      logger.info("Predefined starter activation completed", {
        count: starterConfig.length,
      });
    } catch (error) {
      logger.error("Failed during starter activation pipeline", { error });
    }
  }

  logger.info("Resources registration completed");
  return result;
}
