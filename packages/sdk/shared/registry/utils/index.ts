/**
 * Registry Utilities
 *
 * Consolidated utilities for registry operations.
 *
 * Organized into:
 * - storage/: Entity persistence operations
 * - validation/: Entity validation and template validators
 * - registry-internals.ts: In-memory registry factory
 * - hierarchy-traversal-service.ts: Tree traversal and hierarchy operations
 */

// Core registry factory
export { createRegistry, RegistryImpl, PersistentRegistryImpl } from "./registry-internals.js";
export type { Registry, MutableRegistry } from "./registry-internals.js";

// Hierarchy operations
export { HierarchyTraversalService } from "./hierarchy-traversal-service.js";

// Storage utilities
export type { StorageEntityInfo } from "./storage/index.js";
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
} from "./storage/index.js";

// Validation utilities
export type {
  RequiredFieldRule,
  ValidationResult,
  EntityValidationSchema,
} from "./validation/index.js";

export {
  validateRequiredFields,
  validateRequiredString,
  validateIdentifier,
  validateBoolean,
  validatePositiveNumber,
  validateEnum,
  validateAtLeastOne,
  validateMetadata,
  combineValidationResults,
  isRegistryValidationError,
  validateEntityBySchema,
  validateEntityOrThrow,
  PROMPT_TEMPLATE_SCHEMA,
  FRAGMENT_SCHEMA,
  HOOK_TEMPLATE_SCHEMA,
  NODE_TEMPLATE_SCHEMA,
  TRIGGER_TEMPLATE_SCHEMA,
  validatePromptTemplate,
  validateFragment,
  validateHookTemplate,
  validateNodeTemplate,
  validateTriggerTemplate,
  TRIGGER_TEMPLATE_EVENT_TYPES,
  TRIGGER_TEMPLATE_ACTION_TYPES,
  validateTriggerTemplateRegistry,
} from "./validation/index.js";
