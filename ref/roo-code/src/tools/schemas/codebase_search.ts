import { z } from "zod"
import type OpenAI from "openai"

import { createOpenAITool } from "./base"

// ─── Schema Definitions ────────────────────────────────────────────────────────

/**
 * Schema for a single search query with optional path filter.
 */
export const SearchQuerySchema = z.object({
	query: z.string().describe("The search query string"),
	path: z.string().optional().describe("Optional path to limit the search scope"),
})

/**
 * Schema for codebase_search tool parameters.
 * Supports both string queries and object queries with path filters.
 */
export const CodebaseSearchParamsSchema = z.object({
	queries: z
		.array(
			z.union([
				z.string(),
				SearchQuerySchema,
			]),
		)
		.describe('Array of queries for batch search. Can be array of strings or array of objects with "query" and optional "path" fields'),
})

// ─── Type Exports ──────────────────────────────────────────────────────────────

export type CodebaseSearchParams = z.infer<typeof CodebaseSearchParamsSchema>
export type SearchQuery = z.infer<typeof SearchQuerySchema>

// ─── Tool Creation ──────────────────────────────────────────────────────────────

const CODEBASE_SEARCH_DESCRIPTION = `This tool uses semantic search to find relevant code based on meaning rather than just keywords. For precise search (like a certain function name), use regex_search (search_files tool) instead.

**IMPORTANT**: Always use simple query terms. Avoid using complex queries that contain multiple content. Queries MUST be in English (translate if needed).

**Batch Query Support**: You can provide multiple queries in a single call for complex searches. This is more efficient than multiple separate calls. Results are automatically deduplicated and results matching multiple queries get a score boost (+5% per additional match).

Parameters:
- queries: (required) Array of queries for batch search. Can be:
  - Array of strings: ["error handling", "exception handling"]
  - Array of objects: [{ "query": "error handling", "path": "src/utils" }]

Examples:

Single query:
{ "queries": ["authentication validation"] }

Batch query (simple):
{ "queries": ["error handling", "exception handling", "try catch"] }

Batch query with paths:
{
  "queries": [
    { "query": "error handling", "path": "src/utils" },
    { "query": "exception handling", "path": "src/api" }
  ]
}`

/**
 * Creates the codebase_search tool definition.
 *
 * @returns Native tool definition for codebase_search
 */
export function createCodebaseSearchTool(): OpenAI.Chat.ChatCompletionTool {
	return createOpenAITool({
		name: "codebase_search",
		description: CODEBASE_SEARCH_DESCRIPTION,
		schema: CodebaseSearchParamsSchema,
		strict: true,
	})
}

/**
 * Default codebase_search tool definition.
 */
export const codebaseSearchTool = createCodebaseSearchTool()
