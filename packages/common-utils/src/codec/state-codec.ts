/**
 * State Codec
 *
 * Handles serialization and deserialization of state snapshots.
 * Supports optional compression for storage efficiency.
 */

import { compressBlob, decompressBlob } from "../utils/compression/index.js";
import type { SerializedError } from "@wf-agent/types";

/**
 * Serialization options
 */
export interface SerializationOptions {
  /** Whether to pretty-print JSON (for debugging) */
  prettyPrint?: boolean;
  /** Whether to enable compression */
  compression?: boolean;
  /** Compression threshold in bytes (only compress if data is larger) */
  compressionThreshold?: number;
}

/**
 * Deserialization options
 */
export interface DeserializationOptions {
  /** Target version for migration (if needed) */
  targetVersion?: number;
}

/**
 * Default serialization options
 */
const DEFAULT_OPTIONS: Required<SerializationOptions> = {
  prettyPrint: false,
  compression: true,
  compressionThreshold: 512,
};

/**
 * State Codec
 *
 * Handles serialization and deserialization of state snapshots.
 * Supports optional compression for storage efficiency.
 */
export class StateCodec {
  private readonly options: Required<SerializationOptions>;

  constructor(options?: SerializationOptions) {
    this.options = { ...DEFAULT_OPTIONS, ...options };
  }

  /**
   * Serialize data to Uint8Array
   *
   * @param data The data to serialize
   * @returns Serialized byte array (compressed if enabled and data is large enough)
   */
  async serialize(data: unknown): Promise<Uint8Array> {
    try {
      const json = this.options.prettyPrint
        ? JSON.stringify(data, null, 2)
        : JSON.stringify(data);

      const bytes = new TextEncoder().encode(json);

      // Apply compression if enabled and data exceeds threshold
      if (this.options.compression && bytes.length > this.options.compressionThreshold) {
        const result = await compressBlob(bytes, {
          enabled: true,
          algorithm: "gzip",
          threshold: 0, // Already checked threshold above
        });
        return result.compressed;
      }

      return bytes;
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : String(error);
      throw new Error(`Failed to serialize data: ${errorMessage}`, { cause: error });
    }
  }

  /**
   * Deserialize data from Uint8Array
   *
   * @param data The serialized data (may be compressed)
   * @param options Deserialization options
   * @returns The deserialized data
   */
  async deserialize<T = unknown>(data: Uint8Array, _options?: DeserializationOptions): Promise<T> {
    try {
      let bytes = data;

      // Try to detect and decompress if needed (gzip magic number: 0x1f 0x8b)
      if (data.length > 2 && data[0] === 0x1f && data[1] === 0x8b) {
        try {
          bytes = await decompressBlob(data, "gzip");
        } catch {
          // If decompression fails, assume data is not compressed
        }
      }

      const json = new TextDecoder().decode(bytes);
      return JSON.parse(json) as T;
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : String(error);
      throw new Error(`Failed to deserialize data: ${errorMessage}`, { cause: error });
    }
  }
}

/**
 * Error Codec utilities
 */
export class ErrorCodec {
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
