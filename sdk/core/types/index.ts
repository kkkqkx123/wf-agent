/**
 * Core Types - Unified Export
 *
 * SDK core internal type definitions.
 * These types are implementation details of the SDK core module.
 *
 * Note: For cross-package types, use @wf-agent/types instead.
 */

// Execution types - Task and Execution Instance Definitions
export {
  TaskStatus,
  type ExecutionInstance,
  type ExecutionInstanceType,
  type InstanceRef,
  type TaskInfo,
  type StoredTaskInfo,
  isAgentInstance,
  isWorkflowExecutionInstance,
  getExecutionInstanceType,
  getExecutionInstanceId,
  hasLoadedInstance,
  isStoredTaskInfo,
} from "./execution.js";

// Pool types - Worker and Execution Pool Definitions
export {
  WorkerStatus,
  type QueueStats,
  type PoolStats,
  type ExecutorWrapper,
  type ExecutionPoolConfig,
} from "./pool.js";

// State Manager Interfaces
export { type StateManager, type StateManagerMetadata } from "./state-manager.js";

// Abortable Interface
export { type Abortable } from "./abortable.js";

// Timeout Management Types - Internal SDK timeout system
export * from "./timeout.js";
export * from "./timeout-config.js";
export * from "./timeout-tags.js";
