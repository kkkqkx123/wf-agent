/**
 * Type definitions for ripgrep executor
 */

/**
 * Ripgrep search options
 */
export interface RipgrepSearchOptions {
  /** Working directory for relative path calculation */
  cwd: string;
  /** Directory path to search in */
  directoryPath: string;
  /** Regular expression pattern (Rust regex syntax) */
  pattern: string;
  /** Optional glob pattern to filter files */
  filePattern?: string;
  /** Number of context lines to show (default: 1) */
  contextLines?: number;
  /** Maximum number of results (default: 300) */
  maxResults?: number;
  /** Maximum line length before truncation (default: 500) */
  maxLineLength?: number;
}

/**
 * Ripgrep list files options
 */
export interface RipgrepListFilesOptions {
  /** Workspace path */
  workspacePath: string;
  /** Maximum number of files to return (default: 500) */
  limit?: number;
  /** Whether to follow symlinks (default: true) */
  follow?: boolean;
  /** Whether to include hidden files (default: true) */
  hidden?: boolean;
  /** Additional glob patterns to exclude */
  excludePatterns?: string[];
}

/**
 * Search result for a single line
 */
export interface SearchLineResult {
  /** Line number (1-indexed) */
  line: number;
  /** Line text content */
  text: string;
  /** Whether this line is a match */
  isMatch: boolean;
  /** Column offset for match (optional) */
  column?: number;
}

/**
 * Search result for a single match group
 */
export interface SearchResult {
  /** Lines in this match group */
  lines: SearchLineResult[];
}

/**
 * Search result for a single file
 */
export interface SearchFileResult {
  /** File path */
  file: string;
  /** Search results in this file */
  searchResults: SearchResult[];
}

/**
 * File result from file listing
 */
export interface FileResult {
  /** Relative file path */
  path: string;
  /** Type of result */
  type: "file" | "folder";
  /** Display label (basename) */
  label?: string;
}
