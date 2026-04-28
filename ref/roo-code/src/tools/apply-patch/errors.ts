/**
 * Error types for the apply_patch tool.
 * Provides structured error handling with error codes for better debugging and telemetry.
 */

/**
 * Error codes for patch-related errors.
 */
export enum PatchErrorCode {
	// Format errors
	INVALID_FORMAT = "INVALID_FORMAT",
	MISSING_BEGIN_MARKER = "MISSING_BEGIN_MARKER",
	MISSING_END_MARKER = "MISSING_END_MARKER",
	INVALID_FILE_HEADER = "INVALID_FILE_HEADER",
	INVALID_HUNK_FORMAT = "INVALID_HUNK_FORMAT",
	INVALID_ADD_FILE_CONTENT = "INVALID_ADD_FILE_CONTENT",
	EMPTY_UPDATE_FILE = "EMPTY_UPDATE_FILE",

	// File operation errors
	FILE_NOT_FOUND = "FILE_NOT_FOUND",
	FILE_ALREADY_EXISTS = "FILE_ALREADY_EXISTS",
	DIRECTORY_NOT_FOUND = "DIRECTORY_NOT_FOUND",
	PARENT_DIR_CREATE_FAILED = "PARENT_DIR_CREATE_FAILED",
	DELETE_FAILED = "DELETE_FAILED",
	WRITE_FAILED = "WRITE_FAILED",

	// Content matching errors
	CONTEXT_MISMATCH = "CONTEXT_MISMATCH",
	HUNK_APPLY_FAILED = "HUNK_APPLY_FAILED",
	SEEK_FAILED = "SEEK_FAILED",
	CONTEXT_NOT_FOUND = "CONTEXT_NOT_FOUND",
	OLD_LINES_NOT_FOUND = "OLD_LINES_NOT_FOUND",

	// Permission errors
	PERMISSION_DENIED = "PERMISSION_DENIED",
	WRITE_PROTECTED = "WRITE_PROTECTED",
	ROOIGNORE_VIOLATION = "ROOIGNORE_VIOLATION",

	// Path validation errors
	INVALID_PATH = "INVALID_PATH",
	PATH_TRAVERSAL_DETECTED = "PATH_TRAVERSAL_DETECTED",
	ABSOLUTE_PATH_NOT_ALLOWED = "ABSOLUTE_PATH_NOT_ALLOWED",
	INVALID_FILENAME_CHARACTERS = "INVALID_FILENAME_CHARACTERS",

	// Move/Rename errors
	MOVE_FAILED = "MOVE_FAILED",
	DESTINATION_EXISTS = "DESTINATION_EXISTS",
	DESTINATION_PATH_INVALID = "DESTINATION_PATH_INVALID",

	// System errors
	TIMEOUT = "TIMEOUT",
	DISK_FULL = "DISK_FULL",
	UNEXPECTED_ERROR = "UNEXPECTED_ERROR",
}

/**
 * Base error class for patch-related errors.
 */
export class PatchError extends Error {
	constructor(
		public code: PatchErrorCode,
		message: string,
		public path?: string,
		public lineNumber?: number,
		public details?: Record<string, unknown>,
	) {
		super(message)
		this.name = "PatchError"
	}

	/**
	 * Convert error to a plain object for serialization.
	 */
	toObject(): Record<string, unknown> {
		return {
			name: this.name,
			code: this.code,
			message: this.message,
			path: this.path,
			lineNumber: this.lineNumber,
			details: this.details,
		}
	}
}

/**
 * Parse error for patch parsing failures.
 * Extends PatchError with line number tracking.
 */
export class ParseError extends PatchError {
	constructor(
		message: string,
		lineNumber?: number,
		code: PatchErrorCode = PatchErrorCode.INVALID_FORMAT,
	) {
		super(code, message, undefined, lineNumber)
		this.name = "ParseError"
	}
}

/**
 * Apply error for patch application failures.
 */
export class ApplyError extends PatchError {
	constructor(
		message: string,
		code: PatchErrorCode,
		path?: string,
		details?: Record<string, unknown>,
	) {
		super(code, message, path, undefined, details)
		this.name = "ApplyError"
	}
}

/**
 * Validation error for patch validation failures.
 */
export class ValidationError extends PatchError {
	constructor(
		message: string,
		code: PatchErrorCode,
		path?: string,
		lineNumber?: number,
	) {
		super(code, message, path, lineNumber)
		this.name = "ValidationError"
	}
}

/**
 * Permission error for access control failures.
 */
export class PermissionError extends PatchError {
	constructor(
		message: string,
		code: PatchErrorCode = PatchErrorCode.PERMISSION_DENIED,
		path?: string,
	) {
		super(code, message, path)
		this.name = "PermissionError"
	}
}

// Convenience factory functions for common errors
export const PatchErrors = {
	// Format errors
	invalidFormat: (message?: string) =>
		new ParseError(message ?? "Invalid patch format", undefined, PatchErrorCode.INVALID_FORMAT),

	missingBeginMarker: () =>
		new ParseError("Patch must start with '*** Begin Patch'", undefined, PatchErrorCode.MISSING_BEGIN_MARKER),

	missingEndMarker: () =>
		new ParseError("Patch must end with '*** End Patch'", undefined, PatchErrorCode.MISSING_END_MARKER),

	invalidFileHeader: (line: string, lineNumber: number) =>
		new ParseError(
			`Invalid file header: '${line}'. Valid headers are: '*** Add File: {path}', '*** Delete File: {path}', '*** Update File: {path}'`,
			lineNumber,
			PatchErrorCode.INVALID_FILE_HEADER,
		),

	invalidHunkFormat: (message: string, lineNumber: number) =>
		new ParseError(message, lineNumber, PatchErrorCode.INVALID_HUNK_FORMAT),

	invalidAddFileContent: (line: string, lineNumber: number) =>
		new ParseError(
			`Add File section: expected line starting with '+', got: '${line}'`,
			lineNumber,
			PatchErrorCode.INVALID_ADD_FILE_CONTENT,
		),

	emptyUpdateFile: (path: string, lineNumber: number) =>
		new ParseError(`Update file hunk for path '${path}' is empty`, lineNumber, PatchErrorCode.EMPTY_UPDATE_FILE),

	// File operation errors
	fileNotFound: (path: string) =>
		new ApplyError(`File not found: ${path}`, PatchErrorCode.FILE_NOT_FOUND, path),

	fileAlreadyExists: (path: string) =>
		new ApplyError(`File already exists: ${path}`, PatchErrorCode.FILE_ALREADY_EXISTS, path),

	parentDirCreateFailed: (path: string, error?: Error) =>
		new ApplyError(
			`Failed to create parent directory for: ${path}`,
			PatchErrorCode.PARENT_DIR_CREATE_FAILED,
			path,
			{ originalError: error?.message },
		),

	deleteFailed: (path: string, error?: Error) =>
		new ApplyError(`Failed to delete file: ${path}`, PatchErrorCode.DELETE_FAILED, path, {
			originalError: error?.message,
		}),

	writeFailed: (path: string, error?: Error) =>
		new ApplyError(`Failed to write file: ${path}`, PatchErrorCode.WRITE_FAILED, path, {
			originalError: error?.message,
		}),

	// Content matching errors
	contextMismatch: (context: string, filePath: string) =>
		new ApplyError(
			`Failed to find context '${context}' in ${filePath}`,
			PatchErrorCode.CONTEXT_MISMATCH,
			filePath,
		),

	hunkApplyFailed: (filePath: string, message: string) =>
		new ApplyError(message, PatchErrorCode.HUNK_APPLY_FAILED, filePath),

	contextNotFound: (context: string, filePath: string) =>
		new ApplyError(`Failed to find context '${context}' in ${filePath}`, PatchErrorCode.CONTEXT_NOT_FOUND, filePath),

	oldLinesNotFound: (filePath: string, oldLines: string) =>
		new ApplyError(
			`Failed to find expected lines in ${filePath}:\n${oldLines.substring(0, 200)}${oldLines.length > 200 ? "..." : ""}`,
			PatchErrorCode.OLD_LINES_NOT_FOUND,
			filePath,
		),

	// Permission errors
	permissionDenied: (path: string) =>
		new PermissionError(`Permission denied: ${path}`, PatchErrorCode.PERMISSION_DENIED, path),

	writeProtected: (path: string) =>
		new PermissionError(`File is write-protected: ${path}`, PatchErrorCode.WRITE_PROTECTED, path),

	rooignoreViolation: (path: string) =>
		new PermissionError(`Path is restricted by RooIgnore: ${path}`, PatchErrorCode.ROOIGNORE_VIOLATION, path),

	// Path validation errors
	invalidPath: (path: string, reason: string) =>
		new ValidationError(`Invalid path '${path}': ${reason}`, PatchErrorCode.INVALID_PATH, path),

	pathTraversalDetected: (path: string) =>
		new ValidationError(
			`Path traversal detected: ${path}. Relative paths only.`,
			PatchErrorCode.PATH_TRAVERSAL_DETECTED,
			path,
		),

	absolutePathNotAllowed: (path: string) =>
		new ValidationError(
			`Absolute path not allowed: ${path}. Use relative paths only.`,
			PatchErrorCode.ABSOLUTE_PATH_NOT_ALLOWED,
			path,
		),

	// Move/Rename errors
	moveFailed: (oldPath: string, newPath: string, error?: Error) =>
		new ApplyError(
			`Failed to move file from '${oldPath}' to '${newPath}'`,
			PatchErrorCode.MOVE_FAILED,
			oldPath,
			{ destinationPath: newPath, originalError: error?.message },
		),

	destinationExists: (path: string) =>
		new ApplyError(
			`Cannot rename: destination path already exists: ${path}`,
			PatchErrorCode.DESTINATION_EXISTS,
			path,
		),

	destinationPathInvalid: (path: string, reason: string) =>
		new ApplyError(
			`Cannot rename to invalid path '${path}': ${reason}`,
			PatchErrorCode.DESTINATION_PATH_INVALID,
			path,
		),

	// System errors
	timeout: (timeoutMs: number) =>
		new ApplyError(`Operation timed out after ${timeoutMs}ms`, PatchErrorCode.TIMEOUT, undefined, { timeoutMs }),

	diskFull: () => new ApplyError("Disk is full", PatchErrorCode.DISK_FULL, undefined),

	unexpected: (message: string, error?: Error) =>
		new ApplyError(message, PatchErrorCode.UNEXPECTED_ERROR, undefined, { originalError: error?.message }),
} as const
