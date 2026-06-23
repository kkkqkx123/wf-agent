/**
 * Execution Module Export
 * Provides the Workflow execution engine, node processing functions, router, and event manager.
 */

// Execute entity
export { WorkflowExecutionEntity } from "../entities/index.js";
export { ExecutionState, type SubgraphContext } from "../state-managers/index.js";

// Main Execution Engine
export {
  WorkflowExecutor,
  type WorkflowExecutorDependencies,
} from "./executors/workflow-executor.js";
// Workflow Execution Builder
export { WorkflowExecutionBuilder } from "./factories/workflow-execution-builder.js";
// Workflow State Validation
export {
  validateTransition,
  isValidTransition,
  getAllowedTransitions,
  isTerminalStatus,
  isActiveStatus,
} from "./utils/workflow-state-validator.js";

// Workflow Execution Registry
export { WorkflowExecutionRegistry } from "../stores/workflow-execution-registry.js";

// Variable Coordinator and State Manager
export { VariableCoordinator } from "./coordinators/variable-coordinator.js";
// Unified VariableManager
export {
  VariableManager,
  type VariableManagerSnapshot,
} from "../state-managers/variable-manager.js";

// LLM Execution-related - Re-exporting from the General Execution Core
export {
  ConversationSession,
  type ConversationState,
  type ConversationSessionConfig,
} from "../../shared/messaging/conversation-session.js";
export {
  TokenUsageTracker,
  type TokenUsageTrackerOptions,
  type FullTokenUsageStats,
} from "../../shared/utils/token/token-usage-tracker.js";
export type { TokenUsageStats } from "@wf-agent/types";

// Hook handling function
export * from "./handlers/hook-handlers/index.js";

// Node processing function
export * from "./handlers/node-handlers/index.js";

// Trigger handling function
export * from "./handlers/trigger-handlers/index.js";

// Execution handlers (stateless)
export * from "./handlers/index.js";
