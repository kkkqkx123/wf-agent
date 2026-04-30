/**
 * BLOB Compression Utilities
 * Provides compression and decompression for SQLite BLOB storage
 *
 * Uses Node.js zlib for compression with configurable algorithms (gzip, brotli)
 */

import { promisify } from "util";
import {
  gzip,
  gunzip,
  gzipSync,
  gunzipSync,
  brotliCompress,
  brotliDecompress,
  brotliCompressSync,
  brotliDecompressSync,
  constants as zlibConstants,
} from "zlib";

const gzipAsync = promisify(gzip);
const gunzipAsync = promisify(gunzip);
const brotliCompressAsync = promisify(brotliCompress);
const brotliDecompressAsync = promisify(brotliDecompress);

/**
 * Compression configuration
 */
export interface CompressionConfig {
  /** Whether to enable compression */
  enabled: boolean;
  /** Compression algorithm */
  algorithm?: "gzip" | "brotli";
  /** Compression threshold (bytes) - data smaller than this won't be compressed */
  threshold?: number;
  /** Compression level (1-9, where 9 is maximum compression) - deprecated, use algorithm */
  level?: number;
  /** Minimum data size to compress - deprecated, use threshold */
  minSize?: number;
}

/**
 * Default compression configuration
 * Uses gzip algorithm with threshold of 1KB
 */
export const DEFAULT_COMPRESSION_CONFIG: CompressionConfig = {
  enabled: true,
  algorithm: "gzip",
  threshold: 1024,
};

/**
 * Compression result
 */
export interface CompressionResult {
  /** Compressed data */
  compressed: Uint8Array;
  /** Compression algorithm used, or null if not compressed */
  algorithm: string | null;
  /** Original data size */
  originalSize: number;
  /** Compression ratio (compressed / original) */
  ratio: number;
}

/**
 * Compress BLOB data
 * @param data Raw data to compress
 * @param config Compression configuration
 * @returns Compression result
 */
export async function compressBlob(
  data: Uint8Array,
  config: CompressionConfig = DEFAULT_COMPRESSION_CONFIG,
): Promise<CompressionResult> {
  const originalSize = data.length;
  const threshold = config.threshold ?? config.minSize ?? 1024;
  const level = config.level ?? 6;
  const algorithm = config.algorithm ?? "gzip";

  // Skip compression if disabled or data is too small
  if (!config.enabled || originalSize < threshold) {
    return {
      compressed: data,
      algorithm: null,
      originalSize,
      ratio: 1,
    };
  }

  try {
    let compressed: Buffer;

    switch (algorithm) {
      case "gzip":
        compressed = await gzipAsync(Buffer.from(data), { level });
        break;
      case "brotli":
        // Brotli quality ranges from 0-11, map level 1-9 to quality 4-8
        const quality = Math.max(0, Math.min(11, Math.round((level / 9) * 8)));
        compressed = await brotliCompressAsync(Buffer.from(data), {
          params: {
            [zlibConstants.BROTLI_PARAM_QUALITY]: quality,
          },
        });
        break;
      default:
        throw new Error(`Unsupported compression algorithm: ${algorithm}`);
    }

    const compressedArray = new Uint8Array(compressed);
    const ratio = compressedArray.length / originalSize;

    // Only use compressed data if it actually reduces size
    if (compressedArray.length < originalSize) {
      return {
        compressed: compressedArray,
        algorithm,
        originalSize,
        ratio,
      };
    }

    // Compression didn't help, return original
    return {
      compressed: data,
      algorithm: null,
      originalSize,
      ratio: 1,
    };
  } catch (error) {
    // If compression fails, return original data
    process.stderr.write(`Compression failed, returning original data: ${error}\n`);
    return {
      compressed: data,
      algorithm: null,
      originalSize,
      ratio: 1,
    };
  }
}

/**
 * Create a compression error with cause
 */
function createCompressionError(message: string, cause: unknown): Error {
  return new Error(message, { cause: cause instanceof Error ? cause : undefined });
}

/**
 * Decompress BLOB data
 * @param data Compressed data
 * @param algorithm Compression algorithm used
 * @returns Decompressed data
 */
export async function decompressBlob(
  data: Uint8Array,
  algorithm: string | null,
): Promise<Uint8Array> {
  // If no algorithm specified, data is not compressed
  if (!algorithm) {
    return data;
  }

  try {
    switch (algorithm) {
      case "gzip": {
        const decompressed = await gunzipAsync(Buffer.from(data));
        return new Uint8Array(decompressed);
      }
      case "brotli": {
        const decompressed = await brotliDecompressAsync(Buffer.from(data));
        return new Uint8Array(decompressed);
      }
      default:
        throw new Error(`Unsupported compression algorithm: ${algorithm}`);
    }
  } catch (error) {
    throw createCompressionError(`Decompression failed: ${(error as Error).message}`, error);
  }
}

/**
 * Synchronous compression for use in transactions
 * Note: This blocks the event loop, use with caution
 */
export function compressBlobSync(
  data: Uint8Array,
  config: CompressionConfig = DEFAULT_COMPRESSION_CONFIG,
): CompressionResult {
  const originalSize = data.length;
  const threshold = config.threshold ?? config.minSize ?? 1024;
  const level = config.level ?? 6;
  const algorithm = config.algorithm ?? "gzip";

  // Skip compression if disabled or data is too small
  if (!config.enabled || originalSize < threshold) {
    return {
      compressed: data,
      algorithm: null,
      originalSize,
      ratio: 1,
    };
  }

  try {
    let compressed: Buffer;

    switch (algorithm) {
      case "gzip":
        compressed = gzipSync(Buffer.from(data), { level });
        break;
      case "brotli":
        // Brotli quality ranges from 0-11, map level 1-9 to quality 4-8
        const quality = Math.max(0, Math.min(11, Math.round((level / 9) * 8)));
        compressed = brotliCompressSync(Buffer.from(data), {
          params: {
            [zlibConstants.BROTLI_PARAM_QUALITY]: quality,
          },
        });
        break;
      default:
        throw new Error(`Unsupported compression algorithm: ${algorithm}`);
    }

    const compressedArray = new Uint8Array(compressed);
    const ratio = compressedArray.length / originalSize;

    // Only use compressed data if it actually reduces size
    if (compressedArray.length < originalSize) {
      return {
        compressed: compressedArray,
        algorithm,
        originalSize,
        ratio,
      };
    }

    return {
      compressed: data,
      algorithm: null,
      originalSize,
      ratio: 1,
    };
  } catch (error) {
    process.stderr.write(`Sync compression failed, returning original data: ${error}\n`);
    return {
      compressed: data,
      algorithm: null,
      originalSize,
      ratio: 1,
    };
  }
}

/**
 * Synchronous decompression for use in transactions
 * Note: This blocks the event loop, use with caution
 */
export function decompressBlobSync(data: Uint8Array, algorithm: string | null): Uint8Array {
  if (!algorithm) {
    return data;
  }

  try {
    switch (algorithm) {
      case "gzip": {
        const decompressed = gunzipSync(Buffer.from(data));
        return new Uint8Array(decompressed);
      }
      case "brotli": {
        const decompressed = brotliDecompressSync(Buffer.from(data));
        return new Uint8Array(decompressed);
      }
      default:
        throw new Error(`Unsupported compression algorithm: ${algorithm}`);
    }
  } catch (error) {
    throw createCompressionError(`Sync decompression failed: ${(error as Error).message}`, error);
  }
}
