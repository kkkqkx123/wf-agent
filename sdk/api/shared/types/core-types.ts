/**
 * Core API Type Definitions
 * Defines types related to core execution
 */

import type { WorkflowExecutionOptions, PresetsConfig } from "@wf-agent/types";
import type {
  CheckpointStorageAdapter,
  WorkflowStorageAdapter,
  WorkflowExecutionStorageAdapter,
  TaskStorageAdapter,
  AgentLoopCheckpointStorageAdapter,
} from "@wf-agent/storage";

/**
 * SDK Options
 */
export interface SDKOptions {
  /** Whether to enable debug mode */
  debug?: boolean;
  /** Log Level */
  logLevel?: "debug" | "info" | "warn" | "error";
  /** Default timeout period (in milliseconds) */
  defaultTimeout?: number;
  /** Whether to enable checkpoints */
  enableCheckpoints?: boolean;
  /** Checkpoint storage adapter interface (implemented by the application layer) */
  checkpointStorageAdapter?: CheckpointStorageAdapter;
  /** Workflow storage adapter interface (implemented by the application layer) */
  workflowStorageAdapter?: WorkflowStorageAdapter;
  /** Task storage adapter interface (implemented by the application layer) */
  taskStorageAdapter?: TaskStorageAdapter;
  /** Workflow execution storage adapter interface (implemented by the application layer) */
  workflowExecutionStorageAdapter?: WorkflowExecutionStorageAdapter;
  /** Agent Loop checkpoint storage adapter interface (implemented by the application layer) */
  agentLoopCheckpointStorageAdapter?: AgentLoopCheckpointStorageAdapter;
  /** Whether to enable verification */
  enableValidation?: boolean;
  /** Predefined feature options */
  presets?: PresetsConfig;
}

/**
 * SDK Dependencies
 */
export interface SDKDependencies {
  /** Workflow Registry */
  workflowRegistry?: unknown;
  /** Workflow Execution Registry */
  executionRegistry?: unknown;
  /** Tool Registry */
  toolRegistry?: unknown;
  /** Script Registry */
  scriptRegistry?: unknown;
  /** Event Manager */
  eventManager?: unknown;
  /** Checkpoint storage adapter interface (implemented by the application layer) */
  checkpointStorageAdapter?: CheckpointStorageAdapter;
}

export type { WorkflowExecutionOptions };
