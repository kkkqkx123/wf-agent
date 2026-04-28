/**
 * The `write_file` tool executes the following logic:
 */

import { writeFile, mkdir } from "fs/promises";
import { dirname } from "path";
import type { ToolOutput } from "@wf-agent/types";
import type { WriteFileConfig } from "../../../types.js";
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
 * Create the `write_file` tool execution function
 */
export function createWriteFileHandler(config: WriteFileConfig = {}) {
  return async (params: Record<string, unknown>): Promise<ToolOutput> => {
    try {
      const { path: filePath, content } = params as { path: string; content: string };
      const absolutePath = resolvePath(filePath, config.workspaceDir);
      const workspaceDir = config.workspaceDir ?? process.cwd();

      // Initialize protect controller if enabled
      let protectController: ProtectController | undefined;
      if (config.enableProtect) {
        protectController = new ProtectController({ cwd: workspaceDir });
      }

      // Check if file is write-protected
      const isProtected = protectController?.isWriteProtected(filePath) ?? false;
      if (isProtected) {
        return {
          success: false,
          content: "",
          error: `${SHIELD_SYMBOL} Write operation blocked: ${filePath} is a protected file and requires explicit approval for modifications`,
        };
      }

      // Create a parent directory
      const dir = dirname(absolutePath);
      await mkdir(dir, { recursive: true });

      // Write to the file
      await writeFile(absolutePath, content, "utf-8");

      return {
        success: true,
        content: `Successfully wrote to ${filePath}`,
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
