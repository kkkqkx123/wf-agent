/**
 * Agent Module Main Entry
 *
 * Responsibilities:
 * - Provides a unified module export interface
 * - Manages the export of all components at the Agent layer
 *
 * Architectural Hierarchy:
 * - entities/     Entity layer: Pure data entities, encapsulating execution status
 * - managers/     Manager layer: Manages message history and variable state
 * - execution/    Execution layer: Factory and lifecycle management
 * - coordinators/    Coordinator layer: Manages the lifecycle
 * - executors/    Executor layer: Core execution logic
 * - checkpoint/   Checkpoint layer: Creation and restoration of incremental snapshots
 * - services/     Service layer: Registry, etc.
 */

// Entity Layer
export {
  AgentLoopEntity,
  AgentLoopState,
  AgentLoopStatus,
  type ToolCallRecord,
  type IterationRecord,
} from "./entities/index.js";
// `AgentLoopStateSnapshot` is exported from the `types` package.
export { type AgentLoopStateSnapshot } from "@wf-agent/types";

// Manager Layer
export { MessageHistory, type MessageHistoryState } from "./message/index.js";
export { VariableState, type VariableStateSnapshot } from "./variable/index.js";

// Execution Layer (Factories and Lifecycle)
export {
  AgentLoopFactory,
  createAgentLoopCheckpoint,
  cleanupAgentLoop,
  cloneAgentLoop,
  type AgentLoopEntityOptions,
  type AgentLoopCheckpointDependencies,
  type AgentLoopCheckpointOptions,
} from "./execution/index.js";

// Coordinator Layer
export {
  AgentLoopCoordinator,
  ConversationCoordinator,
  type AgentLoopExecuteOptions,
} from "./execution/coordinators/index.js";

// Executor Layer
export { AgentLoopExecutor } from "./execution/executors/index.js";

// Checkpoint layer
export {
  AgentLoopDiffCalculator,
  AgentLoopDeltaRestorer,
  AgentLoopCheckpointResolver,
  AgentLoopCheckpointCoordinator,
  createCheckpoint,
  restoreFromCheckpoint,
  type CheckpointDependencies,
  type CheckpointOptions,
  type CreateCheckpointOptions,
} from "./checkpoint/index.js";

// Stores layer (export class for direct instantiation, also available via DI)
export { AgentLoopRegistry } from "./stores/agent-loop-registry.js";

// Error Handler
export {
  handleAgentError,
  handleAgentInterruption as handleAgentInterruptionError,
  isRecoverableAgentError,
  createAgentExecutionError,
} from "./execution/handlers/agent-error-handler.js";
