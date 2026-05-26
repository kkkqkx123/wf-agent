/**
 * Unified export of Workflow type definitions
 * Define the complete structure of the workflow, including nodes and edges
 */

// Export Edge Types
export * from "./edge.js";
export * from "./edge-schema.js";

// Export Node Template Types
export * from "./node-template.js";
export * from "./node-template-schema.js";

// Exporting workflow definitions
export * from "./definition.js";

// Export Configuration Type
export * from "./config.js";

// Export Relationship Types

// Export Zod Schemas
export * from "./config-schema.js";

// Export Boundary Data Passing Configuration
export * from "./boundary-config.js";

// Export Boundary Configuration Zod Schemas
export {
  WorkflowVariableInputSchema,
  WorkflowVariableOutputSchema,
  WorkflowMessageInputSchema,
  WorkflowMessageOutputSchema,
  WorkflowDataInputSchema,
  WorkflowDataOutputSchema,
  WorkflowStartConfigSchema,
  WorkflowEndConfigSchema,
  isWorkflowVariableInput,
  isWorkflowVariableOutput,
  isWorkflowMessageInput,
  isWorkflowMessageOutput,
  isWorkflowDataInput,
  isWorkflowDataOutput,
  isWorkflowStartConfig,
  isWorkflowEndConfig,
} from "./boundary-config-schema.js";

// Tool Configuration (Static)
export type { AvailableTools } from "./tool-config.js";
export {
  validateAvailableTools,
  resolveSchemaTools,
  resolveInitialTools,
  requiresApproval,
  isToolAvailable,
} from "./tool-config.js";
