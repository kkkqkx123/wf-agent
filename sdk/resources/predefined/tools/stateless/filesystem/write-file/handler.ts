/**
 * The `write_file` tool executes the following logic:
 */

import { dirname } from "path";
import type { ToolOutput } from "@wf-agent/types";
import type { WriteFileConfig } from "../../../types.js";
import { ProtectController, SHIELD_SYMBOL } from "@wf-agent/sdk/services";
import { resolveFilePath } from "@wf-agent/sdk/utils";
import { HostFSAdapter } from "../../../utils/host-fs-adapter.js";

/**
 * Create the `write_file` tool execution function
 */
export function createWriteFileHandler(config: WriteFileConfig = {}) {
  return async (params: Record<string, unknown>): Promise<ToolOutput> => {
    try {
      const { path: filePath, content } = params as { path: string; content: string };
      const absolutePath = resolveFilePath(filePath, config.workspaceDir);
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

      const data = Buffer.from(content, "utf-8");
      const vfs = config.vfs ?? new HostFSAdapter();
      const vfsPath = absolutePath.replace(/\\/g, "/");

      // Route through VFS (Plan A: VFS as unified middle layer)
      // VFS handles sync-to-host if configured internally
      await vfs.mkdir(dirname(vfsPath).replace(/\\/g, "/"));
      await vfs.writeFile(vfsPath, data);

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
