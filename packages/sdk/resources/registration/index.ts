/**
 * Resources Registration Module
 *
 * This module exports the orchestrator and types for registering resources
 * from all three pipelines (predefined, custom, application) in a coordinated manner.
 *
 * This is the single entry point for all resource registration.
 * Direct registration from submodules is deprecated.
 */

export type {
  PredefinedRegistrationResult,
  CustomResourcesRegistrationResult,
  ApplicationResourcesRegistrationResult,
  RegistrationResult,
  ResourceRegistrationResult,
  ResourceRegistries,
} from "./types.js";

export { registerAllResources } from "./orchestrator.js";
export {
  registerAllPredefinedPrompts,
  registerPredefinedFragments,
  registerPredefinedPromptTemplates,
  areFragmentsRegistered,
  arePromptTemplatesRegistered,
} from "./prompts-registration.js";
export {
  registerAllPredefinedToolDescriptions,
  arePredefinedToolDescriptionsRegistered,
} from "./tool-descriptions-registration.js";
