/**
 * Apply-diff tool utilities
 */

// Diff statistics for SEARCH/REPLACE format
export { computeSearchReplaceStats } from "./diff-stats.js";

// Text normalization
export { unescapeHtmlEntities, escapeHtmlEntities } from "./text-normalization.js";

// SEARCH/REPLACE format utilities
export {
  parseSearchReplaceBlocks,
  validateMarkerSequencing,
  type SearchReplaceBlock,
} from "./search-replace-parser.js";

// Fuzzy matching
export {
  getSimilarity,
  fuzzySearch,
  preserveIndentation,
  BUFFER_LINES,
} from "./fuzzy-matcher.js";
