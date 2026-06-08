/**
 * Agent profile storage utilities - Module-level functions for agent profile persistence.
 * Extracted from AgentProfileRegistry to isolate storage concerns.
 * Dependencies (AgentProfileStorageAdapter) are passed as function parameters.
 */

import type { AgentProfileStorageAdapter } from "@wf-agent/storage";
import type { AgentProfileMeta } from "../agent-profile-registry.js";
import { createContextualLogger } from "../../../utils/contextual-logger.js";
import { getErrorMessage } from "@wf-agent/common-utils";

const logger = createContextualLogger();

/**
 * Persist agent profile to storage (if adapter is available)
 * @param profile Agent profile to persist
 * @param adapter Storage adapter or null
 */
export async function persistAgentProfile(
  profile: AgentProfileMeta,
  adapter?: AgentProfileStorageAdapter | null,
): Promise<void> {
  if (!adapter) {
    logger.debug("No storage adapter configured, skipping agent profile persistence");
    return;
  }

  try {
    const encoder = new TextEncoder();
    const data = encoder.encode(JSON.stringify(profile));

    const metadata = {
      profileId: profile.id,
      name: profile.name,
      description: profile.description || "",
    };

    await adapter.save(profile.id, data, metadata);
    logger.debug("Agent profile persisted successfully", { profileId: profile.id });
  } catch (error) {
    logger.error("Failed to persist agent profile", {
      profileId: profile.id,
      operation: "persist",
      storageType: adapter.constructor.name,
      error: getErrorMessage(error),
    });
    throw error;
  }
}

/**
 * Remove agent profile from storage
 * @param profileId Profile ID to remove
 * @param adapter Storage adapter or null
 */
export async function removeAgentProfile(
  profileId: string,
  adapter?: AgentProfileStorageAdapter | null,
): Promise<void> {
  if (!adapter) {
    return;
  }

  try {
    await adapter.delete(profileId);
    logger.debug("Agent profile removed from storage", { profileId });
  } catch (error) {
    logger.error("Failed to remove agent profile from storage", {
      profileId,
      operation: "remove",
      storageType: adapter.constructor.name,
      error: getErrorMessage(error),
    });
  }
}

/**
 * Load agent profile from storage
 * @param profileId Profile ID to load
 * @param adapter Storage adapter or null
 * @returns Agent profile or null
 */
export async function loadAgentProfile(
  profileId: string,
  adapter?: AgentProfileStorageAdapter | null,
): Promise<AgentProfileMeta | null> {
  if (!adapter) {
    return null;
  }

  try {
    const data = await adapter.load(profileId);
    if (!data) {
      return null;
    }

    const decoder = new TextDecoder();
    const json = decoder.decode(data);
    return JSON.parse(json) as AgentProfileMeta;
  } catch (error) {
    logger.error("Failed to load agent profile from storage", {
      profileId,
      operation: "load",
      storageType: adapter.constructor.name,
      error: getErrorMessage(error),
    });
    return null;
  }
}

/**
 * Initialize agent profiles map from storage.
 * Loads all agent profiles from storage into the provided map.
 * @param adapter Storage adapter or null
 * @param profiles Map to populate with loaded agent profiles
 */
export async function initializeAgentProfilesFromStorage(
  adapter: AgentProfileStorageAdapter | null,
  profiles: Map<string, AgentProfileMeta>,
): Promise<void> {
  if (!adapter) {
    logger.debug("No storage adapter configured, skipping agent profile initialization from storage");
    return;
  }

  try {
    const ids = await adapter.list();

    logger.info("Initializing agent profiles from storage", {
      count: ids.length,
      storageType: adapter.constructor.name,
    });

    for (const id of ids) {
      try {
        const profile = await loadAgentProfile(id, adapter);
        if (profile) {
          profiles.set(id, profile);
        }
      } catch (error) {
        logger.error("Failed to load agent profile from storage", {
          profileId: id,
          error: getErrorMessage(error),
        });
      }
    }

    logger.info("Agent profile initialization complete", {
      loadedCount: profiles.size,
    });
  } catch (error) {
    logger.error("Failed to initialize agent profiles from storage", {
      error: getErrorMessage(error),
      storageType: adapter.constructor.name,
      operation: "initializeFromStorage",
    });
  }
}