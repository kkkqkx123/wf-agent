/**
 * Hook template storage utilities - Module-level functions for hook template persistence.
 * Extracted from HookTemplateRegistry to isolate storage concerns.
 * Dependencies (HookTemplateStorageAdapter) are passed as function parameters.
 */

import type { HookTemplate } from "@wf-agent/types";
import type { HookTemplateStorageAdapter } from "@wf-agent/storage";
import { createContextualLogger } from "../../../utils/contextual-logger.js";
import { getErrorMessage } from "@wf-agent/common-utils";

const logger = createContextualLogger();

/**
 * Persist hook template to storage (if adapter is available)
 * @param template Hook template to persist
 * @param adapter Storage adapter or null
 */
export async function persistHookTemplate(
  template: HookTemplate,
  adapter?: HookTemplateStorageAdapter | null,
): Promise<void> {
  if (!adapter) {
    logger.debug("No storage adapter configured, skipping hook template persistence");
    return;
  }

  try {
    const encoder = new TextEncoder();
    const data = encoder.encode(JSON.stringify(template));

    const metadata = {
      name: template.name,
      hookType: template.hook.hookType,
      description: template.description || "",
      tags: (template.metadata?.["tags"] as string[]) || [],
      category: (template.metadata?.["category"] as string) || "",
      createdAt: template.createdAt,
      updatedAt: template.updatedAt,
    };

    await adapter.save(template.name, data, metadata);
    logger.debug("Hook template persisted successfully", { name: template.name });
  } catch (error) {
    logger.error("Failed to persist hook template", {
      name: template.name,
      operation: "persist",
      storageType: adapter.constructor.name,
      error: getErrorMessage(error),
    });
    throw error;
  }
}

/**
 * Remove hook template from storage
 * @param name Hook template name to remove
 * @param adapter Storage adapter or null
 */
export async function removeHookTemplate(
  name: string,
  adapter?: HookTemplateStorageAdapter | null,
): Promise<void> {
  if (!adapter) {
    return;
  }

  try {
    await adapter.delete(name);
    logger.debug("Hook template removed from storage", { name });
  } catch (error) {
    logger.error("Failed to remove hook template from storage", {
      name,
      operation: "remove",
      storageType: adapter.constructor.name,
      error: getErrorMessage(error),
    });
  }
}

/**
 * Load hook template from storage
 * @param name Hook template name to load
 * @param adapter Storage adapter or null
 * @returns Hook template or null
 */
export async function loadHookTemplate(
  name: string,
  adapter?: HookTemplateStorageAdapter | null,
): Promise<HookTemplate | null> {
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
    return JSON.parse(json) as HookTemplate;
  } catch (error) {
    logger.error("Failed to load hook template from storage", {
      name,
      operation: "load",
      storageType: adapter.constructor.name,
      error: getErrorMessage(error),
    });
    return null;
  }
}

/**
 * Initialize hook templates map from storage.
 * Loads all hook templates from storage into the provided map.
 * @param adapter Storage adapter or null
 * @param templates Map to populate with loaded hook templates
 */
export async function initializeHookTemplatesFromStorage(
  adapter: HookTemplateStorageAdapter | null,
  templates: Map<string, HookTemplate>,
): Promise<void> {
  if (!adapter) {
    logger.debug(
      "No storage adapter configured, skipping hook template initialization from storage",
    );
    return;
  }

  try {
    const ids = await adapter.list();

    logger.info("Initializing hook templates from storage", {
      count: ids.length,
      storageType: adapter.constructor.name,
    });

    for (const id of ids) {
      try {
        const template = await loadHookTemplate(id, adapter);
        if (template) {
          templates.set(id, template);
        }
      } catch (error) {
        logger.error("Failed to load hook template from storage", {
          name: id,
          error: getErrorMessage(error),
        });
      }
    }

    logger.info("Hook template initialization complete", {
      loadedCount: templates.size,
    });
  } catch (error) {
    logger.error("Failed to initialize hook templates from storage", {
      error: getErrorMessage(error),
      storageType: adapter.constructor.name,
      operation: "initializeFromStorage",
    });
  }
}
