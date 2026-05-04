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

// Lifecycle types - State Manager Interfaces
export { type LifecycleCapable } from "./lifecycle-capable.js";
export { type StateManager, type StateManagerMetadata } from "./state-manager.js";
