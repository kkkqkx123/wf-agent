/**
 * Node template storage utilities - Module-level functions for node template persistence.
 * Extracted from NodeTemplateRegistry to isolate storage concerns.
 * Dependencies (NodeTemplateStorageAdapter) are passed as function parameters.
 */

import type { NodeTemplate } from "@wf-agent/types";
import type { NodeTemplateStorageAdapter } from "@wf-agent/storage";
import { createContextualLogger } from "../../../utils/contextual-logger.js";
import { getErrorMessage } from "@wf-agent/common-utils";

const logger = createContextualLogger();

/**
 * Persist node template to storage (if adapter is available)
 * @param template Node template to persist
 * @param adapter Storage adapter or null
 */
export async function persistNodeTemplate(
  template: NodeTemplate,
  adapter?: NodeTemplateStorageAdapter | null,
): Promise<void> {
  if (!adapter) {
    logger.debug("No storage adapter configured, skipping node template persistence");
    return;
  }

  try {
    const encoder = new TextEncoder();
    const data = encoder.encode(JSON.stringify(template));

    const metadata = {
      name: template.name,
      type: template.type,
      description: template.description || "",
      tags: (template.metadata?.["tags"] as string[]) || [],
      category: (template.metadata?.["category"] as string) || "",
      createdAt: template.createdAt,
      updatedAt: template.updatedAt,
    };

    await adapter.save(template.name, data, metadata);
    logger.debug("Node template persisted successfully", { name: template.name });
  } catch (error) {
    logger.error("Failed to persist node template", {
      name: template.name,
      operation: "persist",
      storageType: adapter.constructor.name,
      error: getErrorMessage(error),
    });
    throw error;
  }
}

/**
 * Remove node template from storage
 * @param name Node template name to remove
 * @param adapter Storage adapter or null
 */
export async function removeNodeTemplate(
  name: string,
  adapter?: NodeTemplateStorageAdapter | null,
): Promise<void> {
  if (!adapter) {
    return;
  }

  try {
    await adapter.delete(name);
    logger.debug("Node template removed from storage", { name });
  } catch (error) {
    logger.error("Failed to remove node template from storage", {
      name,
      operation: "remove",
      storageType: adapter.constructor.name,
      error: getErrorMessage(error),
    });
  }
}

/**
 * Load node template from storage
 * @param name Node template name to load
 * @param adapter Storage adapter or null
 * @returns Node template or null
 */
export async function loadNodeTemplate(
  name: string,
  adapter?: NodeTemplateStorageAdapter | null,
): Promise<NodeTemplate | null> {
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
    return JSON.parse(json) as NodeTemplate;
  } catch (error) {
    logger.error("Failed to load node template from storage", {
      name,
      operation: "load",
      storageType: adapter.constructor.name,
      error: getErrorMessage(error),
    });
    return null;
  }
}

/**
 * Initialize node templates map from storage.
 * Loads all node templates from storage into the provided map.
 * @param adapter Storage adapter or null
 * @param templates Map to populate with loaded node templates
 */
export async function initializeNodeTemplatesFromStorage(
  adapter: NodeTemplateStorageAdapter | null,
  templates: Map<string, NodeTemplate>,
): Promise<void> {
  if (!adapter) {
    logger.debug(
      "No storage adapter configured, skipping node template initialization from storage",
    );
    return;
  }

  try {
    const ids = await adapter.list();

    logger.info("Initializing node templates from storage", {
      count: ids.length,
      storageType: adapter.constructor.name,
    });

    for (const id of ids) {
      try {
        const template = await loadNodeTemplate(id, adapter);
        if (template) {
          templates.set(id, template);
        }
      } catch (error) {
        logger.error("Failed to load node template from storage", {
          name: id,
          error: getErrorMessage(error),
        });
      }
    }

    logger.info("Node template initialization complete", {
      loadedCount: templates.size,
    });
  } catch (error) {
    logger.error("Failed to initialize node templates from storage", {
      error: getErrorMessage(error),
      storageType: adapter.constructor.name,
      operation: "initializeFromStorage",
    });
  }
}
