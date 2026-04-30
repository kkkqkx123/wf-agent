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
    "Read file contents from the filesystem with enhanced features:\n\n" +
    "**Reading Modes:**\n" +
    "- Slice mode (default): Read specific line ranges using offset/limit parameters\n" +
    "- Indentation mode: Extract semantic code blocks based on indentation hierarchy (coming soon)\n\n" +
    "**Binary File Handling:**\n" +
    "- Automatically detects binary files (images, PDFs, executables)\n" +
    "- Images: Detected and flagged for vision model processing\n" +
    "- PDF/DOCX: Text extraction support (requires additional libraries)\n" +
    "- Other binaries: Clear error messages indicating unsupported format\n\n" +
    "**Output Format:**\n" +
    "Line numbers always included in 'LINE_NUMBER|LINE_CONTENT' format (1-indexed).\n" +
    "Smart truncation warnings guide you to read more content when files are large.\n\n" +
    "**Best Practices:**\n" +
    "- Use offset and limit for large files to read in manageable chunks\n" +
    "- Call this tool multiple times in parallel to read different files simultaneously\n" +
    "- Line numbers make it easy to reference specific sections in follow-up operations",
  parameters: [
    {
      name: "path",
      type: "string",
      required: true,
      description: "Absolute or relative path to the file",
    },
    {
      name: "mode",
      type: "string",
      required: false,
      description: "Reading mode: 'slice' for line ranges (default). Indentation mode coming soon.",
      defaultValue: "slice",
    },
    {
      name: "offset",
      type: "integer",
      required: false,
      description:
        "Starting line number (1-indexed). Use for large files to read from specific line. Default: 1",
      defaultValue: 1,
    },
    {
      name: "limit",
      type: "integer",
      required: false,
      description:
        "Number of lines to read. Use with offset for large files to read in chunks. Default: 100",
      defaultValue: undefined,
    },
  ],
  tips: [
    "Use offset and limit for large files to read in chunks (e.g., offset=1, limit=100)",
    "Call this tool multiple times in parallel to read different files simultaneously",
    "Output includes line numbers for easy reference in subsequent operations",
    "Binary files (images, PDFs) are automatically detected with helpful error messages",
    "Truncation warnings include guidance on how to read the next chunk of content",
  ],
};
