/**
 * The logic executed by the list_files tool
 */

import path from "path";
import type { ToolOutput } from "@wf-agent/types";
import type { ReadFileConfig, VFSFileIO } from "../../../types.js";
import { IgnoreController, MAX_FILE_RESULTS } from "@wf-agent/sdk/services";
import { resolveFilePath } from "@wf-agent/sdk/utils";
import { HostFSAdapter } from "../../../utils/host-fs-adapter.js";

/**
 * Check if a path is a special directory (root or home)
 */
function isSpecialDirectory(dirPath: string): boolean {
  const absolutePath = path.resolve(dirPath);

  // Check for root directory
  const root = process.platform === "win32" ? path.parse(absolutePath).root : "/";
  if (absolutePath === root) {
    return true;
  }

  // Check for home directory
  const homeDir = process.env["HOME"] || process.env["USERPROFILE"];
  if (homeDir && absolutePath === homeDir) {
    return true;
  }

  return false;
}

interface FileEntry {
  name: string;
  type: "file" | "directory";
  path: string;
}

/**
 * List files recursively with ignore filtering
 */
async function listFilesRecursive(
  dirPath: string,
  basePath: string,
  vfs: VFSFileIO,
  ignoreController?: IgnoreController,
  resultCount: { count: number } = { count: 0 },
): Promise<FileEntry[]> {
  const entries: FileEntry[] = [];

  // Check if we've reached the limit
  if (resultCount.count >= MAX_FILE_RESULTS) {
    return entries;
  }

  const names = (await vfs.readdir(dirPath)) ?? [];

  for (const name of names) {
    // Check limit before processing each item
    if (resultCount.count >= MAX_FILE_RESULTS) {
      break;
    }

    const itemPath = `${dirPath}/${name}`;
    const relativePath = basePath === "." ? name : `${basePath}/${name}`;

    const entryStat = await vfs.stat(itemPath);
    if (!entryStat) continue;

    if (entryStat.type === "directory") {
      // Check if directory should be included
      const shouldInclude = ignoreController
        ? ignoreController.shouldIncludeDirectory(name, itemPath)
        : true;

      if (shouldInclude) {
        entries.push({
          name,
          type: "directory",
          path: relativePath,
        });
        resultCount.count++;

        // Recurse into subdirectory
        const subEntries = await listFilesRecursive(
          itemPath,
          relativePath,
          vfs,
          ignoreController,
          resultCount,
        );
        entries.push(...subEntries);
      }
    } else if (entryStat.type === "file") {
      // Check if file should be included
      const shouldInclude = ignoreController ? ignoreController.validateAccess(itemPath) : true;

      if (shouldInclude) {
        entries.push({
          name,
          type: "file",
          path: relativePath,
        });
        resultCount.count++;
      }
    }
  }

  return entries;
}

/**
 * List files non-recursively with ignore filtering
 */
async function listFilesFlat(
  dirPath: string,
  basePath: string,
  vfs: VFSFileIO,
  ignoreController?: IgnoreController,
): Promise<FileEntry[]> {
  const entries: FileEntry[] = [];
  const names = (await vfs.readdir(dirPath)) ?? [];

  for (const name of names) {
    const relativePath = basePath === "." ? name : `${basePath}/${name}`;
    const itemPath = `${dirPath}/${name}`;

    const entryStat = await vfs.stat(itemPath);
    if (!entryStat) continue;

    if (entryStat.type === "directory") {
      // Check if directory should be included
      const shouldInclude = ignoreController
        ? ignoreController.shouldIncludeDirectory(name, itemPath)
        : true;

      if (shouldInclude) {
        entries.push({
          name,
          type: "directory",
          path: relativePath,
        });
      }
    } else if (entryStat.type === "file") {
      // Check if file should be included
      const shouldInclude = ignoreController ? ignoreController.validateAccess(itemPath) : true;

      if (shouldInclude) {
        entries.push({
          name,
          type: "file",
          path: relativePath,
        });
      }
    }
  }

  return entries;
}

/**
 * Create the `list_files` tool execution function
 */
export function createListFilesHandler(config: ReadFileConfig = {}) {
  return async (params: Record<string, unknown>): Promise<ToolOutput> => {
    try {
      const { path: targetPath, recursive } = params as { path: string; recursive?: boolean };
      const dirPath = resolveFilePath(targetPath, config.workspaceDir);

      // Check for special directories
      if (isSpecialDirectory(dirPath)) {
        return {
          success: false,
          content: "",
          error: "Access to root or home directory is not allowed for security reasons",
        };
      }

      // Check directory existence and type via VFS
      const vfs = config.vfs ?? new HostFSAdapter();
      const dirStat = await vfs.stat(dirPath);
      if (!dirStat) {
        return {
          success: false,
          content: "",
          error: `Directory not found: ${targetPath}`,
        };
      }
      if (dirStat.type !== "directory") {
        return {
          success: false,
          content: "",
          error: `Not a directory: ${targetPath}`,
        };
      }

      // Initialize ignore controller if enabled
      let ignoreController: IgnoreController | undefined;
      if (config.enableIgnore) {
        const workspaceDir = config.workspaceDir ?? process.cwd();
        ignoreController = new IgnoreController({
          cwd: workspaceDir,
          mode: "all",
        });
        await ignoreController.initialize();
      }

      // List files
      const resultCount = { count: 0 };
      const entries = recursive
        ? await listFilesRecursive(dirPath, targetPath, vfs, ignoreController, resultCount)
        : await listFilesFlat(dirPath, targetPath, vfs, ignoreController);

      // Sort entries: directories first, then files, alphabetically within each group
      entries.sort((a, b) => {
        if (a.type !== b.type) {
          return a.type === "directory" ? -1 : 1;
        }
        return a.path.localeCompare(b.path);
      });

      // Format output
      const lines = entries.map(entry => {
        const prefix = entry.type === "directory" ? "[DIR]" : "[FILE]";
        return `${prefix} ${entry.path}`;
      });

      const summary = {
        directories: entries.filter(e => e.type === "directory").length,
        files: entries.filter(e => e.type === "file").length,
        limitReached: resultCount.count >= MAX_FILE_RESULTS,
      };

      let content = "";
      if (lines.length > 0) {
        content = `${lines.join("\n")}\n\nSummary: ${summary.directories} directories, ${summary.files} files`;
        if (summary.limitReached) {
          content += `\n(Result limit of ${MAX_FILE_RESULTS} reached)`;
        }
      } else {
        content = "Empty directory";
      }

      return {
        success: true,
        content,
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
