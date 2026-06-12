/**
 * The logic executed by the glob tool
 *
 * Recursive matching is controlled by the glob pattern itself (via double-asterisk),
 * not by a separate boolean parameter. The tool always walks the directory
 * tree and lets minimatch determine which entries match the pattern.
 *
 * Results are returned as a structured object with both a machine-readable
 * `entries` array and a human-readable `display` string for the LLM.
 *
 * IgnoreController is used by default to exclude build artifacts and
 * hidden directories. LLM can override at call time via includeIgnored=true.
 */

import path from "path";
import { minimatch } from "minimatch";
import type { ToolOutput } from "@wf-agent/types";
import type { GlobConfig } from "../../../types.js";
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

/**
 * A matched file or directory entry
 */
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
 * Walk the directory tree recursively, collecting entries that match
 * the glob pattern. Stops early when maxResults is reached.
 */
async function globWalk(
  dirPath: string,
  basePath: string,
  pattern: string,
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

      if (matchesGlob(relativePath, pattern)) {
        entries.push({ name, type: "directory", path: relativePath });
        resultCount.count++;
      }

      if (shouldInclude) {
        const subEntries = await globWalk(
          itemPath,
          relativePath,
          pattern,
          vfs,
          maxResults,
          ignoreController,
          resultCount,
        );
        entries.push(...subEntries);
      }
    } else if (entryStat.type === "file") {
      const shouldInclude = ignoreController ? ignoreController.validateAccess(itemPath) : true;

      if (shouldInclude && matchesGlob(relativePath, pattern)) {
        entries.push({ name, type: "file", path: relativePath });
        resultCount.count++;
      }
    }
  }

  return entries;
}

/**
 * Create the `glob` tool execution function
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
export function createGlobHandler(config: GlobConfig = {}) {
  const maxResults = config.maxResults ?? 50;
  const enableIgnore = config.enableIgnore ?? true;

  return async (params: Record<string, unknown>): Promise<ToolOutput> => {
    try {
      const {
        path: targetPath,
        pattern,
        includeIgnored,
      } = params as {
        path: string;
        pattern: string;
        includeIgnored?: boolean;
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
      const entries = await globWalk(
        dirPath,
        targetPath,
        pattern,
        vfs,
        maxResults,
        ignoreController,
        resultCount,
      );

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
        display = `${lines.join("\n")}\n\nSummary: ${dirCount} directories, ${fileCount} files, pattern: ${pattern}`;
        if (truncated) {
          display += `\n(Result limit of ${maxResults} reached. Use a more specific pattern to narrow results.)`;
        }
      } else {
        display = `No matches found for pattern: ${pattern}`;
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
