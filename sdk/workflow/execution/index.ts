/**
 * Execution Module Export
 * Provides the Thread execution engine, node processing functions, router, and event manager.
 */

// Execute entity
export { WorkflowExecutionEntity } from "../entities/index.js";
export { ExecutionState, type SubgraphContext } from "../state-managers/index.js";

// Main Execution Engine
export { ThreadExecutor, type ThreadExecutorDependencies } from "./executors/thread-executor.js";

// Thread Builder
export { ThreadBuilder } from "./factories/thread-builder.js";

// Thread Status Verification Tool Function
export {
  validateTransition,
  isValidTransition,
  getAllowedTransitions,
  isTerminalStatus,
  isActiveStatus,
} from "./utils/thread-state-validator.js";

// Thread Registry
export { WorkflowExecutionRegistry } from "../stores/thread-registry.js";

// Variable Coordinator and State Manager
export { VariableCoordinator } from "./coordinators/variable-coordinator.js";
export { VariableState } from "../state-managers/variable-state.js";

// LLM Execution-related - Re-exporting from the General Execution Core
export {
  ConversationSession,
  type ConversationState,
  type ConversationSessionConfig,
} from "../../core/messaging/conversation-session.js";
export {
  TokenUsageTracker,
  type TokenUsageTrackerOptions,
  type FullTokenUsageStats,
} from "../../core/utils/token/token-usage-tracker.js";
export type { TokenUsageStats } from "@wf-agent/types";

// Hook handling function
export * from "./handlers/hook-handlers/index.js";

// Hook Creator Tool
export * from "./utils/hook-creators.js";

// Node processing function
export * from "./handlers/node-handlers/index.js";

// Trigger handling function
export * from "./handlers/trigger-handlers/index.js";

// Execution handlers (stateless)
export * from "./handlers/index.js";
