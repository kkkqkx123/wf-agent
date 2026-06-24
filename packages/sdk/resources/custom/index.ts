/**
 * Custom Resources Module
 *
 * Exports types and loaders for custom resources that can be loaded from configuration files.
 * Custom resources are user-defined extensions to SDK predefined resources.
 */

export type {
  CustomToolDefinition,
  CustomTriggerDefinition,
  CustomPromptDefinition,
  CustomResources,
  CustomResourcesPresetConfig,
} from "./types.js";

export {
  loadCustomTools,
  loadCustomTriggers,
  loadCustomPrompts,
  loadCustomResourcesFromConfig,
} from "./loader.js";

export {
  registerCustomTools,
  registerCustomTriggers,
  registerCustomPrompts,
  registerCustomResources,
} from "./registration.js";
