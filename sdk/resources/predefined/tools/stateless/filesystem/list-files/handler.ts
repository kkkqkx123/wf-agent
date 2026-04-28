/**
 * The logic executed by the list_files tool
 */

import { readdir, stat } from "fs/promises";
import { existsSync } from "fs";
import path from "path";
import type { ToolOutput } from "@wf-agent/types";
import type { ReadFileConfig } from "../../../types.js";
import { IgnoreController, IgnoreMode, MAX_FILE_RESULTS } from "@wf-agent/sdk/services";

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
  ignoreController?: IgnoreController,
  resultCount: { count: number } = { count: 0 },
): Promise<FileEntry[]> {
  const entries: FileEntry[] = [];

  // Check if we've reached the limit
  if (resultCount.count >= MAX_FILE_RESULTS) {
    return entries;
  }

  const items = await readdir(dirPath, { withFileTypes: true });

  for (const item of items) {
    // Check limit before processing each item
    if (resultCount.count >= MAX_FILE_RESULTS) {
      break;
    }

    const itemPath = `${dirPath}/${item.name}`;
    const relativePath = basePath === "." ? item.name : `${basePath}/${item.name}`;

    // Skip symbolic links to prevent circular traversal
    if (item.isSymbolicLink()) {
      continue;
    }

    if (item.isDirectory()) {
      // Check if directory should be included
      const shouldInclude = ignoreController
        ? ignoreController.shouldIncludeDirectory(item.name, itemPath)
        : true;

      if (shouldInclude) {
        entries.push({
          name: item.name,
          type: "directory",
          path: relativePath,
        });
        resultCount.count++;

        // Recurse into subdirectory
        const subEntries = await listFilesRecursive(
          itemPath,
          relativePath,
          ignoreController,
          resultCount,
        );
        entries.push(...subEntries);
      }
    } else if (item.isFile()) {
      // Check if file should be included
      const shouldInclude = ignoreController ? ignoreController.validateAccess(itemPath) : true;

      if (shouldInclude) {
        entries.push({
          name: item.name,
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
  ignoreController?: IgnoreController,
): Promise<FileEntry[]> {
  const entries: FileEntry[] = [];
  const items = await readdir(dirPath, { withFileTypes: true });

  for (const item of items) {
    const relativePath = basePath === "." ? item.name : `${basePath}/${item.name}`;
    const itemPath = `${dirPath}/${item.name}`;

    // Skip symbolic links
    if (item.isSymbolicLink()) {
      continue;
    }

    if (item.isDirectory()) {
      // Check if directory should be included
      const shouldInclude = ignoreController
        ? ignoreController.shouldIncludeDirectory(item.name, itemPath)
        : true;

      if (shouldInclude) {
        entries.push({
          name: item.name,
          type: "directory",
          path: relativePath,
        });
      }
    } else if (item.isFile()) {
      // Check if file should be included
      const shouldInclude = ignoreController ? ignoreController.validateAccess(itemPath) : true;

      if (shouldInclude) {
        entries.push({
          name: item.name,
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
      const dirPath = resolvePath(targetPath, config.workspaceDir);

      // Check for special directories
      if (isSpecialDirectory(dirPath)) {
        return {
          success: false,
          content: "",
          error: "Access to root or home directory is not allowed for security reasons",
        };
      }

      if (!existsSync(dirPath)) {
        return {
          success: false,
          content: "",
          error: `Directory not found: ${targetPath}`,
        };
      }

      const dirStat = await stat(dirPath);
      if (!dirStat.isDirectory()) {
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
          mode: IgnoreMode.All,
        });
        await ignoreController.initialize();
      }

      // List files
      const resultCount = { count: 0 };
      const entries = recursive
        ? await listFilesRecursive(dirPath, targetPath, ignoreController, resultCount)
        : await listFilesFlat(dirPath, targetPath, ignoreController);

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
