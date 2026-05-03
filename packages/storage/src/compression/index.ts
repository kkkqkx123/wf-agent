/**
 * Compression Module
 * Provides compression and decompression utilities for storage backends
 *
 * Supports multiple compression algorithms (gzip, brotli) with configurable thresholds
 * Used by both SQLite and JSON storage implementations
 */

export {
  compressBlob,
  decompressBlob,
  compressBlobSync,
  decompressBlobSync,
  type CompressionConfig,
  type CompressionResult,
  DEFAULT_COMPRESSION_CONFIG,
} from "./compressor.js";

export {
  CompressionService,
  type GlobalCompressionConfig,
  type EntityType,
  DEFAULT_GLOBAL_COMPRESSION_CONFIG,
} from "./compression-service.js";

export {
  detectDataType,
  selectCompressionStrategy,
  logCompressionDecision,
  type DataType,
} from "./adaptive-compression.js";
