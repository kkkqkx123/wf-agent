/**
 * Serializable Interface
 *
 * Defines the capability to convert an entity to a snapshot format.
 */

import type { SnapshotBase, SerializationOptions } from "@wf-agent/types";

/**
 * Serializable interface
 *
 * Entities that can produce a snapshot should implement this interface.
 */
export interface Serializable<TSnapshot extends SnapshotBase> {
  /**
   * Convert the entity to a snapshot
   *
   * @param options Optional serialization options
   * @returns A snapshot representing the entity's state
   */
  toSnapshot(options?: SerializationOptions): TSnapshot;
}

/**
 * Deserializable interface
 *
 * Entities that can be restored from a snapshot should implement this interface.
 */
export interface Deserializable<TSnapshot extends SnapshotBase> {
  /**
   * Restore the entity from a snapshot
   *
   * @param snapshot The snapshot to restore from
   */
  fromSnapshot(snapshot: TSnapshot): void;
}

/**
 * SnapshotCapable interface
 *
 * Combines Serializable and Deserializable for entities that support both operations.
 */
export interface SnapshotCapable<TSnapshot extends SnapshotBase>
  extends Serializable<TSnapshot>, Deserializable<TSnapshot> {}

/**
 * Snapshot provider interface
 *
 * Used when an entity delegates snapshot creation to another component.
 */
export interface SnapshotProvider<TSnapshot extends SnapshotBase> {
  /**
   * Get the current snapshot
   */
  getSnapshot(): TSnapshot;
}

/**
 * Snapshot consumer interface
 *
 * Used when an entity delegates snapshot restoration to another component.
 */
export interface SnapshotConsumer<TSnapshot extends SnapshotBase> {
  /**
   * Apply a snapshot
   */
  applySnapshot(snapshot: TSnapshot): void;
}
