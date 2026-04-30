/**
 * Misc utilities for file processing and text handling.
 * 
 * This module provides reusable utilities for:
 * - File reading with different modes (slice, indentation)
 * - Binary file detection and handling
 * - Image processing and validation
 * - Text extraction from binary formats (PDF, DOCX)
 * - Line formatting and numbering
 * - Streaming reads for large files
 */

export {
  readWithSlice,
  readWithIndentation,
  type SliceReadResult,
  type IndentationReadResult,
  type IndentationOptions,
} from "./file-reader.js";

export {
  detectBinaryFile,
  detectAndHandleBinaryFile,
  isSupportedImageFormat,
  isSupportedBinaryFormat,
  getSupportedBinaryFormats,
  type BinaryFileResult,
} from "./binary-detector.js";

export {
  validateImageForProcessing,
  processImageFile,
  ImageMemoryTracker,
  type ImageValidationResult,
  type ImageProcessResult,
  DEFAULT_MAX_IMAGE_FILE_SIZE_MB,
  DEFAULT_MAX_TOTAL_IMAGE_SIZE_MB,
} from "./image-processor.js";

export {
  extractTextFromBinary,
  addLineNumbers,
  type TextExtractionResult,
} from "./text-extractor.js";

// Re-export from tool-utils for convenience (already available via @wf-agent/sdk)
export { formatLineNumbers, truncateText } from "../tool-utils.js";

export {
  readLinesWithStream,
  countFileLines,
  type StreamReadResult,
} from "./stream-reader.js";
