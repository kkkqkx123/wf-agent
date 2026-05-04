/**
 * Serialization Base Types
 *
 * Provides foundational types for the serialization system.
 * All snapshot types should extend SnapshotBase.
 */

import type { ID, Timestamp } from "../common.js";
import type { SerializedError } from "../errors/serialized-error.js";

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

/**
 * Snapshot metadata for indexing and querying
 */
export interface SnapshotMetadata {
  /** Entity ID */
  entityId: ID;
  /** Entity type */
  entityType: string;
  /** Creation timestamp */
  createdAt: Timestamp;
  /** Optional tags for categorization */
  tags?: string[];
  /** Optional custom fields */
  customFields?: Record<string, unknown>;
}

/**
 * Delta type enumeration
 */
export type DeltaType = "FULL" | "DELTA";

/**
 * Delta result wrapper
 *
 * Used to represent either a full snapshot or a delta from a base snapshot.
 */
export interface DeltaResult<TSnapshot extends SnapshotBase> {
  /** Type of result */
  type: DeltaType;
  /** Full snapshot (when type is FULL) */
  snapshot?: TSnapshot;
  /** Delta data (when type is DELTA) */
  delta?: Partial<TSnapshot>;
  /** Base snapshot ID (when type is DELTA) */
  baseSnapshotId?: ID;
}

/**
 * Serialization options
 */
export interface SerializationOptions {
  /** Pretty print JSON output */
  prettyPrint?: boolean;
  /** Enable compression */
  compression?: boolean;
  /** Target version for serialization */
  targetVersion?: SnapshotVersion;
}

/**
 * Deserialization options
 */
export interface DeserializationOptions {
  /** Strict mode - throw on unknown fields */
  strict?: boolean;
  /** Target version for deserialization */
  targetVersion?: SnapshotVersion;
}
