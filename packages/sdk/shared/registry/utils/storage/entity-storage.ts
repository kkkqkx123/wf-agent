/**
 * Consolidated Entity Storage
 *
 * Unified storage operations for all entity types.
 * Merges generic-storage-utils and entity-storage-utils into a single module.
 */

import type { BaseStorageAdapter } from "@wf-agent/storage";
import {
  AgentProfileStorageAdapter,
  HookTemplateStorageAdapter,
  NodeTemplateStorageAdapter,
  ScriptStorageAdapter,
  ToolStorageAdapter,
  TriggerStorageAdapter,
} from "@wf-agent/storage";
import type {
  AgentProfileStorageMetadata,
  HookTemplateStorageMetadata,
  NodeTemplateStorageMetadata,
  ScriptStorageMetadata,
  ToolStorageMetadata,
  TriggerStorageMetadata,
  HookTemplate,
  NodeTemplate,
  Script,
  Tool,
  TriggerTemplate,
} from "@wf-agent/types";
import type { AgentProfileMeta } from "../../agent-profile-registry.js";
import { createContextualLogger } from "../../../../utils/contextual-logger.js";
import { getErrorMessage } from "@wf-agent/common-utils";
import {
  agentProfileMetadataBuilder,
  hookTemplateMetadataBuilder,
  nodeTemplateMetadataBuilder,
  scriptMetadataBuilder,
  toolMetadataBuilder,
  triggerTemplateMetadataBuilder,
} from "./metadata-builders.js";

const logger = createContextualLogger();

/**
 * Entity descriptor for generic storage operations.
 * Encapsulates all entity-specific knowledge needed for persistence.
 */
export interface StorageEntityInfo<T, TMetadata> {
  getId: (entity: T) => string;
  buildMetadata: (entity: T) => TMetadata;
  entityName: string;
}

// ==================== Generic Storage Operations ====================

/**
 * Persist an item to storage (write-through).
 * Serializes the item as JSON and saves via the adapter.
 */
export async function persistItem<T, TMetadata>(
  item: T,
  adapter: BaseStorageAdapter<TMetadata, void> | null | undefined,
  info: StorageEntityInfo<T, TMetadata>,
): Promise<void> {
  if (!adapter) {
    logger.debug(`No storage adapter configured, skipping ${info.entityName} persistence`);
    return;
  }

  const id = info.getId(item);

  try {
    const encoder = new TextEncoder();
    const data = encoder.encode(JSON.stringify(item));
    const metadata = info.buildMetadata(item);

    await adapter.save(id, data, metadata);
    logger.debug(`${capitalize(info.entityName)} persisted successfully`, { id });
  } catch (error) {
    logger.error(`Failed to persist ${info.entityName}`, {
      id,
      operation: "persist",
      storageType: adapter.constructor.name,
      error: getErrorMessage(error),
    });
    throw error;
  }
}

/**
 * Remove an item from storage.
 */
export async function removeItem<TMetadata>(
  id: string,
  adapter: BaseStorageAdapter<TMetadata, void> | null | undefined,
  entityName: string,
): Promise<void> {
  if (!adapter) {
    return;
  }

  try {
    await adapter.delete(id);
    logger.debug(`${capitalize(entityName)} removed from storage`, { id });
  } catch (error) {
    logger.error(`Failed to remove ${entityName} from storage`, {
      id,
      operation: "remove",
      storageType: adapter.constructor.name,
      error: getErrorMessage(error),
    });
  }
}

/**
 * Load an item from storage.
 * Deserializes the stored JSON back into the expected type.
 */
export async function loadItem<T, TMetadata>(
  id: string,
  adapter: BaseStorageAdapter<TMetadata, void> | null | undefined,
  entityName: string,
): Promise<T | null> {
  if (!adapter) {
    return null;
  }

  try {
    const data = await adapter.load(id);
    if (!data) {
      return null;
    }

    const decoder = new TextDecoder();
    const json = decoder.decode(data);
    return JSON.parse(json) as T;
  } catch (error) {
    logger.error(`Failed to load ${entityName} from storage`, {
      id,
      operation: "load",
      storageType: adapter.constructor.name,
      error: getErrorMessage(error),
    });
    return null;
  }
}

/**
 * Initialize an items collection from storage.
 * Loads all persisted items into the provided collection.
 */
export async function initializeFromStorage<T, TMetadata>(
  adapter: BaseStorageAdapter<TMetadata, void> | null,
  items: { set: (key: string, value: T) => void; has: (key: string) => boolean; size: number },
  info: StorageEntityInfo<T, TMetadata>,
): Promise<void> {
  if (!adapter) {
    logger.debug(
      `No storage adapter configured, skipping ${info.entityName} initialization from storage`,
    );
    return;
  }

  try {
    const ids = await adapter.list();

    logger.info(`Initializing ${info.entityName}s from storage`, {
      count: ids.length,
      storageType: adapter.constructor.name,
    });

    for (const id of ids) {
      try {
        const item = await loadItem<T, TMetadata>(id, adapter, info.entityName);
        if (item) {
          items.set(id, item);
        }
      } catch (error) {
        logger.error(`Failed to load ${info.entityName} from storage`, {
          id,
          error: getErrorMessage(error),
        });
      }
    }

    logger.info(`${capitalize(info.entityName)} initialization complete`, {
      loadedCount: items.size,
    });
  } catch (error) {
    logger.error(`Failed to initialize ${info.entityName}s from storage`, {
      error: getErrorMessage(error),
      storageType: adapter.constructor.name,
      operation: "initializeFromStorage",
    });
  }
}

// ==================== Entity-Specific Storage Functions ====================

// ==================== Agent Profile ====================

export async function persistAgentProfile(
  profile: AgentProfileMeta,
  adapter?: AgentProfileStorageAdapter | null,
): Promise<void> {
  return persistItem(profile, adapter, agentProfileMetadataBuilder);
}

export async function removeAgentProfile(
  profileId: string,
  adapter?: AgentProfileStorageAdapter | null,
): Promise<void> {
  return removeItem(profileId, adapter, "agent profile");
}

export async function loadAgentProfile(
  profileId: string,
  adapter?: AgentProfileStorageAdapter | null,
): Promise<AgentProfileMeta | null> {
  return loadItem<AgentProfileMeta, AgentProfileStorageMetadata>(
    profileId,
    adapter,
    "agent profile",
  );
}

export async function initializeAgentProfilesFromStorage(
  adapter: AgentProfileStorageAdapter | null,
  profiles: {
    set: (key: string, value: AgentProfileMeta) => void;
    has: (key: string) => boolean;
    size: number;
  },
): Promise<void> {
  return initializeFromStorage(adapter, profiles, agentProfileMetadataBuilder);
}

// ==================== Hook Template ====================

export async function persistHookTemplate(
  template: HookTemplate,
  adapter?: HookTemplateStorageAdapter | null,
): Promise<void> {
  return persistItem(template, adapter, hookTemplateMetadataBuilder);
}

export async function removeHookTemplate(
  name: string,
  adapter?: HookTemplateStorageAdapter | null,
): Promise<void> {
  return removeItem(name, adapter, "hook template");
}

export async function loadHookTemplate(
  name: string,
  adapter?: HookTemplateStorageAdapter | null,
): Promise<HookTemplate | null> {
  return loadItem<HookTemplate, HookTemplateStorageMetadata>(name, adapter, "hook template");
}

export async function initializeHookTemplatesFromStorage(
  adapter: HookTemplateStorageAdapter | null,
  templates: {
    set: (key: string, value: HookTemplate) => void;
    has: (key: string) => boolean;
    size: number;
  },
): Promise<void> {
  return initializeFromStorage(adapter, templates, hookTemplateMetadataBuilder);
}

// ==================== Node Template ====================

export async function persistNodeTemplate(
  template: NodeTemplate,
  adapter?: NodeTemplateStorageAdapter | null,
): Promise<void> {
  return persistItem(template, adapter, nodeTemplateMetadataBuilder);
}

export async function removeNodeTemplate(
  name: string,
  adapter?: NodeTemplateStorageAdapter | null,
): Promise<void> {
  return removeItem(name, adapter, "node template");
}

export async function loadNodeTemplate(
  name: string,
  adapter?: NodeTemplateStorageAdapter | null,
): Promise<NodeTemplate | null> {
  return loadItem<NodeTemplate, NodeTemplateStorageMetadata>(name, adapter, "node template");
}

export async function initializeNodeTemplatesFromStorage(
  adapter: NodeTemplateStorageAdapter | null,
  templates: {
    set: (key: string, value: NodeTemplate) => void;
    has: (key: string) => boolean;
    size: number;
  },
): Promise<void> {
  return initializeFromStorage(adapter, templates, nodeTemplateMetadataBuilder);
}

// ==================== Script ====================

export async function persistScript(
  script: Script,
  adapter?: ScriptStorageAdapter | null,
): Promise<void> {
  return persistItem(script, adapter, scriptMetadataBuilder);
}

export async function removeScript(
  scriptName: string,
  adapter?: ScriptStorageAdapter | null,
): Promise<void> {
  return removeItem(scriptName, adapter, "script");
}

export async function loadScript(
  scriptName: string,
  adapter?: ScriptStorageAdapter | null,
): Promise<Script | null> {
  return loadItem<Script, ScriptStorageMetadata>(scriptName, adapter, "script");
}

export async function initializeScriptsFromStorage(
  adapter: ScriptStorageAdapter | null,
  scripts: { set: (key: string, value: Script) => void; has: (key: string) => boolean; size: number },
): Promise<void> {
  return initializeFromStorage(adapter, scripts, scriptMetadataBuilder);
}

// ==================== Tool ====================

export async function persistTool(
  tool: Tool,
  adapter?: ToolStorageAdapter | null,
): Promise<void> {
  return persistItem(tool, adapter, toolMetadataBuilder);
}

export async function removeTool(
  toolId: string,
  adapter?: ToolStorageAdapter | null,
): Promise<void> {
  return removeItem(toolId, adapter, "tool");
}

export async function loadTool(
  toolId: string,
  adapter?: ToolStorageAdapter | null,
): Promise<Tool | null> {
  return loadItem<Tool, ToolStorageMetadata>(toolId, adapter, "tool");
}

export async function initializeToolsFromStorage(
  adapter: ToolStorageAdapter | null,
  tools: { set: (key: string, value: Tool) => void; has: (key: string) => boolean; size: number },
): Promise<void> {
  return initializeFromStorage(adapter, tools, toolMetadataBuilder);
}

// ==================== Trigger Template ====================

export async function persistTrigger(
  template: TriggerTemplate,
  adapter?: TriggerStorageAdapter | null,
): Promise<void> {
  return persistItem(template, adapter, triggerTemplateMetadataBuilder);
}

export async function removeTrigger(
  name: string,
  adapter?: TriggerStorageAdapter | null,
): Promise<void> {
  return removeItem(name, adapter, "trigger template");
}

export async function loadTrigger(
  name: string,
  adapter?: TriggerStorageAdapter | null,
): Promise<TriggerTemplate | null> {
  return loadItem<TriggerTemplate, TriggerStorageMetadata>(name, adapter, "trigger template");
}

export async function initializeTriggersFromStorage(
  adapter: TriggerStorageAdapter | null,
  templates: {
    set: (key: string, value: TriggerTemplate) => void;
    has: (key: string) => boolean;
    size: number;
  },
): Promise<void> {
  return initializeFromStorage(adapter, templates, triggerTemplateMetadataBuilder);
}

function capitalize(s: string): string {
  return s.charAt(0).toUpperCase() + s.slice(1);
}
