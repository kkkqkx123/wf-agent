/**
 * Apply-patch tool utilities
 */

// Type definitions
export type {
  UpdateFileChunk,
  Hunk,
  ApplyPatchArgs,
  ApplyPatchFileResult,
  ApplyPatchSummary,
  ApplyPatchResult,
  ApplyPatchFileChange,
} from "./types.js";

// Parser
export { parsePatch } from "./parser.js";

// Matcher (using shared utility)
export { seekSequence } from "../../../../utils/matcher.js";

// Apply logic
export { applyChunksToContent } from "./apply.js";
