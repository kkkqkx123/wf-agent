/**
 * Type definitions for the apply_patch tool.
 */

import type { PatchErrorCode } from "./errors"

/**
 * Represents a single file operation result.
 */
export interface ApplyPatchFileResult {
	/** Path of the file */
	path: string
	/** Operation type */
	operation: "add" | "delete" | "update" | "rename"
	/** Whether the operation succeeded */
	success: boolean
	/** Error message if failed */
	error?: string
	/** Error code if failed */
	errorCode?: PatchErrorCode
	/** Original path (for rename operations) */
	oldPath?: string
	/** New path (for rename operations) */
	newPath?: string
	/** Diff statistics */
	diffStats?: {
		additions: number
		deletions: number
	}
}

/**
 * Summary of patch application.
 */
export interface ApplyPatchSummary {
	/** Total number of file operations */
	total: number
	/** Number of successful operations */
	succeeded: number
	/** Number of failed operations */
	failed: number
}

/**
 * Result of applying a patch.
 */
export interface ApplyPatchResult {
	/** Whether the overall operation succeeded (all files processed successfully) */
	success: boolean
	/** Results for each file operation */
	results: ApplyPatchFileResult[]
	/** Summary statistics */
	summary: ApplyPatchSummary
	/** Overall error message if the entire operation failed */
	error?: string
}

/**
 * Validation result for patch validation.
 */
export interface PatchValidationResult {
	/** Whether the patch is valid */
	valid: boolean
	/** Number of file operations in the patch */
	fileCount: number
	/** Errors found during validation */
	errors: Array<{
		code: PatchErrorCode
		message: string
		path?: string
		lineNumber?: number
	}>
	/** Warnings found during validation */
	warnings: Array<{
		code: string
		message: string
		path?: string
		lineNumber?: number
	}>
}
