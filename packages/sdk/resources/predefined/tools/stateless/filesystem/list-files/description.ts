/**
 * Tool Description for `list_files`
 *
 * Generated at registration time with configurable max results injected.
 */

import type { ToolDescriptionData } from "@wf-agent/types";
import type { ListFilesConfig } from "../../../types.js";

/**
 * Create list_files tool description with configurable limit injected.
 * The limit value is baked into the description text so LLM always knows
 * the actual cap. This avoids inconsistency between config and prompt.
 */
export function createListFilesDescription(config?: ListFilesConfig): ToolDescriptionData {
  const maxResults = config?.maxResults ?? 1000;

  return {
    id: "list_files",
    type: "STATELESS",
    category: "filesystem",
    description: `List files and directories in a specified path. Supports recursive listing. Results are sorted with directories first, then files, alphabetically within each group. Up to ${maxResults} entries are returned; use path to target deeper.`,
    parameters: [
      {
        name: "path",
        type: "string",
        required: true,
        description: "The directory to list (relative to the current workspace directory)",
      },
      {
        name: "recursive",
        type: "boolean",
        required: false,
        description:
          "Whether to list recursively. Use true for recursive listing, false for top-level only (default: false)",
        defaultValue: false,
      },
      {
        name: "includeIgnored",
        type: "boolean",
        required: false,
        description:
          "Whether to include typically ignored directories (node_modules, .git, target, etc.). Set to true only when you specifically need to view files inside these directories. (default: false)",
        defaultValue: false,
      },
    ],
    tips: [
      `Results are limited to ${maxResults} entries. Target a deeper subdirectory if the listing is truncated.`,
      "Recursive listing can return many results. Use top-level listing first to explore, then target subdirectories with specific paths.",
      "By default, build artifacts (node_modules, target, .next, dist) and hidden dirs (.git, .idea) are excluded. Set includeIgnored=true to bypass.",
      "Use the glob tool instead when you need pattern-based file search (supports *, **, ?, {...} patterns).",
    ],
  };
}
