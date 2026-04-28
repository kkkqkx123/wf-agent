/**
 * Predefined Tool Entry
 *
 * Functions for exporting the definition and registration of predefined tools
 */

// Export tool definition creates a function
export { createPredefinedTools, createAllPredefinedTools } from "./registry.js";

// Export type definitions
export type {
  PredefinedToolsOptions,
  ToolCategory,
  PredefinedToolDefinition,
  ReadFileConfig,
  WriteFileConfig,
  EditFileConfig,
  RunShellConfig,
  SessionNoteConfig,
  BackendShellConfig,
} from "./types.js";

// Export registration-related functions
export {
  registerPredefinedTools,
  unregisterPredefinedTools,
  isPredefinedToolRegistered,
} from "./registration.js";

// Export all tool definitions (both stateless and stateful).
export * from "./stateless/index.js";
export * from "./stateful/index.js";

// Export builtin tools
export * from "./builtin/index.js";

// Export tool descriptions and registration function
export {
  // All descriptions array
  ALL_PREDEFINED_TOOL_DESCRIPTIONS,
  // Registration functions
  registerAllPredefinedToolDescriptions,
  arePredefinedToolDescriptionsRegistered,
  initializeToolDescriptions,
  // Individual descriptions are also exported from the module
} from "./tool-descriptions.js";
