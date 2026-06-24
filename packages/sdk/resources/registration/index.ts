/**
 * Unified Resources Registration Module
 *
 * This module exports the orchestrator and types for registering resources
 * from all three pipelines (predefined, custom, application) in a coordinated manner.
 */

export type {
  PredefinedRegistrationResult,
  CustomResourcesRegistrationResult,
  ApplicationResourcesRegistrationResult,
  UnifiedRegistrationResult,
} from "./types.js";

export { registerAllResources } from "./orchestrator.js";
