/**
 * The logic executed by the read_file tool
 */

import { readFile } from "fs/promises";
import { existsSync } from "fs";
import type { ToolOutput } from "@wf-agent/types";
import { formatLineNumbers, truncateText } from "@wf-agent/sdk";
import type { ReadFileConfig } from "../../../types.js";
import { IgnoreController, IgnoreMode, ProtectController } from "@wf-agent/sdk/services";

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
 * Create the `read_file` tool execution function
 */
export function createReadFileHandler(config: ReadFileConfig = {}) {
  const maxFileSize = config.maxFileSize ?? 500000;

  return async (params: Record<string, unknown>): Promise<ToolOutput> => {
    try {
      const {
        path: filePath,
        offset,
        limit,
      } = params as { path: string; offset?: number; limit?: number };
      const absolutePath = resolvePath(filePath, config.workspaceDir);
      const workspaceDir = config.workspaceDir ?? process.cwd();

      // Initialize controllers if enabled
      let ignoreController: IgnoreController | undefined;
      let protectController: ProtectController | undefined;

      if (config.enableIgnore) {
        ignoreController = new IgnoreController({
          cwd: workspaceDir,
          mode: IgnoreMode.All,
        });
        await ignoreController.initialize();
      }

      if (config.enableProtect) {
        protectController = new ProtectController({ cwd: workspaceDir });
      }

      // Check if file is write-protected (informational only for read operations)
      const isProtected = protectController?.isWriteProtected(filePath) ?? false;

      // Check if file is accessible (ignore filtering)
      if (ignoreController && !ignoreController.validateAccess(absolutePath)) {
        return {
          success: false,
          content: "",
          error: `Access denied: ${filePath} is in ignore list`,
        };
      }

      if (!existsSync(absolutePath)) {
        return {
          success: false,
          content: "",
          error: `File not found: ${filePath}`,
        };
      }

      // Read the contents of the file.
      const content = await readFile(absolutePath, "utf-8");
      const lines = content.split("\n");

      // Apply offsets and restrictions
      const start = offset ? Math.max(0, offset - 1) : 0;
      const end = limit ? Math.min(lines.length, start + limit) : lines.length;
      const selectedLines = lines.slice(start, end);

      // Formatted with line numbers
      const numberedContent = formatLineNumbers(selectedLines, start + 1);

      // App truncation
      const truncatedContent = truncateText(numberedContent, maxFileSize);

      // Add protection notice if applicable
      let finalContent = truncatedContent;
      if (isProtected) {
        const protectionNotice = `\n\n[This file is write-protected and requires approval for modifications]`;
        finalContent = truncatedContent + protectionNotice;
      }

      return {
        success: true,
        content: finalContent,
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
