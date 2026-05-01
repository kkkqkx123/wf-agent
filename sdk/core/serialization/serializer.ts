/**
 * Generic Serializer
 *
 * Provides serialization and deserialization functionality for snapshots.
 * Supports version control, migration, and optional compression.
 */

import type {
  SnapshotBase,
  SerializationOptions,
  DeserializationOptions,
  SerializedError,
} from "@wf-agent/types";
import { compressBlob, decompressBlob } from "@wf-agent/storage";

/**
 * Default serialization options
 */
const DEFAULT_SERIALIZATION_OPTIONS: Required<SerializationOptions> = {
  prettyPrint: false,
  compression: true, // Enable compression by default for better storage efficiency
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
  protected readonly compression: boolean;

  constructor(options?: SerializationOptions) {
    const opts = { ...DEFAULT_SERIALIZATION_OPTIONS, ...options };
    this.currentVersion = opts.targetVersion;
    this.prettyPrint = opts.prettyPrint;
    this.compression = opts.compression;
  }

  /**
   * Serialize a snapshot to Uint8Array
   *
   * @param snapshot The snapshot to serialize
   * @returns Serialized data as byte array (compressed if enabled)
   */
  async serialize(snapshot: TSnapshot): Promise<Uint8Array> {
    try {
      const data = {
        ...snapshot,
        _version: this.currentVersion,
        _timestamp: snapshot._timestamp ?? Date.now(),
      };

      const json = this.prettyPrint ? JSON.stringify(data, null, 2) : JSON.stringify(data);
      const bytes = new TextEncoder().encode(json);

      // Apply compression if enabled
      if (this.compression) {
        const result = await compressBlob(bytes, {
          enabled: true,
          algorithm: "gzip",
          threshold: 512, // Compress data larger than 512 bytes
        });
        return result.compressed;
      }

      return bytes;
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : String(error);
      throw new Error(
        `Failed to serialize ${snapshot._entityType}: ${errorMessage}`,
        { cause: error },
      );
    }
  }

  /**
   * Deserialize data to a snapshot
   *
   * @param data The serialized data (may be compressed)
   * @param options Deserialization options
   * @returns The deserialized snapshot
   */
  async deserialize(data: Uint8Array, options?: DeserializationOptions): Promise<TSnapshot> {
    try {
      let bytes = data;

      // Try to detect and decompress if needed
      // Check if data appears to be compressed (gzip magic number: 0x1f 0x8b)
      if (data.length > 2 && data[0] === 0x1f && data[1] === 0x8b) {
        try {
          bytes = await decompressBlob(data, "gzip");
        } catch (error) {
          // If decompression fails, assume data is not compressed
          console.warn("Failed to decompress data, treating as uncompressed:", error);
        }
      }

      const json = new TextDecoder().decode(bytes);
      const snapshot = JSON.parse(json) as TSnapshot;

      return await this.migrateIfNeeded(snapshot, options?.targetVersion);
    } catch (error) {
      // Don't wrap migration errors as they already have context
      if (error instanceof Error && error.message.includes("Failed to migrate")) {
        throw error;
      }
      
      const errorMessage = error instanceof Error ? error.message : String(error);
      throw new Error(
        `Failed to deserialize data: ${errorMessage}`,
        { cause: error },
      );
    }
  }

  /**
   * Migrate snapshot if needed
   *
   * @param snapshot The snapshot to potentially migrate
   * @param targetVersion The target version (defaults to current version)
   * @returns The migrated snapshot
   */
  protected async migrateIfNeeded(snapshot: TSnapshot, targetVersion?: number): Promise<TSnapshot> {
    const target = targetVersion ?? this.currentVersion;

    if (snapshot._version < target) {
      // Simple version bump migration
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
