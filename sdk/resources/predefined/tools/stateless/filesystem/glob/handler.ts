/**
 * The logic executed by the glob tool
 */

import path from "path";
import { minimatch } from "minimatch";
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

  const root = process.platform === "win32" ? path.parse(absolutePath).root : "/";
  if (absolutePath === root) {
    return true;
  }

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
 * Test if a relative path matches the given glob pattern
 */
function matchesGlob(relativePath: string, pattern: string): boolean {
  const normalized = relativePath.replace(/\\/g, "/");
  return minimatch(normalized, pattern, { dot: true });
}

/**
 * Glob walk recursively with ignore filtering
 */
async function globRecursive(
  dirPath: string,
  basePath: string,
  pattern: string,
  vfs: VFSFileIO,
  ignoreController?: IgnoreController,
  resultCount: { count: number } = { count: 0 },
): Promise<FileEntry[]> {
  const entries: FileEntry[] = [];

  if (resultCount.count >= MAX_FILE_RESULTS) {
    return entries;
  }

  const names = (await vfs.readdir(dirPath)) ?? [];

  for (const name of names) {
    if (resultCount.count >= MAX_FILE_RESULTS) {
      break;
    }

    const itemPath = `${dirPath}/${name}`;
    const relativePath = basePath === "." ? name : `${basePath}/${name}`;

    const entryStat = await vfs.stat(itemPath);
    if (!entryStat) continue;

    if (entryStat.type === "directory") {
      const shouldInclude = ignoreController
        ? ignoreController.shouldIncludeDirectory(name, itemPath)
        : true;

      if (matchesGlob(relativePath, pattern)) {
        entries.push({
          name,
          type: "directory",
          path: relativePath,
        });
        resultCount.count++;
      }

      if (shouldInclude) {
        const subEntries = await globRecursive(
          itemPath,
          relativePath,
          pattern,
          vfs,
          ignoreController,
          resultCount,
        );
        entries.push(...subEntries);
      }
    } else if (entryStat.type === "file") {
      const shouldInclude = ignoreController
        ? ignoreController.validateAccess(itemPath)
        : true;

      if (shouldInclude && matchesGlob(relativePath, pattern)) {
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
 * Glob walk non-recursively (top-level only)
 */
async function globFlat(
  dirPath: string,
  basePath: string,
  pattern: string,
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

    if (!matchesGlob(relativePath, pattern)) continue;

    if (entryStat.type === "directory") {
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
      const shouldInclude = ignoreController
        ? ignoreController.validateAccess(itemPath)
        : true;

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
 * Create the `glob` tool execution function
 */
export function createGlobHandler(config: ReadFileConfig = {}) {
  return async (params: Record<string, unknown>): Promise<ToolOutput> => {
    try {
      const { path: targetPath, pattern, recursive } = params as {
        path: string;
        pattern: string;
        recursive?: boolean;
      };

      if (!pattern || typeof pattern !== "string") {
        return {
          success: false,
          content: "",
          error: "Missing or invalid 'pattern' parameter",
        };
      }

      const dirPath = resolveFilePath(targetPath, config.workspaceDir);

      if (isSpecialDirectory(dirPath)) {
        return {
          success: false,
          content: "",
          error: "Access to root or home directory is not allowed for security reasons",
        };
      }

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

      let ignoreController: IgnoreController | undefined;
      if (config.enableIgnore) {
        const workspaceDir = config.workspaceDir ?? process.cwd();
        ignoreController = new IgnoreController({
          cwd: workspaceDir,
          mode: "all",
        });
        await ignoreController.initialize();
      }

      const isRecursive = recursive !== false;
      const resultCount = { count: 0 };
      const entries = isRecursive
        ? await globRecursive(dirPath, targetPath, pattern, vfs, ignoreController, resultCount)
        : await globFlat(dirPath, targetPath, pattern, vfs, ignoreController);

      entries.sort((a, b) => {
        if (a.type !== b.type) {
          return a.type === "directory" ? -1 : 1;
        }
        return a.path.localeCompare(b.path);
      });

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
        content = `${lines.join("\n")}\n\nSummary: ${summary.directories} directories, ${summary.files} files, pattern: ${pattern}`;
        if (summary.limitReached) {
          content += `\n(Result limit of ${MAX_FILE_RESULTS} reached)`;
        }
      } else {
        content = `No matches found for pattern: ${pattern}`;
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
