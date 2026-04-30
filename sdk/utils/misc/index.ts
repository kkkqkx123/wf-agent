/**
 * Misc utilities for file processing and text handling.
 * 
 * This module provides reusable utilities for:
 * - File reading with different modes (slice, indentation)
 * - Line formatting, numbering, and validation
 * - Smart content truncation
 * - Streaming reads for large files
 * - Token-aware file reading with budget management
 * - Terminal output processing (carriage returns, backspaces, run-length encoding)
 *
 * NOTE: For special format files (PDF, DOCX, XLSX, images), use dedicated extraction scripts
 * instead of these utilities. See SPECIAL_FILE_HANDLING.md for guidance.
 */

export {
  readWithSlice,
  readWithIndentation,
  type SliceReadResult,
  type IndentationReadResult,
  type IndentationOptions,
  type SliceReadOptions,
} from "./file-reader.js";

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
