/**
 * Trigger storage utilities - Module-level functions for trigger template persistence.
 * Extracted from TriggerTemplateRegistry to isolate storage concerns.
 * Dependencies (TriggerStorageAdapter) are passed as function parameters.
 */

import type { TriggerTemplate } from "@wf-agent/types";
import type { TriggerStorageAdapter } from "@wf-agent/storage";
import { createContextualLogger } from "../../../utils/contextual-logger.js";
import { getErrorMessage } from "@wf-agent/common-utils";

const logger = createContextualLogger();

/**
 * Persist trigger template to storage (if adapter is available)
 * @param template Trigger template to persist
 * @param adapter Storage adapter or null
 */
export async function persistTrigger(
  template: TriggerTemplate,
  adapter?: TriggerStorageAdapter | null,
): Promise<void> {
  if (!adapter) {
    logger.debug("No storage adapter configured, skipping trigger persistence");
    return;
  }

  try {
    const encoder = new TextEncoder();
    const data = encoder.encode(JSON.stringify(template));

    const metadata = {
      name: template.name,
      description: template.description || "",
      enabled: template.enabled ?? true,
      createdAt: template.createdAt,
      updatedAt: template.updatedAt,
      tags: (template.metadata?.["tags"] as string[]) || [],
      category: (template.metadata?.["category"] as string) || "",
    };

    await adapter.save(template.name, data, metadata);
    logger.debug("Trigger template persisted successfully", { name: template.name });
  } catch (error) {
    logger.error("Failed to persist trigger template", {
      name: template.name,
      operation: "persist",
      storageType: adapter.constructor.name,
      error: getErrorMessage(error),
    });
    throw error;
  }
}

/**
 * Remove trigger template from storage
 * @param name Trigger template name to remove
 * @param adapter Storage adapter or null
 */
export async function removeTrigger(
  name: string,
  adapter?: TriggerStorageAdapter | null,
): Promise<void> {
  if (!adapter) {
    return;
  }

  try {
    await adapter.delete(name);
    logger.debug("Trigger template removed from storage", { name });
  } catch (error) {
    logger.error("Failed to remove trigger template from storage", {
      name,
      operation: "remove",
      storageType: adapter.constructor.name,
      error: getErrorMessage(error),
    });
  }
}

/**
 * Load trigger template from storage
 * @param name Trigger template name to load
 * @param adapter Storage adapter or null
 * @returns Trigger template or null
 */
export async function loadTrigger(
  name: string,
  adapter?: TriggerStorageAdapter | null,
): Promise<TriggerTemplate | null> {
  if (!adapter) {
    return null;
  }

  try {
    const data = await adapter.load(name);
    if (!data) {
      return null;
    }

    const decoder = new TextDecoder();
    const json = decoder.decode(data);
    return JSON.parse(json) as TriggerTemplate;
  } catch (error) {
    logger.error("Failed to load trigger template from storage", {
      name,
      operation: "load",
      storageType: adapter.constructor.name,
      error: getErrorMessage(error),
    });
    return null;
  }
}

/**
 * Initialize triggers map from storage.
 * Loads all trigger templates from storage into the provided map.
 * @param adapter Storage adapter or null
 * @param templates Map to populate with loaded trigger templates
 */
export async function initializeTriggersFromStorage(
  adapter: TriggerStorageAdapter | null,
  templates: Map<string, TriggerTemplate>,
): Promise<void> {
  if (!adapter) {
    logger.debug("No storage adapter configured, skipping trigger initialization from storage");
    return;
  }

  try {
    const ids = await adapter.list();

    logger.info("Initializing trigger templates from storage", {
      count: ids.length,
      storageType: adapter.constructor.name,
    });

    for (const id of ids) {
      try {
        const template = await loadTrigger(id, adapter);
        if (template) {
          templates.set(id, template);
        }
      } catch (error) {
        logger.error("Failed to load trigger template from storage", {
          name: id,
          error: getErrorMessage(error),
        });
      }
    }

    logger.info("Trigger template initialization complete", {
      loadedCount: templates.size,
    });
  } catch (error) {
    logger.error("Failed to initialize trigger templates from storage", {
      error: getErrorMessage(error),
      storageType: adapter.constructor.name,
      operation: "initializeFromStorage",
    });
  }
}