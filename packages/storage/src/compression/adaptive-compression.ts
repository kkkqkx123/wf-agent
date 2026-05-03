/**
 * Adaptive Compression Utilities
 * Provides intelligent compression strategy selection based on data characteristics
 */

import type { CompressionConfig } from "./compressor.js";

/**
 * Data type classification for compression optimization
 */
export type DataType = "json" | "binary" | "text" | "unknown";

/**
 * Detect the type of data to optimize compression strategy
 * @param data The data to analyze
 * @returns Detected data type
 */
export function detectDataType(data: Uint8Array): DataType {
  // Check if data is empty
  if (data.length === 0) {
    return "unknown";
  }

  // Try to detect JSON by checking first non-whitespace character
  const sampleSize = Math.min(data.length, 100);
  const sample = data.slice(0, sampleSize);
  
  // Find first non-whitespace byte
  let firstNonWhitespace = -1;
  for (let i = 0; i < sample.length; i++) {
    const char = sample[i];
    // Skip whitespace (space, tab, newline, carriage return)
    if (char !== 32 && char !== 9 && char !== 10 && char !== 13) {
      firstNonWhitespace = i;
      break;
    }
  }

  if (firstNonWhitespace !== -1) {
    const firstChar = sample[firstNonWhitespace];
    // JSON typically starts with { (123) or [ (91)
    if (firstChar === 123 || firstChar === 91) {
      return "json";
    }
  }

  // Check if data is mostly printable ASCII (text)
  let printableCount = 0;
  const checkSize = Math.min(data.length, 1000);
  for (let i = 0; i < checkSize; i++) {
    const byte = data[i];
    if (byte !== undefined) {
      // Printable ASCII range: 32-126, plus common whitespace
      if ((byte >= 32 && byte <= 126) || byte === 9 || byte === 10 || byte === 13) {
        printableCount++;
      }
    }
  }

  const printableRatio = printableCount / checkSize;
  if (printableRatio > 0.9) {
    return "text";
  }

  // Otherwise, treat as binary
  return "binary";
}

/**
 * Select optimal compression strategy based on data characteristics
 * @param data The data to compress
 * @param entityType Optional entity type for context (e.g., "checkpoint", "task")
 * @returns Compression configuration
 */
export function selectCompressionStrategy(
  data: Uint8Array,
  entityType?: string,
): CompressionConfig {
  const dataType = detectDataType(data);
  const size = data.length;

  // For small data (< 100 bytes), skip compression overhead
  if (size < 100) {
    return { enabled: false };
  }

  // Strategy selection based on data type and size
  switch (dataType) {
    case "json":
      // JSON compresses very well, use brotli for better ratios
      if (size > 10 * 1024) {
        // Large JSON: use brotli with higher quality
        return {
          enabled: true,
          algorithm: "brotli",
          threshold: 0, // Always compress
          level: 8,
        };
      } else {
        // Small JSON: use gzip for speed
        return {
          enabled: true,
          algorithm: "gzip",
          threshold: 0,
          level: 6,
        };
      }

    case "text":
      // Text data compresses well
      if (size > 50 * 1024) {
        // Large text: brotli
        return {
          enabled: true,
          algorithm: "brotli",
          threshold: 0,
          level: 7,
        };
      } else {
        // Small text: gzip
        return {
          enabled: true,
          algorithm: "gzip",
          threshold: 0,
          level: 6,
        };
      }

    case "binary":
      // Binary data may not compress well, be more conservative
      if (size > 100 * 1024) {
        // Large binary: try gzip with higher threshold
        return {
          enabled: true,
          algorithm: "gzip",
          threshold: 10 * 1024, // Only compress if savings > 10KB
          level: 6,
        };
      } else {
        // Small binary: use default strategy
        return {
          enabled: true,
          algorithm: "gzip",
          threshold: 1024,
          level: 6,
        };
      }

    default:
      // Unknown: use default configuration
      return {
        enabled: true,
        algorithm: "gzip",
        threshold: 1024,
        level: 6,
      };
  }
}

/**
 * Log compression decision for debugging/monitoring
 * @param originalSize Original data size
 * @param compressedSize Compressed data size
 * @param algorithm Algorithm used
 * @param dataType Detected data type
 */
export function logCompressionDecision(
  originalSize: number,
  compressedSize: number,
  algorithm: string | null,
  dataType: DataType,
): void {
  const ratio = compressedSize / originalSize;
  const savings = ((1 - ratio) * 100).toFixed(1);
  
  if (algorithm) {
    process.stderr.write(
      `[Compression] Type: ${dataType}, Algorithm: ${algorithm}, ` +
      `Original: ${originalSize}B, Compressed: ${compressedSize}B, ` +
      `Savings: ${savings}%\n`
    );
  } else {
    process.stderr.write(
      `[Compression] Type: ${dataType}, Skipped (no benefit), ` +
      `Size: ${originalSize}B\n`
    );
  }
}
