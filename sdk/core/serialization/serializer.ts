/**
 * Generic Serializer
 *
 * Provides serialization and deserialization functionality for snapshots.
 * Supports version control and migration.
 */

import type {
  SnapshotBase,
  SerializationOptions,
  DeserializationOptions,
  SerializedError,
} from "@wf-agent/types";

/**
 * Default serialization options
 */
const DEFAULT_SERIALIZATION_OPTIONS: Required<SerializationOptions> = {
  prettyPrint: false,
  compression: false,
  targetVersion: 1,
};

/**
 * Generic Serializer class
 *
 * @template TSnapshot The snapshot type
 */
export class Serializer<TSnapshot extends SnapshotBase> {
  protected readonly currentVersion: number;
  protected readonly prettyPrint: boolean;

  constructor(options?: SerializationOptions) {
    const opts = { ...DEFAULT_SERIALIZATION_OPTIONS, ...options };
    this.currentVersion = opts.targetVersion;
    this.prettyPrint = opts.prettyPrint;
  }

  /**
   * Serialize a snapshot to Uint8Array
   *
   * @param snapshot The snapshot to serialize
   * @returns Serialized data as byte array
   */
  serialize(snapshot: TSnapshot): Uint8Array {
    const data = {
      ...snapshot,
      _version: this.currentVersion,
      _timestamp: snapshot._timestamp ?? Date.now(),
    };

    const json = this.prettyPrint ? JSON.stringify(data, null, 2) : JSON.stringify(data);
    return new TextEncoder().encode(json);
  }

  /**
   * Deserialize data to a snapshot
   *
   * @param data The serialized data
   * @param options Deserialization options
   * @returns The deserialized snapshot
   */
  deserialize(data: Uint8Array, options?: DeserializationOptions): TSnapshot {
    const json = new TextDecoder().decode(data);
    const snapshot = JSON.parse(json) as TSnapshot;

    return this.migrateIfNeeded(snapshot, options?.targetVersion);
  }

  /**
   * Migrate snapshot if needed
   *
   * @param snapshot The snapshot to potentially migrate
   * @param targetVersion The target version (defaults to current version)
   * @returns The migrated snapshot
   */
  protected migrateIfNeeded(snapshot: TSnapshot, targetVersion?: number): TSnapshot {
    const target = targetVersion ?? this.currentVersion;

    if (snapshot._version < target) {
      return this.performMigration(snapshot, target);
    }

    return snapshot;
  }

  /**
   * Perform migration from one version to another
   *
   * Override this method in subclasses to implement custom migration logic.
   *
   * @param snapshot The snapshot to migrate
   * @param targetVersion The target version
   * @returns The migrated snapshot
   */
  protected performMigration(snapshot: TSnapshot, targetVersion: number): TSnapshot {
    return {
      ...snapshot,
      _version: targetVersion,
    };
  }
}

/**
 * Error serializer utilities
 */
export class ErrorSerializer {
  /**
   * Convert Error to serializable format
   */
  static serialize(error: Error): SerializedError {
    return {
      message: error.message,
      name: error.name,
      stack: error.stack,
      cause: (error as Error & { cause?: unknown }).cause,
    };
  }

  /**
   * Convert serialized error back to Error
   */
  static deserialize(serialized: SerializedError): Error {
    const error = new Error(serialized.message, { cause: serialized.cause });
    error.name = serialized.name;
    error.stack = serialized.stack;
    return error;
  }
}
