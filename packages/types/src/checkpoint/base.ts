/**
 * Checkpoint common basic type definitions
 * For use by both the agent and graph modules
 *
 * Contains: snapshot types, checkpoint core interfaces, unified triggers,
 * checkpoint policies, and decision context — all shared across execution domains.
 */

import type { ID, Timestamp } from "../common.js";
import type { ExecutionErrorRecord, ExecutionInterruptionRecord, ExecutionEventRecord } from "./execution-events.js";
import type { ExecutionHierarchyMetadata } from "../execution/hierarchy.js";

// =============================================================================
// Snapshot Types
// =============================================================================

/**
 * Snapshot version for format identification
 */
export type SnapshotVersion = number;

/**
 * Base interface for all snapshot types
 *
 * All serializable entities should produce snapshots that extend this interface.
 */
export interface SnapshotBase {
  /** Snapshot format version */
  _version: SnapshotVersion;
  /** Timestamp when the snapshot was created */
  _timestamp: Timestamp;
  /** Entity type identifier (e.g., 'task', 'checkpoint', 'execution') */
  _entityType: string;
}

// =============================================================================
// Checkpoint Type (FULL / DELTA)
// =============================================================================

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

// =============================================================================
// Checkpoint Metadata
// =============================================================================

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

// =============================================================================
// Checkpoint Options
// =============================================================================

/**
 * Checkpoint creation options
 */
export interface CheckpointOptions {
  /**
   * If true, blocks until data is persisted to disk
   * Default: false (async for performance)
   */
  sync?: boolean;

  /**
   * Timeout for synchronous checkpoint (milliseconds)
   * Only applies when sync=true
   * Default: 30000 (30 seconds)
   */
  syncTimeout?: number;
}

// =============================================================================
// Unified Checkpoint State Base
// =============================================================================

/**
 * Base interface for all checkpoint state snapshots.
 *
 * Provides common fields shared across AgentLoop and Workflow execution types.
 * Both AgentLoopStateSnapshot and WorkflowExecutionStateSnapshot extend this base.
 *
 * ## Design Principles
 *
 * 1. **Common fields only**: Only fields present in both Agent and Workflow snapshots.
 * 2. **Optional**: All fields are optional to allow minimal snapshot creation.
 * 3. **Backward compatible**: Existing snapshots without these fields are still valid.
 *
 * @since 2.0.0
 */
export interface CheckpointStateBase {
  /** Execution status */
  status?: string;
  /** Execution start timestamp (ms) */
  startTime?: number | null;
  /** Execution end timestamp (ms) */
  endTime?: number | null;
  /** Error data (if execution failed) */
  error?: unknown;

  // ========== Execution Event Tracking ==========

  /** Errors that occurred during execution (atomic with state) */
  errorRecords?: ExecutionErrorRecord[];
  /** Interruptions (pauses/stops) that occurred during execution */
  interruptionRecords?: ExecutionInterruptionRecord[];
  /** Recent execution events for timeline view */
  eventRecords?: ExecutionEventRecord[];

  // ========== Hierarchy ==========

  /** Execution hierarchy metadata (parent, children, depth, root info) */
  hierarchy?: ExecutionHierarchyMetadata;
}

// =============================================================================
// Incremental (Delta) Storage Configuration
// =============================================================================

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

// =============================================================================
// Checkpoint Configuration Source
// =============================================================================

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

// =============================================================================
// Domain-specific Trigger Types (Legacy)
// =============================================================================

/**
 * Workflow checkpoint trigger timing
 * @deprecated Use CheckpointTrigger enum instead
 */
export type WorkflowCheckpointTriggerType =
  | "NODE_BEFORE_EXECUTE"
  | "NODE_AFTER_EXECUTE"
  | "TOOL_BEFORE"
  | "TOOL_AFTER"
  | "HOOK"
  | "TRIGGER";

/**
 * Agent Loop Checkpoint Trigger Timing
 * @deprecated Use CheckpointTrigger enum instead
 */
export type AgentLoopCheckpointTriggerType =
  | "ITERATION_END"
  | "ERROR"
  | "COMPLETE"
  | "PAUSE"
  | "TOOL_CALL"
  | "TOOL_RESULT"
  | "MANUAL"
  | "INTERVAL"
  | "NEVER";

// =============================================================================
// Unified Checkpoint Trigger Enum
// =============================================================================

/**
 * Unified Checkpoint Trigger Enumeration
 *
 * Covers all possible trigger points across Workflow and Agent systems.
 * Uses consistent naming convention: UPPER_SNAKE_CASE for clarity.
 */
export enum CheckpointTrigger {
  // ============ Execution Lifecycle ============

  /**
   * Before any execution starts
   * Applicable to: Workflow nodes, Agent loops
   */
  BEFORE_EXECUTE = "BEFORE_EXECUTE",

  /**
   * After execution completes successfully
   * Applicable to: Workflow nodes, Agent iterations
   */
  AFTER_EXECUTE = "AFTER_EXECUTE",

  // ============ Error & Recovery ============

  /**
   * When an error occurs during execution
   * Applicable to: All execution contexts
   */
  ON_ERROR = "ON_ERROR",

  /**
   * When a retry is about to happen
   * Applicable to: Workflow nodes, Agent iterations, main loop
   */
  BEFORE_RETRY = "BEFORE_RETRY",

  /**
   * After a retry succeeds
   * Applicable to: Workflow nodes, Agent iterations, main loop
   */
  AFTER_RETRY_SUCCESS = "AFTER_RETRY_SUCCESS",

  /**
   * When fallback mechanism is activated
   * Applicable to: Workflow nodes (with fallbackOutput), Agent loops (with fallbackOutput)
   */
  ON_FALLBACK = "ON_FALLBACK",

  // ============ Iteration (Agent-specific) ============

  /**
   * After each Agent iteration completes
   * Applicable to: Agent loops only
   */
  ITERATION_END = "ITERATION_END",

  /**
   * When an Agent iteration fails
   * Applicable to: Agent loops only
   */
  ITERATION_FAILED = "ITERATION_FAILED",

  // ============ Tool Invocation ============

  /**
   * Before a tool is called
   * Applicable to: All execution contexts (Agent, Workflow)
   */
  TOOL_BEFORE = "TOOL_BEFORE",

  /**
   * After a tool returns (success or failure)
   * Applicable to: All execution contexts
   */
  TOOL_AFTER = "TOOL_AFTER",

  // ============ Flow Control ============

  /**
   * When execution is paused
   * Applicable to: Workflow execution, Agent loops
   */
  ON_PAUSE = "ON_PAUSE",

  /**
   * When execution is cancelled
   * Applicable to: Workflow execution, Agent loops
   */
  ON_CANCEL = "ON_CANCEL",

  /**
   * When execution completes (success or failure)
   * Applicable to: Workflow execution, Agent loops
   */
  ON_COMPLETE = "ON_COMPLETE",

  // ============ Periodic ============

  /**
   * Periodic checkpoint (every N seconds)
   * Applicable to: Long-running executions
   */
  INTERVAL = "INTERVAL",

  // ============ Manual ============

  /**
   * Manual checkpoint (explicit user request)
   * Applicable to: All execution contexts
   */
  MANUAL = "MANUAL",

  // ============ Disabled ============

  /**
   * Disable automatic checkpointing
   * Used to indicate no checkpointing should occur
   */
  NEVER = "NEVER",
}

/** Type alias for CheckpointTrigger enum values */
export type CheckpointTriggerType = CheckpointTrigger;

// =============================================================================
// Checkpoint Config Result
// =============================================================================

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
  /** Triggering timing (using unified CheckpointTriggerType) */
  triggerType?: CheckpointTriggerType;
}

// =============================================================================
// Checkpoint List Options
// =============================================================================

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

// =============================================================================
// Checkpoint Policy: Content / Retention / Error Handling
// =============================================================================

/**
 * Checkpoint Content Configuration
 *
 * Controls what data is included in each checkpoint.
 */
export interface CheckpointContentConfig {
  /**
   * Include full execution state
   * Examples: variable bindings, loop counters, LLM state
   * @default true
   */
  includeState?: boolean;

  /**
   * Include message/interaction history
   * For Agent: LLM messages, tool calls, results
   * For Workflow: node outputs, intermediate values
   * @default true
   */
  includeHistory?: boolean;

  /**
   * Include statistics (retry count, timing, etc.)
   * Adds: retry counts, delays, duration, fallback usage
   * @default false
   */
  includeStatistics?: boolean;

  /**
   * Custom metadata to attach to the checkpoint
   * Useful for tagging, filtering, or debugging
   * @default undefined
   */
  metadata?: Record<string, unknown>;
}

/**
 * Checkpoint Retention and Cleanup Configuration
 */
export interface CheckpointRetentionConfig {
  /**
   * Maximum number of checkpoints to keep
   * When exceeded, oldest checkpoints are deleted
   * @default 1000
   * @example -1 for unlimited
   */
  maxCheckpoints?: number;

  /**
   * Maximum age of checkpoints in milliseconds
   * Older checkpoints are automatically deleted
   * @default 7 days
   * @example -1 for no limit
   */
  maxAge?: number;

  /**
   * Compression strategy for checkpoint storage
   * @default 'auto' (compress large checkpoints)
   */
  compression?: 'none' | 'gzip' | 'auto';
}

/**
 * Checkpoint Error Handling Configuration
 */
export interface CheckpointErrorHandlingConfig {
  /**
   * Whether to fail execution if checkpoint creation fails
   * If false, checkpoint failures are logged but don't interrupt execution
   * @default false
   */
  failOnCheckpointError?: boolean;

  /**
   * Retry checkpoint creation on failure
   * @default true
   */
  retryOnFailure?: boolean;

  /**
   * Maximum number of checkpoint creation retries
   * @default 3
   */
  maxRetries?: number;
}

/**
 * Unified Checkpoint Policy
 *
 * Defines when, what, and how checkpoints are created and retained.
 */
export interface UnifiedCheckpointPolicy {
  /**
   * Whether automatic checkpointing is enabled
   * @default true
   */
  enabled: boolean;

  /**
   * Trigger events (can combine multiple)
   *
   * Single trigger:
   * ```typescript
   * triggers: CheckpointTrigger.ON_ERROR
   * ```
   *
   * Multiple triggers:
   * ```typescript
   * triggers: [CheckpointTrigger.ON_ERROR, CheckpointTrigger.ON_COMPLETE]
   * ```
   *
   * Disable checkpointing:
   * ```typescript
   * triggers: CheckpointTrigger.NEVER
   * ```
   */
  triggers: CheckpointTriggerType | CheckpointTriggerType[];

  /**
   * Content configuration for checkpoints
   * @default { includeState: true, includeHistory: true }
   */
  content?: CheckpointContentConfig;

  /**
   * Retention and cleanup policy
   * @default { maxCheckpoints: 1000, maxAge: 7 days }
   */
  retention?: CheckpointRetentionConfig;

  /**
   * Error handling configuration
   * @default { failOnCheckpointError: false, retryOnFailure: true, maxRetries: 3 }
   */
  errorHandling?: CheckpointErrorHandlingConfig;
}

// =============================================================================
// Base Checkpoint Interfaces
// =============================================================================

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

// =============================================================================
// Predefined Checkpoint Policies
// =============================================================================

/**
 * Minimal Checkpoint Policy
 *
 * Only creates checkpoints on error and completion.
 * Storage-efficient for low-overhead execution.
 */
export const CHECKPOINT_POLICY_MINIMAL: UnifiedCheckpointPolicy = {
  enabled: true,
  triggers: [CheckpointTrigger.ON_ERROR, CheckpointTrigger.ON_COMPLETE],
  content: {
    includeState: true,
    includeHistory: false,
  },
  retention: {
    maxCheckpoints: 100,
    maxAge: 24 * 60 * 60 * 1000, // 1 day
    compression: 'gzip',
  },
};

/**
 * Standard Checkpoint Policy
 *
 * Creates checkpoints on important lifecycle events.
 * Balanced between observability and resource usage.
 */
export const CHECKPOINT_POLICY_STANDARD: UnifiedCheckpointPolicy = {
  enabled: true,
  triggers: [
    CheckpointTrigger.BEFORE_EXECUTE,
    CheckpointTrigger.ON_ERROR,
    CheckpointTrigger.ON_COMPLETE,
  ],
  content: {
    includeState: true,
    includeHistory: true,
  },
  retention: {
    maxCheckpoints: 1000,
    maxAge: 7 * 24 * 60 * 60 * 1000, // 7 days
    compression: 'auto',
  },
};

/**
 * Comprehensive Checkpoint Policy
 *
 * Creates detailed checkpoints for every significant event.
 * Provides maximum observability for debugging and time-travel scenarios.
 */
export const CHECKPOINT_POLICY_COMPREHENSIVE: UnifiedCheckpointPolicy = {
  enabled: true,
  triggers: [
    CheckpointTrigger.BEFORE_EXECUTE,
    CheckpointTrigger.AFTER_EXECUTE,
    CheckpointTrigger.ON_ERROR,
    CheckpointTrigger.BEFORE_RETRY,
    CheckpointTrigger.AFTER_RETRY_SUCCESS,
    CheckpointTrigger.ITERATION_END,
    CheckpointTrigger.TOOL_BEFORE,
    CheckpointTrigger.TOOL_AFTER,
    CheckpointTrigger.ON_FALLBACK,
  ],
  content: {
    includeState: true,
    includeHistory: true,
    includeStatistics: true,
  },
  retention: {
    maxCheckpoints: 10000,
    maxAge: 30 * 24 * 60 * 60 * 1000, // 30 days
    compression: 'gzip',
  },
};

/**
 * No Checkpoint Policy
 *
 * Disables automatic checkpointing completely.
 * Checkpoints can still be created manually.
 */
export const CHECKPOINT_POLICY_NONE: UnifiedCheckpointPolicy = {
  enabled: false,
  triggers: CheckpointTrigger.NEVER,
};

/**
 * Map of common checkpoint policies for easy selection
 */
export const CHECKPOINT_POLICIES = {
  MINIMAL: CHECKPOINT_POLICY_MINIMAL,
  STANDARD: CHECKPOINT_POLICY_STANDARD,
  COMPREHENSIVE: CHECKPOINT_POLICY_COMPREHENSIVE,
  NONE: CHECKPOINT_POLICY_NONE,
} as const;

// =============================================================================
// Checkpoint Decision Context
// =============================================================================

/**
 * Context Information for Checkpoint Decision-Making
 *
 * Provides execution context to enable intelligent checkpoint decisions
 * based on the current execution state and history.
 */
export interface CheckpointContext {
  /**
   * Execution entity type
   * Indicates whether this is a Workflow, Agent loop, or Node
   */
  entityType: 'workflow' | 'agent-loop' | 'node';

  /**
   * Entity identifier (executionId, agentLoopId, or nodeId)
   */
  entityId: string;

  /**
   * Current attempt/iteration number (1-based)
   */
  attempt?: number;

  /**
   * Current retry count at the relevant level
   * For workflows: node-level retries
   * For agent loops: iteration-level and/or main-loop retries
   */
  retryCount?: number;

  /**
   * Error information (if execution failed)
   * Present for ON_ERROR and ITERATION_FAILED triggers
   */
  error?: unknown;

  /**
   * Whether fallback mechanism is being used
   * Present for ON_FALLBACK trigger
   */
  fallbackUsed?: boolean;

  /**
   * Whether this is a main-loop retry (Agent-specific)
   * Indicates retry at the entire loop level vs iteration level
   */
  isMainLoopRetry?: boolean;

  /**
   * Execution timing information
   */
  timing?: {
    /** Execution start timestamp */
    startTime?: number;
    /** Current time */
    currentTime?: number;
    /** Execution duration in milliseconds */
    duration?: number;
  };

  /**
   * Custom context metadata for extensibility
   */
  metadata?: Record<string, unknown>;
}
