/**
 * Misc utilities for file processing and text handling.
 * 
 * This module provides reusable utilities for:
 * - File reading with different modes (slice, indentation)
 * - Binary file detection and handling
 * - Image processing and validation
 * - Text extraction from binary formats (PDF, DOCX, XLSX, IPYNB)
 * - Line formatting, numbering, and validation
 * - Smart content truncation
 * - Streaming reads for large files
 * - Token-aware file reading with budget management
 * - Terminal output processing (carriage returns, backspaces, run-length encoding)
 */

export {
  readWithSlice,
  readWithIndentation,
  type SliceReadResult,
  type IndentationReadResult,
  type IndentationOptions,
  type SliceReadOptions,
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

export {
  TextExtractorManager,
} from "./text-extractor-manager.js";

// Re-export from tool-utils for convenience (already available via @wf-agent/sdk)
export { formatLineNumbers, truncateText } from "../tool-utils.js";

export {
  readLinesWithStream,
  countFileLines,
  type StreamReadResult,
} from "./stream-reader.js";

export {
  stripLineNumbers,
  everyLineHasLineNumbers,
  truncateOutput,
} from "./line-number-utils.js";

export {
  countFileLinesAndTokens,
  readFileWithTokenBudget,
  type LineAndTokenCountResult,
  type LineAndTokenCountOptions,
  type ReadWithBudgetResult,
  type ReadWithBudgetOptions,
} from "./token-aware-reader.js";

export {
  processCarriageReturns,
  processBackspaces,
  applyRunLengthEncoding,
} from "./terminal-output-utils.js";
