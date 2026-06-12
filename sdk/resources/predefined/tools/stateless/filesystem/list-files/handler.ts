/**
 * The logic executed by the list_files tool
 *
 * Returns structured results with both machine-readable `entries` array
 * and human-readable `display` string (for LLM consumption).
 *
 * IgnoreController is used by default to exclude build artifacts and
 * hidden directories. LLM can override at call time via includeIgnored=true.
 */

import path from "path";
import type { ToolOutput } from "@wf-agent/types";
import type { ListFilesConfig } from "../../../types.js";
import { IgnoreController } from "@wf-agent/sdk/services";
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
 * List files recursively with ignore filtering
 */
async function listFilesRecursive(
  dirPath: string,
  basePath: string,
  vfs: HostFSAdapter,
  maxResults: number,
  ignoreController?: IgnoreController,
  resultCount: { count: number } = { count: 0 },
): Promise<FileEntry[]> {
  const entries: FileEntry[] = [];

  if (resultCount.count >= maxResults) {
    return entries;
  }

  const names = (await vfs.readdir(dirPath)) ?? [];

  for (const name of names) {
    if (resultCount.count >= maxResults) {
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

      if (shouldInclude) {
        entries.push({ name, type: "directory", path: relativePath });
        resultCount.count++;

        const subEntries = await listFilesRecursive(
          itemPath,
          relativePath,
          vfs,
          maxResults,
          ignoreController,
          resultCount,
        );
        entries.push(...subEntries);
      }
    } else if (entryStat.type === "file") {
      const shouldInclude = ignoreController ? ignoreController.validateAccess(itemPath) : true;

      if (shouldInclude) {
        entries.push({ name, type: "file", path: relativePath });
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
  vfs: HostFSAdapter,
  maxResults: number,
  ignoreController?: IgnoreController,
  resultCount: { count: number } = { count: 0 },
): Promise<FileEntry[]> {
  const entries: FileEntry[] = [];
  const names = (await vfs.readdir(dirPath)) ?? [];

  for (const name of names) {
    if (resultCount.count >= maxResults) {
      break;
    }

    const relativePath = basePath === "." ? name : `${basePath}/${name}`;
    const itemPath = `${dirPath}/${name}`;

    const entryStat = await vfs.stat(itemPath);
    if (!entryStat) continue;

    if (entryStat.type === "directory") {
      const shouldInclude = ignoreController
        ? ignoreController.shouldIncludeDirectory(name, itemPath)
        : true;

      if (shouldInclude) {
        entries.push({ name, type: "directory", path: relativePath });
        resultCount.count++;
      }
    } else if (entryStat.type === "file") {
      const shouldInclude = ignoreController ? ignoreController.validateAccess(itemPath) : true;

      if (shouldInclude) {
        entries.push({ name, type: "file", path: relativePath });
        resultCount.count++;
      }
    }
  }

  return entries;
}

/**
 * Create the `list_files` tool execution function
 *
 * Returns a structured ToolOutput where `content` is an object:
 * ```ts
 * {
 *   entries: FileEntry[],       // machine-readable matched entries
 *   display: string,            // human-readable formatted output for LLM
 *   total: number,              // total matching entries (before truncation)
 *   truncated: boolean          // true if maxResults was reached
 * }
 * ```
 */
export function createListFilesHandler(config: ListFilesConfig = {}) {
  const maxResults = config.maxResults ?? 1000;
  const enableIgnore = config.enableIgnore ?? true;

  return async (params: Record<string, unknown>): Promise<ToolOutput> => {
    try {
      const {
        path: targetPath,
        recursive,
        includeIgnored,
      } = params as {
        path: string;
        recursive?: boolean;
        includeIgnored?: boolean;
      };
      const dirPath = resolveFilePath(targetPath, config.workspaceDir);

      if (isSpecialDirectory(dirPath)) {
        return {
          success: false,
          content: "",
          error: "Access to root or home directory is not allowed for security reasons",
        };
      }

      const vfs = new HostFSAdapter();
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

      // IgnoreController: used by default. LLM can bypass via includeIgnored=true.
      const useIgnore = includeIgnored !== true && enableIgnore;
      let ignoreController: IgnoreController | undefined;
      if (useIgnore) {
        const workspaceDir = config.workspaceDir ?? process.cwd();
        ignoreController = new IgnoreController({ cwd: workspaceDir, mode: "all" });
        await ignoreController.initialize();
      }

      const resultCount = { count: 0 };
      const entries = recursive
        ? await listFilesRecursive(
            dirPath,
            targetPath,
            vfs,
            maxResults,
            ignoreController,
            resultCount,
          )
        : await listFilesFlat(dirPath, targetPath, vfs, maxResults, ignoreController, resultCount);

      entries.sort((a, b) => {
        if (a.type !== b.type) {
          return a.type === "directory" ? -1 : 1;
        }
        return a.path.localeCompare(b.path);
      });

      const dirCount = entries.filter(e => e.type === "directory").length;
      const fileCount = entries.filter(e => e.type === "file").length;
      const truncated = resultCount.count >= maxResults;

      let display: string;
      if (entries.length > 0) {
        const lines = entries.map(e => {
          const prefix = e.type === "directory" ? "[DIR]" : "[FILE]";
          return `${prefix} ${e.path}`;
        });
        display = `${lines.join("\n")}\n\nSummary: ${dirCount} directories, ${fileCount} files`;
        if (truncated) {
          display += `\n(Result limit of ${maxResults} reached. Target a deeper subdirectory or use a more specific path.)`;
        }
      } else {
        display = "Empty directory";
      }

      return {
        success: true,
        content: {
          entries,
          display,
          total: entries.length,
          truncated,
        },
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
