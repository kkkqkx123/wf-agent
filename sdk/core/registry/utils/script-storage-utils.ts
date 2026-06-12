/**
 * Script storage utilities - Module-level functions for script persistence.
 * Extracted from ScriptRegistry to isolate storage concerns.
 * Dependencies (ScriptStorageAdapter) are passed as function parameters.
 */

import type { Script } from "@wf-agent/types";
import type { ScriptStorageAdapter } from "@wf-agent/storage";
import { createContextualLogger } from "../../../utils/contextual-logger.js";
import { getErrorMessage } from "@wf-agent/common-utils";

const logger = createContextualLogger();

/**
 * Persist script to storage (if adapter is available)
 * @param script Script definition to persist
 * @param adapter Storage adapter or null
 */
export async function persistScript(
  script: Script,
  adapter?: ScriptStorageAdapter | null,
): Promise<void> {
  if (!adapter) {
    logger.debug("No storage adapter configured, skipping script persistence");
    return;
  }

  try {
    const encoder = new TextEncoder();
    const data = encoder.encode(JSON.stringify(script));

    const metadata = {
      name: script.name,
      description: script.description || "",
      enabled: script.enabled ?? true,
      tags: script.metadata?.tags || [],
      category: script.metadata?.category || "",
      createdAt: 0,
      updatedAt: 0,
    };

    await adapter.save(script.name, data, metadata);
    logger.debug("Script persisted successfully", { scriptName: script.name });
  } catch (error) {
    logger.error("Failed to persist script", {
      scriptName: script.name,
      operation: "persist",
      storageType: adapter.constructor.name,
      error: getErrorMessage(error),
    });
    throw error;
  }
}

/**
 * Remove script from storage
 * @param scriptName Script name to remove
 * @param adapter Storage adapter or null
 */
export async function removeScript(
  scriptName: string,
  adapter?: ScriptStorageAdapter | null,
): Promise<void> {
  if (!adapter) {
    return;
  }

  try {
    await adapter.delete(scriptName);
    logger.debug("Script removed from storage", { scriptName });
  } catch (error) {
    logger.error("Failed to remove script from storage", {
      scriptName,
      operation: "remove",
      storageType: adapter.constructor.name,
      error: getErrorMessage(error),
    });
  }
}

/**
 * Load script from storage
 * @param scriptName Script name to load
 * @param adapter Storage adapter or null
 * @returns Script definition or null
 */
export async function loadScript(
  scriptName: string,
  adapter?: ScriptStorageAdapter | null,
): Promise<Script | null> {
  if (!adapter) {
    return null;
  }

  try {
    const data = await adapter.load(scriptName);
    if (!data) {
      return null;
    }

    const decoder = new TextDecoder();
    const json = decoder.decode(data);
    return JSON.parse(json) as Script;
  } catch (error) {
    logger.error("Failed to load script from storage", {
      scriptName,
      operation: "load",
      storageType: adapter.constructor.name,
      error: getErrorMessage(error),
    });
    return null;
  }
}

/**
 * Initialize scripts map from storage.
 * Loads all scripts from storage into the provided map.
 * @param adapter Storage adapter or null
 * @param scripts Map to populate with loaded script definitions
 */
export async function initializeScriptsFromStorage(
  adapter: ScriptStorageAdapter | null,
  scripts: Map<string, Script>,
): Promise<void> {
  if (!adapter) {
    logger.debug("No storage adapter configured, skipping script initialization from storage");
    return;
  }

  try {
    const ids = await adapter.list();

    logger.info("Initializing scripts from storage", {
      count: ids.length,
      storageType: adapter.constructor.name,
    });

    for (const id of ids) {
      try {
        const script = await loadScript(id, adapter);
        if (script) {
          scripts.set(id, script);
        }
      } catch (error) {
        logger.error("Failed to load script from storage", {
          scriptName: id,
          error: getErrorMessage(error),
        });
      }
    }

    logger.info("Script initialization complete", {
      loadedCount: scripts.size,
    });
  } catch (error) {
    logger.error("Failed to initialize scripts from storage", {
      error: getErrorMessage(error),
      storageType: adapter.constructor.name,
      operation: "initializeFromStorage",
    });
  }
}
