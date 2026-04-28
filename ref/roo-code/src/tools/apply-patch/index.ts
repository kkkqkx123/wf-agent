/**
 * apply_patch tool module
 *
 * A stripped-down, file-oriented diff format designed to be easy to parse and safe to apply.
 * Based on the Codex apply_patch specification.
 */

export { parsePatch } from "./parser"
export type { Hunk, UpdateFileChunk, ApplyPatchArgs } from "./parser"

export { seekSequence } from "./seek-sequence"

export { applyChunksToContent, processHunk, processAllHunks } from "./apply"
export type { ApplyPatchFileChange } from "./apply"

export {
	PatchError,
	PatchErrorCode,
	ValidationError,
	PermissionError,
	PatchErrors,
	ParseError,
	ApplyError,
} from "./errors"
export type { ApplyPatchResult, ApplyPatchSummary, ApplyPatchFileResult } from "./types"
