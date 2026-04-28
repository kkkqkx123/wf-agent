/**
 * The logic executed by the edit tool
 */

import { readFile, writeFile } from "fs/promises";
import { existsSync } from "fs";
import type { ToolOutput } from "@wf-agent/types";
import type { EditFileConfig } from "../../../types.js";
import { ProtectController, SHIELD_SYMBOL } from "@wf-agent/sdk/services";

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

      // Check if old_string exists in the file
      if (!content.includes(old_string)) {
        return {
          success: false,
          content: "",
          error: `String not found in file: "${old_string.substring(0, 100)}${old_string.length > 100 ? "..." : ""}"`,
        };
      }

      // Check uniqueness if required
      if (require_unique) {
        const occurrences = content.split(old_string).length - 1;
        if (occurrences > 1) {
          return {
            success: false,
            content: "",
            error: `Found ${occurrences} occurrences of old_string. The string must be unique in the file when require_unique is true.`,
          };
        }
      }

      let newContent: string;
      let replacementCount: number;

      if (replace_all) {
        // Replace all occurrences
        const parts = content.split(old_string);
        replacementCount = parts.length - 1;
        newContent = parts.join(new_string);
      } else {
        // Replace only the first occurrence
        const index = content.indexOf(old_string);
        if (index === -1) {
          return {
            success: false,
            content: "",
            error: `String not found in file`,
          };
        }
        newContent =
          content.substring(0, index) + new_string + content.substring(index + old_string.length);
        replacementCount = 1;
      }

      await writeFile(filePath, newContent, "utf-8");

      return {
        success: true,
        content: `Edited ${file_path}: replaced ${replacementCount} occurrence(s)`,
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
