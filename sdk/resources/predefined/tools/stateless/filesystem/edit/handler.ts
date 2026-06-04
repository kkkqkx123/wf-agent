/**
 * The logic executed by the edit tool
 */

import { readFile as fsReadFile, writeFile as fsWriteFile } from "fs/promises";
import { existsSync } from "fs";
import type { ToolOutput } from "@wf-agent/types";
import type { EditFileConfig } from "../../../types.js";
import { ProtectController, SHIELD_SYMBOL } from "@wf-agent/sdk/services";
import { normalizeUnicode } from "../../../utils/matcher.js";
import { resolveFilePath } from "@wf-agent/sdk/utils";

/**
 * Create the `edit` tool execution function
 */
export function createEditHandler(config: EditFileConfig = {}) {
  return async (params: Record<string, unknown>): Promise<ToolOutput> => {
    try {
      const { file_path, old_string, new_string, mode } = params as {
        file_path: string;
        old_string: string;
        new_string: string;
        mode?: "safe" | "batch";
      };

      const filePath = resolveFilePath(file_path, config.workspaceDir);
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

      // Read file content (via VFS or Host FS)
      let content: string;

      if (config.vfs) {
        const vfsPath = filePath.replace(/\\/g, "/");
        const exists = await config.vfs.exists(vfsPath);
        if (!exists) {
          return {
            success: false,
            content: "",
            error: `File not found: ${file_path}`,
          };
        }
        const buf = await config.vfs.readFile(vfsPath);
        if (!buf) {
          return {
            success: false,
            content: "",
            error: `File not found: ${file_path}`,
          };
        }
        content = Buffer.from(buf).toString("utf-8");
      } else {
        if (!existsSync(filePath)) {
          return {
            success: false,
            content: "",
            error: `File not found: ${file_path}`,
          };
        }
        content = await fsReadFile(filePath, "utf-8");
      }

      // Determine if fuzzy matching is allowed
      // Fuzzy matching (Unicode normalization) is only allowed in safe mode
      // In batch mode, we enforce exact matching for safety
      const isSafeMode = mode !== "batch"; // Default to safe mode
      const allowFuzzyMatch = isSafeMode;

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
          : `String not found in file: "${old_string.substring(0, 100)}${old_string.length > 100 ? "..." : ""}"\n\nNote: Fuzzy matching is disabled in batch mode. The string must match exactly.`;

        return {
          success: false,
          content: "",
          error: errorMsg,
        };
      }

      // Count occurrences for uniqueness check
      const occurrences = content.split(old_string).length - 1;

      // Check uniqueness (only in safe mode)
      if (isSafeMode && occurrences > 1) {
        return {
          success: false,
          content: "",
          error: `Found ${occurrences} occurrences of old_string. The string must be unique in the file.\n\nTo replace all occurrences, use mode="batch".\nTo replace a specific occurrence, make old_string more unique by including surrounding context and keep mode="safe".`,
        };
      }

      let newContent: string;
      let replacementCount: number;

      if (!isSafeMode) {
        // Batch mode: Replace all occurrences with exact matching
        const parts = content.split(old_string);
        replacementCount = parts.length - 1;
        newContent = parts.join(new_string);
      } else {
        // Safe mode: Replace only the matched occurrence (uses matchIndex which may be from fuzzy match)
        newContent =
          content.substring(0, matchIndex) + new_string + content.substring(matchIndex + old_string.length);
        replacementCount = 1;
      }

      if (config.vfs) {
        const vfsPath = filePath.replace(/\\/g, "/");
        await config.vfs.writeFile(vfsPath, Buffer.from(newContent, "utf-8"));
      } else {
        await fsWriteFile(filePath, newContent, "utf-8");
      }

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
