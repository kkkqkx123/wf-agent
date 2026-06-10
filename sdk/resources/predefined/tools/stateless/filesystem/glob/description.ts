/**
 * Tool Description for `glob`
 *
 * Generated at registration time with configurable max results injected.
 */

import type { ToolDescriptionData } from "@wf-agent/types";
import type { GlobConfig } from "../../../types.js";

/**
 * Create glob tool description with configurable limit injected.
 * The limit value is baked into the description text so LLM always knows
 * the actual cap. This avoids inconsistency between config and prompt.
 */
export function createGlobDescription(config?: GlobConfig): ToolDescriptionData {
  const maxResults = config?.maxResults ?? 50;

  return {
    id: "glob",
    type: "STATELESS",
    category: "filesystem",
    description: `Find files and directories by name using glob pattern matching. Supports standard glob patterns including *, **, ?, and {...}. Results are sorted with directories first, then files, alphabetically within each group. Up to ${maxResults} entries are returned; use a more specific pattern if results are truncated.`,
    parameters: [
      {
        name: "path",
        type: "string",
        required: true,
        description:
          "The directory to search in (relative to the current workspace directory)",
      },
      {
        name: "pattern",
        type: "string",
        required: true,
        description:
          "Glob pattern to match file/directory names against. The pattern alone controls recursion: use '**' for recursive (e.g., 'src/**/*.ts'), use '*' for top-level only (e.g., '*.ts'). Examples: '*.{json,yaml}', '**/*.test.ts', '.env*'",
      },
      {
        name: "includeIgnored",
        type: "boolean",
        required: false,
        description:
          "Whether to include typically ignored directories (node_modules, .git, target, etc.). Set to true only when you specifically need to search inside these directories. (default: false)",
        defaultValue: false,
      },
    ],
    tips: [
      "Recursion is controlled by the pattern, not a separate flag. Use ** for recursive (e.g., 'src/**/*.ts'), omit it for top-level only (e.g., '*.ts')",
      "Use * to match files within a single directory (e.g., '*.json')",
      "Use ? to match a single character (e.g., 'file-?.txt')",
      "Use {a,b} for alternatives (e.g., '*.{ts,js}')",
      "Dotfiles are included in matching (e.g., '.*' matches .gitignore)",
      `Results are limited to ${maxResults} entries. Narrow your pattern (e.g., add path prefix or more specific chars) if results are truncated.`,
      "By default, build artifacts (node_modules, target, .next, dist) and hidden dirs (.git, .idea) are excluded. Set includeIgnored=true to bypass.",
    ],
  };
}