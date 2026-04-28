/**
 * Serialization Registry
 *
 * Central registry for all entity serializers.
 * Provides a unified interface for serialization operations across entity types.
 */

import type { SnapshotBase, SerializationOptions, DeserializationOptions } from "@wf-agent/types";
import { Serializer } from "./serializer.js";
import { DeltaCalculator } from "./delta-calculator.js";
import { DeltaRestorer, SnapshotLoader, SnapshotLister } from "./delta-restorer.js";

/**
 * Serializer entry containing all serialization components for an entity type
 */
export interface SerializerEntry {
  /** Entity type identifier */
  entityType: string;
  /** Serializer instance */
  serializer: Serializer<SnapshotBase>;
  /** Delta calculator instance */
  deltaCalculator: DeltaCalculator<SnapshotBase>;
}

/**
 * Serialization Registry
 *
 * Manages serializers for different entity types and provides
 * a unified interface for serialization operations.
 */
export class SerializationRegistry {
  private static instance: SerializationRegistry;
  private entries: Map<string, SerializerEntry> = new Map();

  private constructor() {}

  /**
   * Get the singleton instance
   */
  static getInstance(): SerializationRegistry {
    if (!SerializationRegistry.instance) {
      SerializationRegistry.instance = new SerializationRegistry();
    }
    return SerializationRegistry.instance;
  }

  /**
   * Register a serializer entry for an entity type
   */
  register(entry: SerializerEntry): void {
    this.entries.set(entry.entityType, entry);
  }

  /**
   * Unregister a serializer for an entity type
   */
  unregister(entityType: string): boolean {
    return this.entries.delete(entityType);
  }

  /**
   * Check if a serializer is registered for an entity type
   */
  has(entityType: string): boolean {
    return this.entries.has(entityType);
  }

  /**
   * Get the serializer for an entity type
   */
  getSerializer(entityType: string): Serializer<SnapshotBase> | null {
    const entry = this.entries.get(entityType);
    return entry?.serializer ?? null;
  }

  /**
   * Get the delta calculator for an entity type
   */
  getDeltaCalculator(entityType: string): DeltaCalculator<SnapshotBase> | null {
    const entry = this.entries.get(entityType);
    return entry?.deltaCalculator ?? null;
  }

  /**
   * Serialize a snapshot
   *
   * @param snapshot The snapshot to serialize
   * @param options Serialization options
   * @returns Serialized data
   */
  serialize<TSnapshot extends SnapshotBase>(
    snapshot: TSnapshot,
    options?: SerializationOptions,
  ): Uint8Array {
    const serializer = this.getSerializer(snapshot._entityType);

    if (serializer) {
      return serializer.serialize(snapshot);
    }

    const defaultSerializer = new Serializer<TSnapshot>(options);
    return defaultSerializer.serialize(snapshot);
  }

  /**
   * Deserialize data to a snapshot
   *
   * @param entityType The entity type
   * @param data The serialized data
   * @param options Deserialization options
   * @returns The deserialized snapshot
   */
  deserialize<TSnapshot extends SnapshotBase>(
    entityType: string,
    data: Uint8Array,
    options?: DeserializationOptions,
  ): TSnapshot {
    const serializer = this.getSerializer(entityType);

    if (serializer) {
      return serializer.deserialize(data, options) as TSnapshot;
    }

    const defaultSerializer = new Serializer<TSnapshot>();
    return defaultSerializer.deserialize(data, options);
  }

  /**
   * Create a delta restorer for an entity type
   *
   * @param entityType The entity type
   * @param loadSnapshot Function to load a snapshot by ID
   * @param listSnapshots Function to list snapshot IDs
   */
  createRestorer<TSnapshot extends SnapshotBase>(
    _entityType: string,
    loadSnapshot: SnapshotLoader<TSnapshot>,
    listSnapshots: SnapshotLister,
  ): DeltaRestorer<TSnapshot> {
    return new DeltaRestorer(loadSnapshot, listSnapshots);
  }

  /**
   * Get all registered entity types
   */
  getEntityTypes(): string[] {
    return Array.from(this.entries.keys());
  }

  /**
   * Clear all registered serializers
   */
  clear(): void {
    this.entries.clear();
  }
}
