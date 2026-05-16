/**
 * Predefined Trigger Entries
 *
 * Export the definitions and registration functions for predefined triggers.
 */

// Export trigger definition creation functions
export { createPredefinedTriggers } from "./registry.js";

// Export type definitions
export type { PredefinedTriggersOptions, TriggerCategory, PredefinedTriggerMetadata } from "./types.js";

// Export registration-related functions
export {
  registerPredefinedTriggers,
  unregisterPredefinedTriggers,
  isPredefinedTriggerRegistered,
} from "./registration.js";
