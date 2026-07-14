/**
 * Workflow Checkpoint Configuration Type Definition
 */

import type { ID } from "../../common.js";
import type {
  CheckpointMetadata,
  DeltaStorageConfig,
  CheckpointConfigSource,
  CheckpointTriggerType,
} from "../base.js";
import { DEFAULT_DELTA_STORAGE_CONFIG } from "../base.js";

// Reexport the generic types
export type { CheckpointMetadata, DeltaStorageConfig };
export { DEFAULT_DELTA_STORAGE_CONFIG };

/**
 * Checkpoint configuration context
 */
export interface CheckpointConfigContext {
  /** Triggering timing (using unified CheckpointTrigger enum) */
  triggerType: CheckpointTriggerType;
  /** Node ID (optional) */
  nodeId?: string;
  /** Tool ID (optional) */
  toolId?: ID;
  /** Whether it is a triggered sub-workflow */
  isTriggeredSubworkflow?: boolean;
  /** Whether to explicitly enable checkpoints */
  explicitEnableCheckpoint?: boolean;
}

/**
 * Workflow Checkpoint Configuration Level
 */
export interface WorkflowCheckpointConfigLayer {
  /** Configure the source */
  source: CheckpointConfigSource;
  /** Configuration content */
  config: CheckpointConfigContent;
}

/**
 * Checkpoint configuration content
 */
export interface CheckpointConfigContent {
  /** Whether to enable checkpoints */
  enabled?: boolean;
  /** Checkpoint Description */
  description?: string;
  /** Enable configuration for specific trigger moments */
  triggers?: {
    nodeBeforeExecute?: boolean;
    nodeAfterExecute?: boolean;
    toolBefore?: boolean;
    toolAfter?: boolean;
  };
}

/**
 * Workflow Checkpoint Content Configuration
 *
 * Fine-grained control over what data is included in workflow checkpoint snapshots.
 * This allows reducing snapshot size by excluding large optional fields.
 *
 * @since 2.0.0
 */
export interface WorkflowCheckpointContentConfig {
  /**
   * Include conversation state (messages, markMap, tokenUsage)
   * This is the largest field in a typical checkpoint.
   * @default true
   */
  includeConversation?: boolean;

  /**
   * Maximum number of messages to include in conversation state.
   * When set, only the most recent N messages are included.
   * Set to 0 to exclude messages entirely (requires includeConversation=true).
   * @default undefined (no limit, include all messages)
   */
  maxMessages?: number;

  /**
   * Include variable state (all variables in flat map).
   * Variables can be large for complex workflows.
   * @default true
   */
  includeVariables?: boolean;

  /**
   * Include node execution results.
   * Node results accumulate over time and can be large for long-running workflows.
   * @default true
   */
  includeNodeResults?: boolean;

  /**
   * Maximum number of node results to include.
   * When set, only the most recent N node results are included.
   * @default undefined (no limit, include all node results)
   */
  nodeResultLimit?: number;

  /**
   * Maximum snapshot size in bytes.
   * When the estimated snapshot size exceeds this limit, large fields are
   * automatically truncated to fit within the budget.
   * @default undefined (no size limit)
   */
  maxSnapshotSize?: number;

  /**
   * Maximum number of error records to include in snapshot.
   * @default 100
   */
  maxErrorRecords?: number;

  /**
   * Maximum number of interruption records to include in snapshot.
   * @default 50
   */
  maxInterruptionRecords?: number;

  /**
   * Maximum number of event records to include in snapshot.
   * @default 100
   */
  maxEventRecords?: number;

  // ========== P1: Incremental Message Storage ==========

  /**
   * Enable incremental message storage for FULL checkpoints.
   * When enabled, FULL checkpoints only store messages that are new since
   * the previous FULL checkpoint, instead of duplicating the entire message history.
   * A `messageBaseCheckpointId` reference is stored to enable reconstruction.
   *
   * @default false (store all messages in FULL checkpoints)
   */
  incrementalMessages?: boolean;

  // ========== P2: Async Serialization ==========

  /**
   * Enable async (non-blocking) checkpoint creation.
   * When enabled, the checkpoint is created in the background and the
   * checkpoint ID is returned immediately. Use `waitForPersistence()`
   * to ensure the checkpoint is fully persisted before continuing.
   *
   * @default false (blocking, synchronous)
   */
  async?: boolean;
}
