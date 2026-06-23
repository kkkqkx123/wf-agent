/**
 * Storage Index
 *
 * Re-export all storage functions and types for convenient access.
 */

export type { StorageEntityInfo } from "./entity-storage.js";
export {
  persistItem,
  removeItem,
  loadItem,
  initializeFromStorage,
  persistAgentProfile,
  removeAgentProfile,
  loadAgentProfile,
  initializeAgentProfilesFromStorage,
  persistHookTemplate,
  removeHookTemplate,
  loadHookTemplate,
  initializeHookTemplatesFromStorage,
  persistNodeTemplate,
  removeNodeTemplate,
  loadNodeTemplate,
  initializeNodeTemplatesFromStorage,
  persistScript,
  removeScript,
  loadScript,
  initializeScriptsFromStorage,
  persistTool,
  removeTool,
  loadTool,
  initializeToolsFromStorage,
  persistTrigger,
  removeTrigger,
  loadTrigger,
  initializeTriggersFromStorage,
} from "./entity-storage.js";
