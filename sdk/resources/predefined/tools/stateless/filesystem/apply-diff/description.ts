/**
 * Tool Description for `apply_diff`
 */

import type { ToolDescriptionData } from "@wf-agent/types";

export const APPLY_DIFF_TOOL_DESCRIPTION: ToolDescriptionData = {
  id: "apply_diff",
  type: "STATELESS",
  category: "filesystem",
  description: `Apply changes to a file using search-replace blocks. This tool finds specific code sections and replaces them with new content.

Format:
<<<<<<< SEARCH
[exact code to find]
=======
[new code to replace with]
>>>>>>> REPLACE

Features:
- Supports multiple search-replace blocks in one call
- Handles whitespace differences and Unicode normalization automatically
- Preserves indentation automatically
- Use :start_line:N or # line: N hint for precise location when needed

Example with line hint:
<<<<<<< SEARCH
:start_line:10
function oldName() {
=======
function newName() {
>>>>>>> REPLACE`,
  parameters: [
    {
      name: "path",
      type: "string",
      required: true,
      description: "The path of the file to edit (relative to the current workspace directory)",
    },
    {
      name: "diff",
      type: "string",
      required: true,
      description:
        "One or more SEARCH/REPLACE blocks. Each block must have <<<<<<< SEARCH, =======, and >>>>>>> REPLACE markers.",
    },
  ],
  tips: [
    "Include 2-3 lines of context around your change for unique identification",
    "Use :start_line:N or # line:N to hint at the location when code appears multiple times",
    "Multiple SEARCH/REPLACE blocks can be used for related changes",
  ],
};
