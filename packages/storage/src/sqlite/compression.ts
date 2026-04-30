/**
 * BLOB Compression Utilities (DEPRECATED)
 * This file is kept for backward compatibility.
 * Please use '../compression/index.js' instead.
 *
 * @deprecated Use packages/storage/src/compression/index.ts
 */

export {
  compressBlob,
  decompressBlob,
  compressBlobSync,
  decompressBlobSync,
  type CompressionConfig,
  type CompressionResult,
  DEFAULT_COMPRESSION_CONFIG,
} from "../compression/index.js";
