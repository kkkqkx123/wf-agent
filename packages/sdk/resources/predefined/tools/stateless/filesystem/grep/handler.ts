/**
 * The logic executed by the grep tool
 */

import type { ToolOutput } from "@wf-agent/types";
import type { ReadFileConfig } from "../../../types.js";
import { GrepSearchEngine } from "./utils/search-engine.js";
import { resolveFilePath } from "@wf-agent/sdk/utils";
import { FilesystemToolUtils } from "../utils/filesystem-tool-utils.js";
import { HostFSAdapter } from "../../../utils/host-fs-adapter.js";

/**
 * Create the `grep` tool execution function
 */
export function createGrepHandler(config: ReadFileConfig = {}) {
  return async (params: Record<string, unknown>): Promise<ToolOutput> => {
    try {
      const { path, regex, file_pattern } = params as {
        path: string;
        regex: string;
        file_pattern?: string | null;
      };

      const workspaceDir = config.workspaceDir ?? process.cwd();
      const dirPath = resolveFilePath(path, workspaceDir);

      // Initialize VFS
      const vfs = config.vfs ?? new HostFSAdapter();

      // Validate directory using FilesystemToolUtils
      const validation = await FilesystemToolUtils.validateDirectory(dirPath, vfs);
      if (!validation.valid) {
        return {
          success: false,
          content: "",
          error: validation.error,
        };
      }

      // Validate regex pattern
      try {
        new RegExp(regex, "g");
      } catch {
        return {
          success: false,
          content: "",
          error: `Invalid regex pattern: ${regex}`,
        };
      }

      // Initialize grep search engine
      try {
        await GrepSearchEngine.initialize();
      } catch {
        // If ripgrep is not available, fall back to a helpful error message
        return {
          success: false,
          content: "",
          error: `ripgrep is not available. Please install ripgrep: https://github.com/BurntSushi/ripgrep#installation`,
        };
      }

      // Perform search using grep search engine
      const result = await GrepSearchEngine.searchContent({
        cwd: workspaceDir,
        directoryPath: dirPath,
        pattern: regex,
        filePattern: file_pattern ?? undefined,
        contextLines: 1,
        maxResults: 300,
      });

      if (result === "No results found") {
        return {
          success: true,
          content: "No matches found",
        };
      }

      return {
        success: true,
        content: result,
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
