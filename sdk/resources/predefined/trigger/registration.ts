/**
 * Predefined Trigger Registration Entrance
 *
 * Responsible for the registration and deregistration of predefined triggers.
 */

import type { TriggerTemplateRegistry } from "@sdk/core/registry/trigger-template-registry.js";
import { createContextualLogger } from "@sdk/utils/contextual-logger.js";
import { createPredefinedTriggers } from "./registry.js";
import type { PredefinedTriggersOptions } from "./types.js";

const logger = createContextualLogger({ component: "PredefinedTriggers" });

/**
 * Register predefined triggers
 *
 * @param registry: The trigger template registry
 * @param options: Configuration options
 * @param skipIfExists: Whether to skip the registration if the trigger already exists (instead of reporting an error)
 * @returns: The registration result
 */
export function registerPredefinedTriggers(
  registry: TriggerTemplateRegistry,
  options?: PredefinedTriggersOptions,
  skipIfExists: boolean = true,
): {
  success: string[];
  failures: Array<{ triggerName: string; error: string }>;
} {
  const success: string[] = [];
  const failures: Array<{ triggerName: string; error: string }> = [];

  try {
    // Create predefined triggers
    const triggers = createPredefinedTriggers(options);

    // Register with the trigger registry.
    for (const trigger of triggers) {
      try {
        // Check if it already exists.
        if (skipIfExists && registry.has(trigger.name)) {
          logger.info(`Trigger already registered, skipping: ${trigger.name}`);
          continue;
        }

        // Register the trigger.
        registry.register(trigger);
        success.push(trigger.name);
        logger.info(`Registered predefined trigger: ${trigger.name}`);
      } catch (error) {
        const errorMsg = error instanceof Error ? error.message : String(error);
        failures.push({ triggerName: trigger.name, error: errorMsg });
        logger.error(`Failed to register predefined trigger: ${trigger.name}`, { error: errorMsg });
      }
    }

    logger.info(
      `Predefined triggers registration completed: ${success.length} succeeded, ${failures.length} failed`,
    );
  } catch (error) {
    logger.error(`Failed to create predefined triggers`, { error });
  }

  return { success, failures };
}

/**
 * Unregister predefined triggers
 *
 * @param registry: Trigger template registry
 * @param triggerNames: List of trigger names to be unregistered; if empty, all predefined triggers will be unregistered
 * @returns: Unregistration result
 */
export async function unregisterPredefinedTriggers(
  registry: TriggerTemplateRegistry,
  triggerNames?: string[],
): Promise<{
  success: string[];
  failures: Array<{ triggerName: string; error: string }>;
}> {
  const success: string[] = [];
  const failures: Array<{ triggerName: string; error: string }> = [];

  // If no trigger name is specified, retrieve all predefined trigger names.
  const predefinedTriggerNames = triggerNames || ["context_compression_trigger"];

  for (const triggerName of predefinedTriggerNames) {
    try {
      if (registry.has(triggerName)) {
        await registry.unregister(triggerName);
        success.push(triggerName);
        logger.info(`Unregistered predefined trigger: ${triggerName}`);
      }
    } catch (error) {
      const errorMsg = error instanceof Error ? error.message : String(error);
      failures.push({ triggerName, error: errorMsg });
      logger.error(`Failed to unregister predefined trigger: ${triggerName}`, { error: errorMsg });
    }
  }

  logger.info(
    `Predefined triggers unregistration completed: ${success.length} succeeded, ${failures.length} failed`,
  );
  return { success, failures };
}

/**
 * Check if a predefined trigger has been registered.
 */
export function isPredefinedTriggerRegistered(
  registry: TriggerTemplateRegistry,
  triggerName: string,
): boolean {
  return registry.has(triggerName);
}
