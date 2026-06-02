/**
 * Type definitions for the apply_diff tool
 */

/**
 * A parsed SEARCH/REPLACE block
 */
export interface SearchReplaceBlock {
  /** Content to search for */
  searchContent: string;
  /** Content to replace with */
  replaceContent: string;
  /** Optional line number hint for precise location */
  startLine?: number;
  /** Optional context hint (function/class name) for disambiguation */
  contextHint?: string;
}

/**
 * Result of applying a single block
 */
export interface BlockApplyResult {
  /** Whether the block was applied successfully */
  success: boolean;
  /** Error message if failed */
  error?: string;
}

/**
 * Configuration for the apply_diff handler
 */
export interface ApplyDiffConfig {
  /** Workspace directory for resolving relative paths */
  workspaceDir?: string;
  /** Enable file protection checks */
  enableProtect?: boolean;
  /** Model ID (for HTML entity unescaping) */
  modelId?: string;
}
