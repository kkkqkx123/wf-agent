/**
 * Predefined Workflow Inlets
 *
 * Export functions for defining and registering predefined workflows
 */

// Export workflow definition creation functions
export { createPredefinedWorkflows } from "./registry.js";

// Export type definitions
export type { PredefinedWorkflowsOptions, WorkflowCategory, PredefinedWorkflowMetadata } from "./types.js";

// Export registration-related functions
export {
  registerPredefinedWorkflows,
  unregisterPredefinedWorkflows,
  isPredefinedWorkflowRegistered,
} from "./registration.js";

// Export individual workflow definitions
export * from "./llm-summary.js";
