/**
 * Tool-related error type definitions
 * Provides structured error handling for tool operations
 */

import { SDKError, ErrorSeverity } from "./base.js";

/**
 * Error codes for tool operations
 */
export enum ToolErrorCode {
  // Patch tool specific errors
  PATCH_INVALID_FORMAT = "PATCH_INVALID_FORMAT",
  PATCH_MISSING_BEGIN_MARKER = "PATCH_MISSING_BEGIN_MARKER",
  PATCH_MISSING_END_MARKER = "PATCH_MISSING_END_MARKER",
  PATCH_INVALID_FILE_HEADER = "PATCH_INVALID_FILE_HEADER",
  PATCH_INVALID_HUNK_FORMAT = "PATCH_INVALID_HUNK_FORMAT",
  PATCH_INVALID_ADD_FILE_CONTENT = "PATCH_INVALID_ADD_FILE_CONTENT",
  PATCH_EMPTY_UPDATE_FILE = "PATCH_EMPTY_UPDATE_FILE",

  // File operation errors
  PATCH_FILE_NOT_FOUND = "PATCH_FILE_NOT_FOUND",
  PATCH_FILE_ALREADY_EXISTS = "PATCH_FILE_ALREADY_EXISTS",
  PATCH_FILE_PROTECTED = "PATCH_FILE_PROTECTED",
  PATCH_DIRECTORY_NOT_FOUND = "PATCH_DIRECTORY_NOT_FOUND",
  PATCH_PARENT_DIR_CREATE_FAILED = "PATCH_PARENT_DIR_CREATE_FAILED",
  PATCH_DELETE_FAILED = "PATCH_DELETE_FAILED",
  PATCH_WRITE_FAILED = "PATCH_WRITE_FAILED",

  // Content matching errors
  PATCH_CONTEXT_MISMATCH = "PATCH_CONTEXT_MISMATCH",
  PATCH_HUNK_APPLY_FAILED = "PATCH_HUNK_APPLY_FAILED",
  PATCH_SEEK_FAILED = "PATCH_SEEK_FAILED",
  PATCH_CONTEXT_NOT_FOUND = "PATCH_CONTEXT_NOT_FOUND",
  PATCH_OLD_LINES_NOT_FOUND = "PATCH_OLD_LINES_NOT_FOUND",

  // Path validation errors
  PATCH_INVALID_PATH = "PATCH_INVALID_PATH",
  PATCH_PATH_TRAVERSAL_DETECTED = "PATCH_PATH_TRAVERSAL_DETECTED",
  PATCH_ABSOLUTE_PATH_NOT_ALLOWED = "PATCH_ABSOLUTE_PATH_NOT_ALLOWED",
  PATCH_INVALID_FILENAME_CHARACTERS = "PATCH_INVALID_FILENAME_CHARACTERS",

  // Move/Rename errors
  PATCH_MOVE_FAILED = "PATCH_MOVE_FAILED",
  PATCH_DESTINATION_EXISTS = "PATCH_DESTINATION_EXISTS",
  PATCH_DESTINATION_PATH_INVALID = "PATCH_DESTINATION_PATH_INVALID",

  // System errors
  PATCH_TIMEOUT = "PATCH_TIMEOUT",
  PATCH_UNEXPECTED_ERROR = "PATCH_UNEXPECTED_ERROR",
}

/**
 * Base class for patch tool errors
 */
export class PatchToolError extends SDKError {
  constructor(
    message: string,
    public readonly code: ToolErrorCode,
    public readonly filePath?: string,
    public readonly lineNumber?: number,
    context?: Record<string, unknown>,
    cause?: Error,
    severity?: ErrorSeverity,
  ) {
    super(message, severity, { ...context, toolName: "apply_patch", code, filePath, lineNumber }, cause);
  }

  protected override getDefaultSeverity(): ErrorSeverity {
    return "error";
  }

  /**
   * Convert error to a plain object for serialization
   */
  toObject(): Record<string, unknown> {
    return {
      name: this.name,
      toolName: "apply_patch",
      code: this.code,
      message: this.message,
      filePath: this.filePath,
      lineNumber: this.lineNumber,
      context: this.context,
      cause: this.cause
        ? {
            name: this.cause.name,
            message: this.cause.message,
          }
        : undefined,
    };
  }
}

/**
 * Parse error for patch parsing failures
 */
export class PatchParseError extends PatchToolError {
  constructor(
    message: string,
    code: ToolErrorCode = ToolErrorCode.PATCH_INVALID_FORMAT,
    lineNumber?: number,
    context?: Record<string, unknown>,
  ) {
    super(message, code, undefined, lineNumber, context);
    this.name = "PatchParseError";
  }
}

/**
 * Apply error for patch application failures
 */
export class PatchApplyError extends PatchToolError {
  constructor(
    message: string,
    code: ToolErrorCode,
    filePath?: string,
    context?: Record<string, unknown>,
    cause?: Error,
  ) {
    super(message, code, filePath, undefined, context, cause);
    this.name = "PatchApplyError";
  }
}

/**
 * Validation error for patch validation failures
 */
export class PatchValidationError extends PatchToolError {
  constructor(
    message: string,
    code: ToolErrorCode,
    filePath?: string,
    lineNumber?: number,
    context?: Record<string, unknown>,
  ) {
    super(message, code, filePath, lineNumber, context);
    this.name = "PatchValidationError";
  }
}

/**
 * Convenience factory functions for common patch errors
 */
export const PatchErrors = {
  // Format errors
  invalidFormat: (message?: string) =>
    new PatchParseError(message ?? "Invalid patch format", ToolErrorCode.PATCH_INVALID_FORMAT),

  missingBeginMarker: () =>
    new PatchParseError(
      "Patch must start with '*** Begin Patch'",
      ToolErrorCode.PATCH_MISSING_BEGIN_MARKER,
    ),

  missingEndMarker: () =>
    new PatchParseError(
      "Patch must end with '*** End Patch'",
      ToolErrorCode.PATCH_MISSING_END_MARKER,
    ),

  invalidFileHeader: (line: string, lineNumber: number) =>
    new PatchParseError(
      `Invalid file header: '${line}'. Valid headers are: '*** Add File: {path}', '*** Delete File: {path}', '*** Update File: {path}'`,
      ToolErrorCode.PATCH_INVALID_FILE_HEADER,
      lineNumber,
    ),

  invalidHunkFormat: (message: string, lineNumber: number) =>
    new PatchParseError(message, ToolErrorCode.PATCH_INVALID_HUNK_FORMAT, lineNumber),

  invalidAddFileContent: (line: string, lineNumber: number) =>
    new PatchParseError(
      `Add File section: expected line starting with '+', got: '${line}'`,
      ToolErrorCode.PATCH_INVALID_ADD_FILE_CONTENT,
      lineNumber,
    ),

  emptyUpdateFile: (path: string, lineNumber: number) =>
    new PatchParseError(
      `Update file hunk for path '${path}' is empty`,
      ToolErrorCode.PATCH_EMPTY_UPDATE_FILE,
      lineNumber,
    ),

  // File operation errors
  fileNotFound: (path: string) =>
    new PatchApplyError(`File not found: ${path}`, ToolErrorCode.PATCH_FILE_NOT_FOUND, path),

  fileAlreadyExists: (path: string) =>
    new PatchApplyError(
      `File already exists: ${path}`,
      ToolErrorCode.PATCH_FILE_ALREADY_EXISTS,
      path,
    ),

  parentDirCreateFailed: (path: string, error?: Error) =>
    new PatchApplyError(
      `Failed to create parent directory for: ${path}`,
      ToolErrorCode.PATCH_PARENT_DIR_CREATE_FAILED,
      path,
      { originalError: error?.message },
      error,
    ),

  deleteFailed: (path: string, error?: Error) =>
    new PatchApplyError(`Failed to delete file: ${path}`, ToolErrorCode.PATCH_DELETE_FAILED, path, {
      originalError: error?.message,
    }, error),

  writeFailed: (path: string, error?: Error) =>
    new PatchApplyError(`Failed to write file: ${path}`, ToolErrorCode.PATCH_WRITE_FAILED, path, {
      originalError: error?.message,
    }, error),

  // Content matching errors
  contextNotFound: (context: string, filePath: string) =>
    new PatchApplyError(
      `Failed to find context '${context}' in ${filePath}`,
      ToolErrorCode.PATCH_CONTEXT_NOT_FOUND,
      filePath,
    ),

  oldLinesNotFound: (filePath: string, oldLines: string) =>
    new PatchApplyError(
      `Failed to find expected lines in ${filePath}:\n${oldLines.substring(0, 200)}${oldLines.length > 200 ? "..." : ""}`,
      ToolErrorCode.PATCH_OLD_LINES_NOT_FOUND,
      filePath,
    ),

  // Path validation errors
  invalidPath: (path: string, reason: string, lineNumber?: number) =>
    new PatchValidationError(
      `Invalid path '${path}': ${reason}`,
      ToolErrorCode.PATCH_INVALID_PATH,
      path,
      lineNumber,
    ),

  pathTraversalDetected: (path: string, lineNumber?: number) =>
    new PatchValidationError(
      `Path traversal detected: ${path}. Relative paths only.`,
      ToolErrorCode.PATCH_PATH_TRAVERSAL_DETECTED,
      path,
      lineNumber,
    ),

  absolutePathNotAllowed: (path: string, lineNumber?: number) =>
    new PatchValidationError(
      `Absolute path not allowed: ${path}. Use relative paths only.`,
      ToolErrorCode.PATCH_ABSOLUTE_PATH_NOT_ALLOWED,
      path,
      lineNumber,
    ),

  invalidFilenameCharacters: (path: string, lineNumber?: number) =>
    new PatchValidationError(
      `Invalid characters in path: ${path}`,
      ToolErrorCode.PATCH_INVALID_FILENAME_CHARACTERS,
      path,
      lineNumber,
    ),

  // Move/Rename errors
  moveFailed: (oldPath: string, newPath: string, error?: Error) =>
    new PatchApplyError(
      `Failed to move file from '${oldPath}' to '${newPath}'`,
      ToolErrorCode.PATCH_MOVE_FAILED,
      oldPath,
      { destinationPath: newPath, originalError: error?.message },
      error,
    ),

  destinationExists: (path: string) =>
    new PatchApplyError(
      `Cannot rename: destination path already exists: ${path}`,
      ToolErrorCode.PATCH_DESTINATION_EXISTS,
      path,
    ),

  // System errors
  timeout: (timeoutMs: number) =>
    new PatchApplyError(
      `Operation timed out after ${timeoutMs}ms`,
      ToolErrorCode.PATCH_TIMEOUT,
      undefined,
      { timeoutMs },
    ),

  unexpected: (message: string, error?: Error) =>
    new PatchApplyError(
      message,
      ToolErrorCode.PATCH_UNEXPECTED_ERROR,
      undefined,
      { originalError: error?.message },
      error,
    ),
} as const;
