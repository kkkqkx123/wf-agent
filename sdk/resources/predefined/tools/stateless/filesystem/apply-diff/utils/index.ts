/**
 * Apply-diff tool utilities
 */

// Type definitions
export type { SearchReplaceBlock, BlockApplyResult, ApplyDiffConfig } from "./types.js";

// Parser
export { parseSearchReplaceBlocks, validateMarkerSequencing } from "./parser.js";

// Apply logic
export { applyBlock } from "./apply.js";
