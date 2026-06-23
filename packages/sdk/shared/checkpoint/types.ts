/**
 * Common interfaces and types for universal checkpoint system
 */

import type { BaseCheckpoint, DeltaStorageConfig } from "@wf-agent/types";

/**
 * Storage adapter interface for checkpoint persistence
 *
 * Re-exported from @wf-agent/storage to maintain a single unified interface
 * across the codebase. This eliminates the dual-type issue where SDK core
 * and packages/storage defined separate CheckpointStorageAdapter interfaces.
 */
export type { CheckpointStorageAdapter } from "@wf-agent/storage";

/**
 * Minimal entity interface for checkpointing
 * Only requires an ID - state extraction is handled by coordinator
 */
export interface CheckpointableEntity {
  id: string;
}

/**
 * Checkpoint dependencies interface
 * Generic enough to work with any checkpoint type
 */
export interface CheckpointDependencies<TCheckpoint extends BaseCheckpoint<unknown, unknown>> {
  saveCheckpoint: (checkpoint: TCheckpoint) => Promise<string>;
  getCheckpoint: (id: string) => Promise<TCheckpoint | null>;
  listCheckpoints: (parentId: string) => Promise<string[]>;
  deltaConfig?: DeltaStorageConfig;
}

/**
 * Delta restoration result
 */
export interface DeltaRestoreResult<TState> {
  snapshot: TState;
  metadata: {
    checkpointChain: string[];
    baseCheckpointId: string;
  };
}

/**
 * Unified checkpoint restoration result
 *
 * Used to standardize the return value from checkpoint restore operations.
 * Provides a consistent interface for both Agent and Workflow checkpoint restoration.
 *
 * @template TEntity The type of the restored entity (AgentLoopEntity or WorkflowExecutionEntity)
 * @template TState The type of the state snapshot
 * @template TExtra Additional type-specific data (optional, defaults to unknown)
 */
export interface RestoreResult<TEntity, TState = unknown, TExtra = unknown> {
  /** The restored entity */
  entity: TEntity;
  /** The restored state snapshot */
  snapshot: TState;
  /** The checkpoint ID that was restored */
  checkpointId: string;
  /** Optional additional restoration metadata */
  metadata?: {
    checkpointChain?: string[];
    baseCheckpointId?: string;
    version?: string;
  };
  /** Type-specific extra data (e.g., ConversationSession, StateCoordinator) */
  extra?: TExtra;
}

