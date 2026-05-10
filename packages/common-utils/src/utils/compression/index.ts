/**
 * Compression Module
 * Provides compression and decompression utilities for storage backends
 *
 * Supports multiple compression algorithms (gzip, brotli) with heuristic-based strategy selection.
 * Data characteristics (type and size) automatically determine optimal compression approach.
 * Used by both SQLite and JSON storage implementations.
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
  detectDataType,
  selectCompressionStrategy,
  logCompressionDecision,
  type DataType,
} from "./adaptive-compression.js";
