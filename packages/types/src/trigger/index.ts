/**
 * Trigger Type Definition Unified Export
 * Define trigger type and structure for implementing event-based trigger mechanism
 *
 * design principles:
 * - Trigger is dedicated to event listening
 * - No time triggers or state triggers
 * - Use type aliases and interfaces to maintain simplicity.
 * - Easy serialization and deserialization
 */

// Exporting Trigger Definitions
export * from "./definition.js";

// Export Status Type
export * from "./state.js";

// Export Configuration Type
export * from "./config.js";

// Export execution-related types
export * from "./execution.js";

// Export Zod Schemas for Trigger Validation
export {
  TriggerConditionSchema,
  TriggerActionSchema,
  WorkflowTriggerSchema,
  TriggerReferenceSchema,
  TriggerTemplateSchema,
  TriggerConfigOverrideSchema,
  ConversationHistoryOptionsSchema,
  ExecuteTriggeredSubgraphActionConfigSchema,
  ExecuteScriptActionConfigSchema,
  StopThreadActionParametersSchema,
  PauseThreadActionParametersSchema,
  ResumeThreadActionParametersSchema,
  SkipNodeActionParametersSchema,
  SetVariableActionParametersSchema,
  SendNotificationActionParametersSchema,
  CustomActionParametersSchema,
  ApplyMessageOperationActionParametersSchema,
  isTriggerCondition,
  isTriggerAction,
  isWorkflowTrigger,
  isTriggerReference,
  isTriggerTemplate,
  isTriggerConfigOverride,
} from "./trigger-schema.js";
