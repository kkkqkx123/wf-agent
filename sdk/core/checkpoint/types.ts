/**
 * Common interfaces and types for universal checkpoint system
 */

import type { BaseCheckpoint } from "@wf-agent/types";

/**
 * Storage adapter interface for checkpoint persistence
 * 
 * Note: This adapter works with serialized data (Uint8Array).
 * Serialization/deserialization is handled externally by StateCodec.
 * The adapter is agnostic to the specific checkpoint type.
 */
export interface CheckpointStorageAdapter<TMetadata = unknown> {
  save(id: string, data: Uint8Array, metadata: TMetadata): Promise<void>;
  load(id: string): Promise<Uint8Array | null>;
  delete(id: string): Promise<void>;
  list(options?: { parentId?: string; limit?: number }): Promise<string[]>;
  initialize?(): Promise<void>;
  close?(): Promise<void>;
}

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
  deltaConfig?: import("@wf-agent/types").DeltaStorageConfig;
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
