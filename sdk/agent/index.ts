/**
 * Agent Module Main Entry
 *
 * Responsibilities:
 * - Provides a unified module export interface
 * - Manages the export of all components at the Agent layer
 *
 * Architectural Hierarchy:
 * - entities/         Entity layer: Pure data entities, encapsulating execution status
 * - state-managers/   State Manager layer: Centralized state management (AgentLoopState, MessageHistory)
 * - execution/        Execution layer: Factory and lifecycle management
 * - coordinators/     Coordinator layer: Manages the lifecycle
 * - executors/        Executor layer: Core execution logic
 * - checkpoint/       Checkpoint layer: Creation and restoration of incremental snapshots
 * - stores/           Service layer: Registry, etc.
 */

// Entity Layer
export {
  AgentLoopEntity,
  AgentLoopStatus,
  type ToolCallRecord,
  type IterationRecord,
} from "./entities/index.js";
// `AgentLoopStateSnapshot` is exported from the `types` package.
export { type AgentLoopStateSnapshot } from "@wf-agent/types";

// State Manager Layer (NEW)
export {
  AgentLoopState,
  MessageHistory,
  type MessageHistoryState,
} from "./state-managers/index.js";

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
  AgentExecutionCoordinator,
  AgentLoopStateTransitor,
  ToolExecutionCoordinator,
  type AgentLoopExecuteOptions,
  type AgentExecutionCoordinatorDependencies,
  type AgentLoopStreamEvent,
  type ToolExecutionCoordinatorDependencies,
} from "./execution/coordinators/index.js";

// Executor Layer
export { AgentLoopExecutor } from "./execution/executors/index.js";

// Checkpoint layer
export {
  AgentLoopCheckpointConfigResolver,
  AgentLoopCheckpointCoordinator,
  AgentLoopCheckpointStateManager,
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

// Agent-specific Types
export { AgentExecutionInterruptedException } from "./execution/types/index.js";
