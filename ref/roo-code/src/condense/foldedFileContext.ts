import * as path from "path"
import { RooIgnoreController } from "../ignore/RooIgnoreController"
import {
	foldSingleFile,
	MergedSection,
	ParsedDefinitionLine,
	applyRandomDrop,
	estimateAvgTokensPerSection,
	formatMergedSections,
	formatSectionsWithOriginalContent,
} from "../file-folding"
import { tiktoken } from "../../utils/tiktoken"

/**
 * Result of generating folded file context.
 */
export interface FoldedFileContextResult {
	/** The formatted string containing all folded file definitions (joined) */
	content: string
	/** Individual file sections, each in its own <system-reminder> block */
	sections: string[]
	/** Number of files successfully processed */
	filesProcessed: number
	/** Number of files that failed or were skipped */
	filesSkipped: number
	/** Total character count of the folded content */
	characterCount: number
	/** Total token count of the folded content */
	totalTokens: number
	/** Number of sections dropped due to token limit */
	sectionsDropped: number
}

/**
 * Options for generating folded file context.
 */
export interface FoldedFileContextOptions {
	/** Maximum total tokens for the folded content (default: 10000) */
	maxTokens?: number
	/** Maximum total characters for the folded content (deprecated, use maxTokens instead) */
	maxCharacters?: number
	/** The current working directory for resolving relative paths */
	cwd: string
	/** Optional RooIgnoreController for file access validation */
	rooIgnoreController?: RooIgnoreController
	/** Whether to merge function blocks (default: true) */
	mergeFunctions?: boolean
	/** Maximum line span before interrupting function merging (default: 100) */
	maxLineSpan?: number
	/** Format mode: 'detailed' (full content) or 'minimal' (names only, no 'other' types) */
	mode?: "detailed" | "minimal"
}

/**
 * Generates folded (signatures-only) file context for a list of files using tree-sitter.
 *
 * This function takes file paths that were read during a conversation and produces
 * a condensed representation showing only function signatures, class declarations,
 * and other important structural definitions - hiding implementation bodies.
 *
 * Each file is wrapped in its own `<system-reminder>` block during context condensation,
 * allowing the model to retain awareness of file structure without consuming excessive tokens.
 *
 * @param filePaths - Array of file paths to process (relative to cwd)
 * @param options - Configuration options including cwd and max characters
 * @returns FoldedFileContextResult with the formatted content and statistics
 *
 * @example
 * ```typescript
 * const result = await generateFoldedFileContext(
 *   ['src/utils/helpers.ts', 'src/api/client.ts'],
 *   { cwd: '/project', maxCharacters: 30000 }
 * )
 * // result.content contains individual <system-reminder> blocks for each file:
 * // <system-reminder>
 * // ## File Context: src/utils/helpers.ts
 * // 1--15 | export function formatDate(...)
 * // 17--45 | export class DateHelper {...}
 * // </system-reminder>
 * // <system-reminder>
 * // ## File Context: src/api/client.ts
 * // ...
 * // </system-reminder>
 * ```
 */
/**
 * Represents a folded file with its sections.
 */
interface FoldedFile {
	filePath: string
	sections: MergedSection[]
	content: string
	tokens: number
	/** Original parsed lines for output formatting */
	parsedLines: ParsedDefinitionLine[]
}

export async function generateFoldedFileContext(
	filePaths: string[],
	options: FoldedFileContextOptions,
): Promise<FoldedFileContextResult> {
	const {
		maxTokens = 10000,
		maxCharacters,
		cwd,
		rooIgnoreController,
		mergeFunctions = true,
		maxLineSpan = 100,
		mode = "minimal",
	} = options

	// Support legacy maxCharacters option by converting to approximate token count
	const effectiveMaxTokens = maxCharacters ? Math.floor(maxCharacters / 4) : maxTokens

	const result: FoldedFileContextResult = {
		content: "",
		sections: [],
		filesProcessed: 0,
		filesSkipped: 0,
		characterCount: 0,
		totalTokens: 0,
		sectionsDropped: 0,
	}

	if (filePaths.length === 0) {
		return result
	}

	const foldedFiles: FoldedFile[] = []
	const failedFiles: string[] = []

	// Step 1: Fold each file individually using the file-folding API
	for (const filePath of filePaths) {
		try {
			// Resolve to absolute path for tree-sitter
			const absolutePath = path.isAbsolute(filePath) ? filePath : path.resolve(cwd, filePath)

			// Use foldSingleFile to process the file
			const foldedFile = await foldSingleFile(absolutePath, rooIgnoreController, {
				mergeFunctions,
				maxLineSpan,
				formatOptions: {
					filePath,
					wrapInSystemReminder: true,
					mode,
				},
			})

			if (!foldedFile) {
				result.filesSkipped++
				continue
			}

			foldedFiles.push({
				filePath,
				sections: foldedFile.sections,
				content: foldedFile.content,
				tokens: foldedFile.tokens,
				parsedLines: foldedFile.parsedLines,
			})

			result.filesProcessed++
		} catch (error) {
			failedFiles.push(filePath)
			result.filesSkipped++
		}
	}

	// Log failed files as a single batch summary
	if (failedFiles.length > 0) {
		console.warn(
			`Folded context generation: skipped ${failedFiles.length} file(s) due to errors: ${failedFiles.slice(0, 5).join(", ")}${failedFiles.length > 5 ? ` and ${failedFiles.length - 5} more` : ""}`,
		)
	}

	if (foldedFiles.length === 0) {
		return result
	}

	// Step 2: Calculate total tokens
	let totalTokens = foldedFiles.reduce((sum, file) => sum + file.tokens, 0)

	// Step 3: Check if we need to apply random drop
	if (totalTokens > effectiveMaxTokens) {
		// Flatten all sections from all files
		const allSections: Array<{ section: MergedSection; fileIndex: number }> = []
		for (let i = 0; i < foldedFiles.length; i++) {
			const file = foldedFiles[i]!
			for (const section of file.sections) {
				allSections.push({ section, fileIndex: i })
			}
		}

		// Estimate average tokens per section
		const avgTokensPerSection = await estimateAvgTokensPerSection(
			allSections.map((item) => item.section),
			(section) => formatMergedSections([section]),
		)

		// Apply random drop
		const dropResult = applyRandomDrop(allSections.map((item) => item.section), {
			maxTokens: effectiveMaxTokens,
			currentTokens: totalTokens,
			avgTokensPerSection,
		})

		// Rebuild files with kept sections
		const newFoldedFiles: FoldedFile[] = []
		const keptSectionIndices = new Set<number>()

		// Mark kept sections
		for (let i = 0; i < dropResult.keptSections.length; i++) {
			const keptSection = dropResult.keptSections[i]!
			// Find the original index
			const originalIndex = allSections.findIndex(
				(item) =>
					item.section.type === keptSection.type &&
					item.section.names.length === keptSection.names.length &&
					item.section.names.every((name, idx) => name === keptSection.names[idx]),
			)
			if (originalIndex !== -1) {
				keptSectionIndices.add(originalIndex)
			}
		}

		// Rebuild files
		for (let i = 0; i < foldedFiles.length; i++) {
			const file = foldedFiles[i]!
			const keptSectionsForFile: MergedSection[] = []

			for (let j = 0; j < allSections.length; j++) {
				const item = allSections[j]!
				if (item.fileIndex === i && keptSectionIndices.has(j)) {
					keptSectionsForFile.push(item.section)
				}
			}

			// Only include files that have at least one section
			if (keptSectionsForFile.length > 0) {
				const content = formatSectionsWithOriginalContent(keptSectionsForFile, file.parsedLines, mode)
				const fileContent = `<system-reminder>
## File Context: ${file.filePath}
${content}
</system-reminder>`
				const tokens = await tiktoken([{ type: "text", text: fileContent }])

				newFoldedFiles.push({
					filePath: file.filePath,
					sections: keptSectionsForFile,
					content: fileContent,
					tokens,
					parsedLines: file.parsedLines,
				})
			}
		}

		// Update result
		result.sectionsDropped = dropResult.droppedCount
		result.filesProcessed = newFoldedFiles.length
		result.filesSkipped += foldedFiles.length - newFoldedFiles.length

		// Update folded files
		foldedFiles.length = 0
		foldedFiles.push(...newFoldedFiles)

		// Recalculate total tokens
		totalTokens = foldedFiles.reduce((sum, file) => sum + file.tokens, 0)
	}

	// Step 4: Build final result
	if (foldedFiles.length > 0) {
		result.sections = foldedFiles.map((file) => file.content)
		result.content = result.sections.join("\n")
		result.characterCount = result.content.length
		result.totalTokens = totalTokens
	}

	return result
}
