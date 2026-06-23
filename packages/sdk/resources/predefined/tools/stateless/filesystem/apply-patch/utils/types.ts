/**
 * Type definitions for the apply_patch tool
 */

import type { ToolErrorCode } from "@wf-agent/types";

/**
 * A chunk within an UpdateFile hunk
 */
export interface UpdateFileChunk {
  /** Optional context line (e.g., class or function name) to narrow search */
  changeContext: string | null;
  /** Lines to find and replace (context + removed lines) */
  oldLines: string[];
  /** Lines to replace with (context + added lines) */
  newLines: string[];
  /** If true, old_lines must match at end of file */
  isEndOfFile: boolean;
}

/**
 * Represents a file operation in a patch
 */
export type Hunk =
  | {
      type: "AddFile";
      path: string;
      contents: string;
    }
  | {
      type: "DeleteFile";
      path: string;
    }
  | {
      type: "UpdateFile";
      path: string;
      movePath: string | null;
      chunks: UpdateFileChunk[];
    };

/**
 * Result of parsing a patch
 */
export interface ApplyPatchArgs {
  hunks: Hunk[];
  patch: string;
}

/**
 * Represents a single file operation result
 */
export interface ApplyPatchFileResult {
  /** Path of the file */
  path: string;
  /** Operation type */
  operation: "add" | "delete" | "update" | "rename";
  /** Whether the operation succeeded */
  success: boolean;
  /** Error message if failed */
  error?: string;
  /** Error code if failed */
  errorCode?: ToolErrorCode;
  /** Original path (for rename operations) */
  oldPath?: string;
  /** New path (for rename operations) */
  newPath?: string;
}

/**
 * Summary of patch application
 */
export interface ApplyPatchSummary {
  /** Total number of file operations */
  total: number;
  /** Number of successful operations */
  succeeded: number;
  /** Number of failed operations */
  failed: number;
}

/**
 * Result of applying a patch
 */
export interface ApplyPatchResult {
  /** Whether the overall operation succeeded (all files processed successfully) */
  success: boolean;
  /** Results for each file operation */
  results: ApplyPatchFileResult[];
  /** Summary statistics */
  summary: ApplyPatchSummary;
  /** Overall error message if the entire operation failed */
  error?: string;
}

/**
 * Result of applying a patch to a file
 */
export interface ApplyPatchFileChange {
  type: "add" | "delete" | "update";
  /** Original path of the file */
  path: string;
  /** New path if the file was moved/renamed */
  movePath?: string;
  /** Original content (for delete/update) */
  originalContent?: string;
  /** New content (for add/update) */
  newContent?: string;
}
