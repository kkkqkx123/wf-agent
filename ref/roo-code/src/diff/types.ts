/**
 * Diff handling types
 *
 * This module contains all type definitions related to diff operations.
 */

import type { ToolUse } from "../assistant-message/types"
import type { ToolProgressStatus } from "@coder/types"

/**
 * Result type for diff operations.
 *
 * @remarks
 * A diff operation can either succeed or fail. When successful, it returns
 * the modified content. When it fails, it returns an error message and
 * optional details about why the diff couldn't be applied.
 */
export type DiffResult =
	| DiffSuccess
	| DiffFailure

/**
 * Successful diff result.
 */
export interface DiffSuccess {
	/** Indicates the diff was applied successfully */
	success: true
	/** The modified content after applying the diff */
	content: string
	/** Optional: parts of the diff that failed (for batch operations) */
	failParts?: DiffResult[]
}

/**
 * Failed diff result.
 */
export interface DiffFailure {
	/** Indicates the diff failed */
	success: false
	/** Optional: error message describing the failure */
	error?: string
	/** Optional: detailed information about why the diff failed */
	details?: DiffFailureDetails
	/** Optional: parts of the diff that failed (for batch operations) */
	failParts?: DiffResult[]
}

/**
 * Detailed information about a diff failure.
 */
export interface DiffFailureDetails {
	/** Similarity score between the search string and the best match */
	similarity?: number
	/** Threshold used for matching */
	threshold?: number
	/** The range of the best match found */
	matchedRange?: { start: number; end: number }
	/** The content that was searched */
	searchContent?: string
	/** The best match found */
	bestMatch?: string
}

/**
 * Item type for new diff format (array-based).
 *
 * @remarks
 * Used for the new array-based diff format where each item represents
 * a separate change. This format is more flexible and allows for
 * better handling of complex multi-part changes.
 */
export interface DiffItem {
	/** The content of the diff item */
	content: string
	/** Optional: the starting line number for this item */
	startLine?: number
}

/**
 * Interface for diff strategy implementations.
 *
 * @remarks
 * Diff strategies define how to apply changes to content. Different
 * strategies may use different algorithms and formats. The common
 * interface allows for easy swapping and testing of different approaches.
 */
export interface DiffStrategy {
	/**
	 * Get the name of this diff strategy.
	 *
	 * @returns The name of the strategy (e.g., "search-replace", "patch")
	 */
	getName(): string

	/**
	 * Apply a diff to the original content.
	 *
	 * @param originalContent - The original file content
	 * @param diffContent - The diff content in the strategy's format
	 * @param startLine - Optional line number where the search block starts
	 * @param endLine - Optional line number where the search block ends
	 * @returns A DiffResult object containing either the successful result or error details
	 *
	 * @remarks
	 * The diffContent can be either a string (for legacy formats) or an
	 * array of DiffItem (for the new array-based format). The strategy
	 * should handle both formats appropriately.
	 */
	applyDiff(
		originalContent: string,
		diffContent: string | DiffItem[],
		startLine?: number,
		endLine?: number,
	): Promise<DiffResult>

	/**
	 * Get the progress status for a diff operation.
	 *
	 * @param toolUse - The tool use that triggered this diff
	 * @param result - Optional result from the diff operation
	 * @returns The progress status for the operation
	 *
	 * @remarks
	 * This is an optional method that allows strategies to provide
	 * custom progress information. If not implemented, the default
	 * progress handling will be used.
	 */
	getProgressStatus?(toolUse: ToolUse, result?: any): ToolProgressStatus
}
