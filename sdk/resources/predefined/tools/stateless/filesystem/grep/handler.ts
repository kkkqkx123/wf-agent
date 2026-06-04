/**
 * The logic executed by the grep tool
 */

import type { ToolOutput } from "@wf-agent/types";
import type { ReadFileConfig } from "../../../types.js";
import { SearchService, IgnoreController } from "@wf-agent/sdk/services";
import { resolveFilePath } from "@wf-agent/sdk/utils";
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

      // Check directory existence and type via VFS
      const vfs = config.vfs ?? new HostFSAdapter();
      const dirStat = await vfs.stat(dirPath);
      if (!dirStat) {
        return {
          success: false,
          content: "",
          error: `Directory not found: ${path}`,
        };
      }
      if (dirStat.type !== "directory") {
        return {
          success: false,
          content: "",
          error: `Not a directory: ${path}`,
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

      // Initialize search service
      const searchService = new SearchService();

      try {
        await searchService.initialize();
      } catch {
        // If ripgrep is not available, fall back to a helpful error message
        return {
          success: false,
          content: "",
          error: `ripgrep is not available. Please install ripgrep: https://github.com/BurntSushi/ripgrep#installation`,
        };
      }

      // Initialize ignore controller if enabled
      const ignoreController = config.enableIgnore
        ? new IgnoreController({ cwd: workspaceDir })
        : undefined;

      // Perform search using search service
      const result = await searchService.searchContent({
        cwd: workspaceDir,
        directoryPath: dirPath,
        pattern: regex,
        filePattern: file_pattern ?? undefined,
        contextLines: 1,
        maxResults: 300,
      });

      // If ignore controller is enabled, we need to filter results
      // Note: ripgrep already respects .gitignore, but we may have additional ignore patterns
      if (ignoreController && result !== "No results found") {
        // For now, we return the result as-is since ripgrep handles most ignore patterns
        // Additional filtering can be added here if needed
      }

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
