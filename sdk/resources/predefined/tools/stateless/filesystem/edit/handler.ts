/**
 * The logic executed by the edit tool
 */

import { readFile, writeFile } from "fs/promises";
import { existsSync } from "fs";
import type { ToolOutput } from "@wf-agent/types";
import type { EditFileConfig } from "../../../types.js";
import { ProtectController, SHIELD_SYMBOL } from "@wf-agent/sdk/services";
import { normalizeUnicode } from "../../../utils/matcher.js";

/**
 * Parse paths (relative paths are supported)
 */
function resolvePath(filePath: string, workspaceDir?: string): string {
  if (filePath.startsWith("/") || filePath.match(/^[A-Za-z]:\\/)) {
    return filePath;
  }
  const baseDir = workspaceDir ?? process.cwd();
  return `${baseDir}/${filePath}`.replace(/\\/g, "/");
}

/**
 * Create the `edit` tool execution function
 */
export function createEditHandler(config: EditFileConfig = {}) {
  return async (params: Record<string, unknown>): Promise<ToolOutput> => {
    try {
      const { file_path, old_string, new_string, replace_all, require_unique } = params as {
        file_path: string;
        old_string: string;
        new_string: string;
        replace_all?: boolean;
        require_unique?: boolean;
      };

      const filePath = resolvePath(file_path, config.workspaceDir);
      const workspaceDir = config.workspaceDir ?? process.cwd();

      // Initialize protect controller if enabled
      let protectController: ProtectController | undefined;
      if (config.enableProtect) {
        protectController = new ProtectController({ cwd: workspaceDir });
      }

      // Check if file is write-protected
      const isProtected = protectController?.isWriteProtected(file_path) ?? false;
      if (isProtected) {
        return {
          success: false,
          content: "",
          error: `${SHIELD_SYMBOL} Edit operation blocked: ${file_path} is a protected file and requires explicit approval for modifications`,
        };
      }

      if (!existsSync(filePath)) {
        return {
          success: false,
          content: "",
          error: `File not found: ${file_path}`,
        };
      }

      if (old_string === undefined || old_string === null) {
        return {
          success: false,
          content: "",
          error: "Missing 'old_string' parameter",
        };
      }

      if (new_string === undefined || new_string === null) {
        return {
          success: false,
          content: "",
          error: "Missing 'new_string' parameter",
        };
      }

      const content = await readFile(filePath, "utf-8");

      // Determine if fuzzy matching is allowed
      // Fuzzy matching (Unicode normalization) is only allowed when require_unique is true or not specified
      // When require_unique is explicitly false, we enforce exact matching for safety
      const allowFuzzyMatch = require_unique !== false;

      // Try to find the old_string with optional fuzzy matching
      let matchIndex = content.indexOf(old_string);
      let usedFuzzyMatch = false;

      // Step 1: If exact match fails and fuzzy matching is allowed, try Unicode normalization
      if (matchIndex === -1 && allowFuzzyMatch) {
        const normalizedOld = normalizeUnicode(old_string);
        const normalizedContent = content.split('\n').map(normalizeUnicode).join('\n');
        const normalizedIndex = normalizedContent.indexOf(normalizedOld);

        if (normalizedIndex !== -1) {
          matchIndex = normalizedIndex;
          usedFuzzyMatch = true;
        }
      }

      // Step 2: If still not found, return detailed error
      if (matchIndex === -1) {
        const errorMsg = allowFuzzyMatch
          ? `String not found in file: "${old_string.substring(0, 100)}${old_string.length > 100 ? "..." : ""}"\n\nSuggestions:\n1. Read the file first to get the exact content\n2. Check for Unicode character differences (e.g., fancy quotes vs ASCII quotes)\n3. Use apply_diff for complex code edits`
          : `String not found in file: "${old_string.substring(0, 100)}${old_string.length > 100 ? "..." : ""}"\n\nNote: Fuzzy matching is disabled because require_unique=false. The string must match exactly.`;

        return {
          success: false,
          content: "",
          error: errorMsg,
        };
      }

      // Count occurrences for uniqueness check
      const occurrences = content.split(old_string).length - 1;

      // Check uniqueness (default is true)
      const shouldRequireUnique = require_unique !== false; // Default to true
      if (shouldRequireUnique && occurrences > 1) {
        return {
          success: false,
          content: "",
          error: `Found ${occurrences} occurrences of old_string. The string must be unique in the file.\n\nTo replace all occurrences, set require_unique=false and replace_all=true.\nTo replace a specific occurrence, make old_string more unique by including surrounding context.`,
        };
      }

      let newContent: string;
      let replacementCount: number;

      if (replace_all) {
        // Replace all occurrences (only allowed when require_unique=false)
        const parts = content.split(old_string);
        replacementCount = parts.length - 1;
        newContent = parts.join(new_string);
      } else {
        // Replace only the matched occurrence (uses matchIndex which may be from fuzzy match)
        newContent =
          content.substring(0, matchIndex) + new_string + content.substring(matchIndex + old_string.length);
        replacementCount = 1;
      }

      await writeFile(filePath, newContent, "utf-8");

      const fuzzyWarning = usedFuzzyMatch
        ? "\n\n⚠️ Note: Used Unicode normalization to match the string (e.g., fancy quotes converted to ASCII)."
        : "";

      return {
        success: true,
        content: `Edited ${file_path}: replaced ${replacementCount} occurrence(s)${fuzzyWarning}`,
      };
    } catch (error) {
      return {
        success: false,
        content: "",
        error: error instanceof Error ? error.message : String(error),
      };
    }
  };
}
