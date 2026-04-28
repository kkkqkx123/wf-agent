import { z } from "zod"
import type OpenAI from "openai"

import { createOpenAITool } from "./base"

// ─── Constants ────────────────────────────────────────────────────────────────

/** Default maximum lines to return per file (Codex-inspired predictable limit) */
export const DEFAULT_LINE_LIMIT = 2000

/** Maximum characters per line before truncation */
export const MAX_LINE_LENGTH = 2000

/** Default indentation levels to include above anchor (0 = unlimited) */
export const DEFAULT_MAX_LEVELS = 0

// ─── Schema Definitions ────────────────────────────────────────────────────────

/**
 * Schema for indentation mode parameters.
 * Used when mode='indentation' to extract semantic code blocks.
 */
export const IndentationParamsSchema = z.object({
	anchor_line: z
		.number()
		.optional()
		.describe(
			"1-based line number to anchor the extraction. REQUIRED for meaningful indentation mode results. The extractor finds the semantic block (function, method, class) containing this line and returns it completely. Without anchor_line, indentation mode defaults to line 1 and returns only imports/header content. Obtain anchor_line from: search results, error stack traces, definition lookups, codebase_search results, or condensed file summaries (e.g., '14--28 | export class UserService' means anchor_line=14).",
		),
	max_levels: z
		.number()
		.optional()
		.describe(
			"Maximum indentation levels to include above the anchor (indentation mode, 0 = unlimited (default)). Higher values include more parent context.",
		),
	include_siblings: z
		.boolean()
		.optional()
		.describe(
			"Include sibling blocks at the same indentation level as the anchor block (indentation mode, default: false). Useful for seeing related methods in a class.",
		),
	include_header: z
		.boolean()
		.optional()
		.describe(
			"Include file header content (imports, module-level comments) at the top of output (indentation mode, default: true).",
		),
	max_lines: z
		.number()
		.optional()
		.describe(
			"Hard cap on lines returned for indentation mode. Acts as a separate limit from the top-level 'limit' parameter.",
		),
})

/**
 * Schema for read_file reading mode.
 */
export const ReadFileModeSchema = z.enum(["slice", "indentation"])

/**
 * Schema for read_file tool parameters.
 *
 * Supports two reading modes:
 * 1. **Slice Mode** (default): Simple offset/limit reading
 * 2. **Indentation Mode**: Semantic code block extraction
 */
export const ReadFileParamsSchema = z.object({
	path: z
		.string()
		.describe("Path to the file to read, relative to the workspace"),
	mode: ReadFileModeSchema.optional().describe(
		"Reading mode. 'slice' (default): read lines sequentially with offset/limit - use for general file exploration or when you don't have a target line number (may truncate code mid-function). 'indentation': extract complete semantic code blocks containing anchor_line - PREFERRED when you have a line number because it guarantees complete, valid code blocks. WARNING: Do not use indentation mode without specifying indentation.anchor_line, or you will only get header content.",
	),
	offset: z
		.number()
		.optional()
		.describe("1-based line offset to start reading from (slice mode, default: 1)"),
	limit: z
		.number()
		.optional()
		.describe(`Maximum number of lines to return (slice mode, default: ${DEFAULT_LINE_LIMIT})`),
	indentation: IndentationParamsSchema.optional().describe(
		"Indentation mode options. Only used when mode='indentation'. You MUST specify anchor_line for useful results - it determines which code block to extract.",
	),
})

// ─── Type Exports ──────────────────────────────────────────────────────────────

export type ReadFileParams = z.infer<typeof ReadFileParamsSchema>
export type IndentationParams = z.infer<typeof IndentationParamsSchema>
export type ReadFileMode = z.infer<typeof ReadFileModeSchema>

// ─── Helper Functions ───────────────────────────────────────────────────────────

/**
 * Generates the file support note, optionally including image format support.
 */
function getReadFileSupportsNote(supportsImages: boolean): string {
	if (supportsImages) {
		return `Supports text extraction from PDF and DOCX files. Automatically processes and returns image files (PNG, JPG, JPEG, GIF, BMP, SVG, WEBP, ICO, AVIF) for visual analysis. May not handle other binary files properly.`
	}
	return `Supports text extraction from PDF and DOCX files, but may not handle other binary files properly.`
}

/**
 * Builds the description for the read_file tool.
 */
function buildDescription(supportsImages: boolean): string {
	const descriptionIntro =
		"Read a file and return its contents with line numbers for diffing or discussion. IMPORTANT: This tool reads exactly one file per call. If you need multiple files, issue multiple parallel read_file calls."

	const modeDescription =
		` Supports two modes: 'slice' (default) reads lines sequentially with offset/limit; 'indentation' extracts complete semantic code blocks around an anchor line based on indentation hierarchy.` +
		` Slice mode is ideal for initial file exploration, understanding overall structure, reading configuration/data files, or when you need a specific line range. Use it when you don't have a target line number.` +
		` PREFER indentation mode when you have a specific line number from search results, error messages, or definition lookups - it guarantees complete, syntactically valid code blocks without mid-function truncation.` +
		` IMPORTANT: Indentation mode requires anchor_line to be useful. Without it, only header content (imports) is returned.`

	const limitNote = ` By default, returns up to ${DEFAULT_LINE_LIMIT} lines per file. Lines longer than ${MAX_LINE_LENGTH} characters are truncated.`

	return (
		descriptionIntro +
		modeDescription +
		limitNote +
		" " +
		getReadFileSupportsNote(supportsImages) +
		` Example: { path: 'src/app.ts' }` +
		` Example (indentation mode): { path: 'src/app.ts', mode: 'indentation', indentation: { anchor_line: 42 } }`
	)
}

// ─── Tool Creation ──────────────────────────────────────────────────────────────

/**
 * Options for creating the read_file tool definition.
 */
export interface ReadFileToolOptions {
	/** Whether the model supports image processing (default: false) */
	supportsImages?: boolean
}

/**
 * Creates the read_file tool definition with Codex-inspired modes.
 *
 * @param options - Configuration options for the tool
 * @returns Native tool definition for read_file
 */
export function createReadFileTool(options: ReadFileToolOptions = {}): OpenAI.Chat.ChatCompletionTool {
	const { supportsImages = false } = options

	return createOpenAITool({
		name: "read_file",
		description: buildDescription(supportsImages),
		schema: ReadFileParamsSchema,
		strict: true,
	})
}

/**
 * Default read_file tool definition.
 */
export const readFileTool = createReadFileTool()
