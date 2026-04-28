/**
 * Type definitions for search service
 */

/**
 * Options for file search
 */
export interface FileSearchOptions {
  /** Search query string */
  query: string;
  /** Workspace path to search in */
  workspacePath: string;
  /** Maximum number of results to return (default: 20) */
  limit?: number;
}

/**
 * Options for listing all files
 */
export interface ListAllFilesOptions {
  /** Workspace path */
  workspacePath: string;
  /** Maximum number of files to return (default: 10000) */
  limit?: number;
}

/**
 * File search result
 */
export interface FileSearchResult {
  /** Relative file path */
  path: string;
  /** Type of result */
  type: "file" | "folder";
  /** Display label (basename) */
  label?: string;
}
