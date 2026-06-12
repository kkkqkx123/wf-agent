/**
 * Tool storage utilities - Module-level functions for tool persistence.
 * Extracted from ToolRegistry to isolate storage concerns.
 * Dependencies (ToolStorageAdapter) are passed as function parameters.
 */

import type { Tool } from "@wf-agent/types";
import type { ToolStorageAdapter } from "@wf-agent/storage";
import { createContextualLogger } from "../../../utils/contextual-logger.js";
import { getErrorMessage } from "@wf-agent/common-utils";

const logger = createContextualLogger();

/**
 * Persist tool to storage (if adapter is available)
 * @param tool Tool definition to persist
 * @param adapter Storage adapter or null
 */
export async function persistTool(tool: Tool, adapter?: ToolStorageAdapter | null): Promise<void> {
  if (!adapter) {
    logger.debug("No storage adapter configured, skipping tool persistence");
    return;
  }

  try {
    const encoder = new TextEncoder();
    const data = encoder.encode(JSON.stringify(tool));

    const metadata = {
      toolId: tool.id,
      type: tool.type,
      description: tool.description || "",
      tags: tool.metadata?.tags || [],
      category: tool.metadata?.category || "",
    };

    await adapter.save(tool.id, data, metadata);
    logger.debug("Tool persisted successfully", { toolId: tool.id });
  } catch (error) {
    logger.error("Failed to persist tool", {
      toolId: tool.id,
      operation: "persist",
      storageType: adapter.constructor.name,
      error: getErrorMessage(error),
    });
    throw error;
  }
}

/**
 * Remove tool from storage
 * @param toolId Tool ID to remove
 * @param adapter Storage adapter or null
 */
export async function removeTool(
  toolId: string,
  adapter?: ToolStorageAdapter | null,
): Promise<void> {
  if (!adapter) {
    return;
  }

  try {
    await adapter.delete(toolId);
    logger.debug("Tool removed from storage", { toolId });
  } catch (error) {
    logger.error("Failed to remove tool from storage", {
      toolId,
      operation: "remove",
      storageType: adapter.constructor.name,
      error: getErrorMessage(error),
    });
  }
}

/**
 * Load tool from storage
 * @param toolId Tool ID to load
 * @param adapter Storage adapter or null
 * @returns Tool definition or null
 */
export async function loadTool(
  toolId: string,
  adapter?: ToolStorageAdapter | null,
): Promise<Tool | null> {
  if (!adapter) {
    return null;
  }

  try {
    const data = await adapter.load(toolId);
    if (!data) {
      return null;
    }

    const decoder = new TextDecoder();
    const json = decoder.decode(data);
    return JSON.parse(json) as Tool;
  } catch (error) {
    logger.error("Failed to load tool from storage", {
      toolId,
      operation: "load",
      storageType: adapter.constructor.name,
      error: getErrorMessage(error),
    });
    return null;
  }
}

/**
 * Initialize tools map from storage.
 * Loads all tools from storage into the provided map.
 * @param adapter Storage adapter or null
 * @param tools Map to populate with loaded tool definitions
 */
export async function initializeToolsFromStorage(
  adapter: ToolStorageAdapter | null,
  tools: Map<string, Tool>,
): Promise<void> {
  if (!adapter) {
    logger.debug("No storage adapter configured, skipping tool initialization from storage");
    return;
  }

  try {
    const ids = await adapter.list();

    logger.info("Initializing tools from storage", {
      count: ids.length,
      storageType: adapter.constructor.name,
    });

    for (const id of ids) {
      try {
        const tool = await loadTool(id, adapter);
        if (tool) {
          tools.set(id, tool);
        }
      } catch (error) {
        logger.error("Failed to load tool from storage", {
          toolId: id,
          error: getErrorMessage(error),
        });
      }
    }

    logger.info("Tool initialization complete", {
      loadedCount: tools.size,
    });
  } catch (error) {
    logger.error("Failed to initialize tools from storage", {
      error: getErrorMessage(error),
      storageType: adapter.constructor.name,
      operation: "initializeFromStorage",
    });
  }
}
