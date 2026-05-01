/**
 * Checkpoint common basic type definitions
 * For use by both the agent and graph modules
 */

import type { ID } from "../common.js";

/**
 * Checkpoint type
 */
export type CheckpointType = "FULL" | "DELTA";

/**
 * Checkpoint type enumeration values
 */
export const CheckpointTypeEnum: Record<string, CheckpointType> = {
  /** Complete checkpoint */
  FULL: "FULL",
  /** Incremental checkpoint */
  DELTA: "DELTA",
};

/**
 * Checkpoint metadata type
 */
export interface CheckpointMetadata {
  /** Checkpoint description */
  description?: string;
  /** Tag array */
  tags?: string[];
  /** Custom field object */
  customFields?: Record<string, unknown>;
}

/**
 * Incremental Storage Strategy Configuration (General)
 */
export interface DeltaStorageConfig {
  /** Whether to enable incremental storage */
  enabled: boolean;
  /** Baseline checkpoint interval (a baseline is created every N checkpoints) */
  baselineInterval: number;
  /** Maximum increment chain length (a new baseline is created if exceeded) */
  maxDeltaChainLength: number;
}

/**
 * Default incremental storage configuration
 */
export const DEFAULT_DELTA_STORAGE_CONFIG: DeltaStorageConfig = {
  enabled: true,
  baselineInterval: 10,
  maxDeltaChainLength: 20,
};

/**
 * Checkpoint configuration source
 * Indicates the location where the configuration definition is located
 */
export type CheckpointConfigSource =
  | "runtime" // Passed in at runtime
  | "workflow" // Workflow Definition
  | "node" // Node Definition
  | "agent" // Agent Loop Configuration
  | "global" // Global Configuration
  | "default"; // Default value

/**
 * Graph checkpoint trigger timing
 */
export type GraphCheckpointTriggerType =
  | "NODE_BEFORE_EXECUTE"
  | "NODE_AFTER_EXECUTE"
  | "TOOL_BEFORE"
  | "TOOL_AFTER"
  | "HOOK"
  | "TRIGGER";

/**
 * Agent Loop Checkpoint Trigger Timing
 */
export type AgentLoopCheckpointTriggerType = "ITERATION_END" | "ERROR";

/**
 * Checkpoint configuration results (general)
 */
export interface CheckpointConfigResult {
  /** Should a checkpoint be created? */
  shouldCreate: boolean;
  /** Checkpoint description */
  description?: string;
  /** The actual source of the effective configuration */
  effectiveSource: CheckpointConfigSource;
  /** Triggering timing */
  triggerType?: GraphCheckpointTriggerType | AgentLoopCheckpointTriggerType;
}

/**
 * General Checkpoint List Options
 */
export interface CheckpointListOptions {
  /** Parent ID (executionId or agentLoopId) */
  parentId?: ID;
  /** Tag filtering */
  tags?: string[];
  /** Limit the quantity */
  limit?: number;
  /** Offset */
  offset?: number;
}

/**
 * Base Checkpoint Core Interface (Strict, no index signature)
 * Defines the minimal common fields for all checkpoint types
 */
export interface BaseCheckpointCore<TDelta, TSnapshot> {
  /** Checkpoint unique identifier */
  id: ID;
  /** Checkpoint type */
  type?: CheckpointType;
  /** Baseline checkpoint ID (required for delta checkpoints) */
  baseCheckpointId?: ID;
  /** Previous checkpoint ID (required for delta checkpoints) */
  previousCheckpointId?: ID;
  /** Delta data (for delta checkpoints) */
  delta?: TDelta;
  /** Full snapshot (for full checkpoints) */
  snapshot?: TSnapshot;
  /** Creation timestamp */
  timestamp?: number;
}

/**
 * Base Checkpoint Interface (Extensible)
 * Extends core interface with common metadata
 */
export interface BaseCheckpoint<TDelta, TSnapshot>
  extends BaseCheckpointCore<TDelta, TSnapshot> {
  /** Checkpoint metadata */
  metadata?: CheckpointMetadata;
}

/**
 * Full Checkpoint Interface
 * Represents a complete state snapshot
 */
export interface FullCheckpoint<TSnapshot> extends BaseCheckpoint<never, TSnapshot> {
  type: "FULL";
  snapshot: TSnapshot;
}

/**
 * Delta Checkpoint Interface
 * Represents incremental changes from a base checkpoint
 */
export interface DeltaCheckpoint<TDelta> extends BaseCheckpoint<TDelta, never> {
  type: "DELTA";
  baseCheckpointId: ID;
  previousCheckpointId: ID;
  delta: TDelta;
}

/**
 * Union type for any checkpoint
 */
export type AnyCheckpoint<TDelta, TSnapshot> = 
  | FullCheckpoint<TSnapshot>
  | DeltaCheckpoint<TDelta>;
