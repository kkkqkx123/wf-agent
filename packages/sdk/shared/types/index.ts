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
  type TimeoutPolicy,
  isAgentInstance,
  isWorkflowExecutionInstance,
  getExecutionInstanceType,
  getExecutionInstanceId,
  hasLoadedInstance,
  isStoredTaskInfo,
} from "./execution.js";

// Execution Entity Interface - Unified contract for all execution entities
export { type IExecutionEntity, type ExecutionStatus, type Abortable } from "./execution-entity.js";

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

// Timeout Management Types - Internal SDK timeout system
export * from "./timeout.js";
export * from "./timeout-tags.js";

// Task Snapshot Types - Serializable task data for persistence
export { type TaskSnapshot, TaskSerializationUtils } from "./task-snapshot.js";
