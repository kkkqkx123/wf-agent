/**
 * State Codec
 *
 * Handles serialization and deserialization of state snapshots.
 * Supports optional compression for storage efficiency.
 */

import {
  compressBlob,
  decompressBlob,
  selectCompressionStrategy,
  type CompressionConfig,
} from "../utils/compression/index.js";
import type { SerializedError } from "@wf-agent/types";

/**
 * Compression strategy mode
 */
export type CompressionStrategy = "simple" | "adaptive";

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
  /**
   * Compression strategy:
   * - "simple": Always uses gzip (legacy behavior)
   * - "adaptive": Uses selectCompressionStrategy for content-aware algorithm selection (brotli for large JSON, gzip otherwise)
   * @default "adaptive"
   */
  compressionStrategy?: CompressionStrategy;
  /**
   * Optional custom compression config override.
   * When set, takes precedence over adaptive selection.
   */
  compressionConfig?: CompressionConfig;
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
const DEFAULT_OPTIONS: SerializationOptions & Required<Pick<SerializationOptions, 'prettyPrint' | 'compression' | 'compressionThreshold' | 'compressionStrategy'>> = {
  prettyPrint: false,
  compression: true,
  compressionThreshold: 512,
  compressionStrategy: "adaptive",
};

/**
 * State Codec
 *
 * Handles serialization and deserialization of state snapshots.
 * Supports optional compression for storage efficiency.
 */
export class StateCodec {
  private readonly options: SerializationOptions & Required<Pick<SerializationOptions, 'prettyPrint' | 'compression' | 'compressionThreshold' | 'compressionStrategy'>>;

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

      // Skip compression if disabled
      if (!this.options.compression) {
        return bytes;
      }

      // Skip compression if data is too small
      if (bytes.length <= this.options.compressionThreshold) {
        return bytes;
      }

      // Determine compression config
      let config: CompressionConfig;
      if (this.options.compressionConfig) {
        config = this.options.compressionConfig;
      } else if (this.options.compressionStrategy === "adaptive") {
        config = selectCompressionStrategy(bytes);
      } else {
        config = {
          enabled: true,
          algorithm: "gzip",
          threshold: 0,
        };
      }

      const result = await compressBlob(bytes, config);
      return result.compressed;
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : String(error);
      throw new Error(`Failed to serialize data: ${errorMessage}`, { cause: error });
    }
  }

  /**
   * Detect compression algorithm from magic bytes
   */
  private detectAlgorithm(data: Uint8Array): string | null {
    if (data.length < 2) return null;

    // Gzip magic number: 0x1f 0x8b
    if (data[0] === 0x1f && data[1] === 0x8b) {
      return "gzip";
    }

    // Brotli magic bytes: 0xce 0xb2 0xcf 0x81
    if (data.length >= 4 && data[0] === 0xce && data[1] === 0xb2 && data[2] === 0xcf && data[3] === 0x81) {
      return "brotli";
    }

    return null;
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

      // Detect and decompress if needed (supports gzip and brotli)
      const algorithm = this.detectAlgorithm(data);
      if (algorithm) {
        try {
          bytes = await decompressBlob(data, algorithm);
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
