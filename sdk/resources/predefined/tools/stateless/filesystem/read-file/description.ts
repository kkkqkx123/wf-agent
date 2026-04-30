/**
 * Tool Description for `read_file`
 */

import type { ToolDescriptionData } from "@wf-agent/prompt-templates";

export const READ_FILE_TOOL_DESCRIPTION: ToolDescriptionData = {
  name: "read_file",
  id: "read_file",
  type: "STATELESS",
  category: "filesystem",
  description:
    "Read a file and return its contents with line numbers for diffing or discussion. IMPORTANT: This tool reads exactly one file per call. If you need multiple files, issue multiple parallel read_file calls." +
    " Supports two modes: 'slice' (default) reads lines sequentially with offset/limit; 'indentation' extracts complete semantic code blocks around an anchor line based on indentation hierarchy." +
    " Slice mode is ideal for initial file exploration, understanding overall structure, reading configuration/data files, or when you need a specific line range. Use it when you don't have a target line number." +
    " PREFER indentation mode when you have a specific line number from search results, error messages, or definition lookups - it guarantees complete, syntactically valid code blocks without mid-function truncation." +
    " IMPORTANT: Indentation mode requires anchor_line to be useful. Without it, only header content (imports) is returned. Obtain anchor_line from: search results, error stack traces, definition lookups, or condensed file summaries (e.g., '14--28 | export class UserService' means anchor_line=14)." +
    " By default, returns up to 2000 lines per file. Lines longer than 2000 characters are truncated." +
    " Automatically detects binary files (images, PDFs, executables). Images are flagged for vision model processing. PDF/DOCX text extraction supported. Other binaries show clear error messages." +
    " Example: { path: 'src/app.ts' }" +
    " Example (indentation mode): { path: 'src/app.ts', mode: 'indentation', indentation: { anchor_line: 42 } }",
  parameters: [
    {
      name: "path",
      type: "string",
      required: true,
      description: "Path to the file to read, relative to the workspace",
    },
    {
      name: "mode",
      type: "string",
      required: false,
      description:
        "Reading mode. 'slice' (default): read lines sequentially with offset/limit. 'indentation': extract complete semantic code blocks containing anchor_line.",
      defaultValue: "slice",
    },
    {
      name: "offset",
      type: "integer",
      required: false,
      description:
        "1-based line offset to start reading from (slice mode, default: 1). Use for large files to read from specific line.",
      defaultValue: 1,
    },
    {
      name: "limit",
      type: "integer",
      required: false,
      description:
        "Maximum number of lines to return (default: 2000). Use with offset for large files to read in chunks.",
      defaultValue: undefined,
    },
    {
      name: "indentation",
      type: "object",
      required: false,
      description:
        "Indentation mode options. Only used when mode='indentation'. You MUST specify anchor_line for useful results.",
      defaultValue: undefined,
    },
  ],
  tips: [
    "Use slice mode for initial exploration when you don't have a target line number",
    "Switch to indentation mode when you have a specific line number from search/error results",
    "ALWAYS specify indentation.anchor_line when using indentation mode",
    "Call this tool multiple times in parallel to read different files",
  ],
};
