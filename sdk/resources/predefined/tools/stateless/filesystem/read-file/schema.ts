/**
 * The `read_file` tool parameter `Schema` is defined as follows:
 */

import type { ToolParameterSchema } from "@wf-agent/types";

/** Default maximum lines to return per file */
const DEFAULT_LINE_LIMIT = 2000;

/** Default maximum characters to return (protects against extremely long lines) */
const DEFAULT_CHAR_LIMIT = 50000;

/**
 * Schema for indentation mode parameters.
 * Used when mode='indentation' to extract semantic code blocks.
 */
const indentationParamsSchema: ToolParameterSchema = {
  type: "object",
  properties: {
    anchor_line: {
      type: "integer",
      description:
        "REQUIRED: 1-based line number to anchor the extraction. The extractor finds the semantic block (function, method, class) containing this line and returns it completely. Without anchor_line, indentation mode defaults to line 1 and returns only imports/header content. Obtain anchor_line from: search results, error stack traces, definition lookups, or condensed file summaries (e.g., '14--28 | export class UserService' means anchor_line=14).",
      minimum: 1,
    },
    max_levels: {
      type: "integer",
      description:
        "Maximum indentation levels to include above the anchor (0 = unlimited, default). Higher values include more parent context.",
      minimum: 0,
    },
    include_siblings: {
      type: "boolean",
      description:
        "Include sibling blocks at the same indentation level as the anchor block (default: false). Useful for seeing related methods in a class.",
    },
    include_header: {
      type: "boolean",
      description:
        "Include file header content (imports, module-level comments) at the top of output (default: true).",
    },
    max_lines: {
      type: "integer",
      description:
        "Hard cap on lines returned for indentation mode. Acts as a separate limit from the top-level 'limit' parameter.",
      minimum: 1,
    },
  },
  required: [],
};

/**
 * read_file tool parameter Schema
 *
 * Supports two reading modes:
 * 1. **Slice Mode** (default): Simple offset/limit reading for sequential line ranges
 * 2. **Indentation Mode**: Semantic code block extraction based on indentation hierarchy
 */
export const readFileSchema: ToolParameterSchema = {
  type: "object",
  properties: {
    path: {
      type: "string",
      description: "Path to the file to read, relative to the workspace",
    },
    mode: {
      type: "string",
      enum: ["slice", "indentation"],
      description:
        "Reading mode. 'slice' (default): read lines sequentially with offset/limit - use for general file exploration or when you don't have a target line number (may truncate code mid-function). 'indentation': extract complete semantic code blocks containing anchor_line - PREFERRED when you have a line number because it guarantees complete, valid code blocks. WARNING: Do not use indentation mode without specifying indentation.anchor_line, or you will only get header content.",
      default: "slice",
    },
    offset: {
      type: "integer",
      description:
        "1-based line offset to start reading from (slice mode, default: 1). Use for large files to read from specific line.",
      minimum: 1,
    },
    limit: {
      type: "integer",
      description: `Maximum number of lines to return (default: ${DEFAULT_LINE_LIMIT}). Use with offset for large files to read in chunks.`,
      minimum: 1,
    },
    max_chars: {
      type: "integer",
      description: `Maximum total characters to return across all lines (default: ${DEFAULT_CHAR_LIMIT}). Prevents excessive output from files with extremely long lines (e.g., minified JSON). Takes precedence over line limit when exceeded.`,
      minimum: 1,
    },
    indentation: {
      ...indentationParamsSchema,
      description:
        "Indentation mode options. Only used when mode='indentation'. You MUST specify anchor_line for useful results - it determines which code block to extract.",
    },
  },
  required: ["path"],
};
