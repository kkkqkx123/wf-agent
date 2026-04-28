/**
 * Context Compression Trigger Module
 *
 * Provides trigger definition and registration functionality
 * Combines the definition layer with the execution layer, simplifying the architecture
 */

import type { TriggerTemplate } from "@wf-agent/types";
import { now } from "@wf-agent/common-utils";
import type { TriggerTemplateRegistry } from "../../../core/registry/trigger-template-registry.js";
import { CONTEXT_COMPRESSION_WORKFLOW_ID } from "../workflow/context-compression.js";
import { createContextualLogger } from "../../../utils/contextual-logger.js";

const logger = createContextualLogger({ component: "ContextCompressionTrigger" });

/**
 * Context Compression Trigger Template Name
 */
export const CONTEXT_COMPRESSION_TRIGGER_NAME = "context_compression_trigger";

/**
 * Context compression configuration options
 */
export interface ContextCompressionConfig {
  /** Custom compression hint words */
  compressionPrompt?: string;
  /** Timeout duration (in milliseconds), default is 60000 */
  timeout?: number;
  /** Maximum number of triggers (0 indicates no limit); the default value is 0. */
  maxTriggers?: number;
}

/**
 * Create a predefined context compression trigger template
 */
export function createContextCompressionTriggerTemplate(): TriggerTemplate {
  return {
    name: CONTEXT_COMPRESSION_TRIGGER_NAME,
    description:
      "Automatically triggers a context compression sub-workflow when Token usage exceeds the limit",
    condition: {
      eventType: "CONTEXT_COMPRESSION_REQUESTED",
    },
    action: {
      type: "execute_triggered_subgraph",
      parameters: {
        triggeredWorkflowId: CONTEXT_COMPRESSION_WORKFLOW_ID,
        waitForCompletion: true,
        timeout: 60000,
        recordHistory: false,
      },
    },
    enabled: true,
    maxTriggers: 0,
    metadata: {
      category: "system",
      tags: ["context", "compression", "token", "memory"],
    },
    createdAt: now(),
    updatedAt: now(),
  };
}

/**
 * Create a custom configuration for context compression trigger templates.
 */
export function createCustomContextCompressionTrigger(
  config: ContextCompressionConfig = {},
): TriggerTemplate {
  const template = createContextCompressionTriggerTemplate();

  if (config.timeout !== undefined) {
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    (template.action.parameters as any)["timeout"] = config.timeout;
  }

  if (config.maxTriggers !== undefined) {
    template.maxTriggers = config.maxTriggers;
  }

  const metadata = template.metadata || {};
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  (metadata as any)["customConfig"] = config;
  template.metadata = metadata;

  template.updatedAt = now();
  return template;
}

/**
 * Register context compression trigger
 */
export function registerContextCompressionTrigger(
  registry: TriggerTemplateRegistry,
  config?: ContextCompressionConfig,
  skipIfExists: boolean = true,
): boolean {
  try {
    const template = config
      ? createCustomContextCompressionTrigger(config)
      : createContextCompressionTriggerTemplate();

    // Check if the template already exists
    if (registry.has(template.name)) {
      if (skipIfExists) {
        logger.info("Context compression trigger already exists, skipping registration");
        return false;
      }
      // If skipIfExists is false, let the registry.register throw the error
    }

    registry.register(template, { skipIfExists });
    logger.info("Registered context compression trigger");
    return true;
  } catch (error) {
    logger.error("Failed to register context compression trigger", { error });
    // Re-throw the error if skipIfExists is false (the caller expects it)
    if (!skipIfExists) {
      throw error;
    }
    return false;
  }
}

/**
 * Cancel the context compression trigger.
 */
export function unregisterContextCompressionTrigger(registry: TriggerTemplateRegistry): boolean {
  try {
    if (registry.has(CONTEXT_COMPRESSION_TRIGGER_NAME)) {
      registry.unregister(CONTEXT_COMPRESSION_TRIGGER_NAME);
      logger.info("Unregistered context compression trigger");
      return true;
    }
    return false;
  } catch (error) {
    logger.error("Failed to unregister context compression trigger", { error });
    return false;
  }
}

/**
 * Check whether the context compression trigger has been registered.
 */
export function isContextCompressionTriggerRegistered(registry: TriggerTemplateRegistry): boolean {
  return registry.has(CONTEXT_COMPRESSION_TRIGGER_NAME);
}
