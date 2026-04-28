/**
 * The logic executed by the apply_diff tool
 */

import { readFile, writeFile } from "fs/promises";
import { existsSync } from "fs";
import type { ToolOutput } from "@wf-agent/types";
import type { ReadFileConfig } from "../../../types.js";
import { ProtectController, SHIELD_SYMBOL } from "@wf-agent/sdk/services";

/**
 * Parse paths (relative paths are supported)
 */
function resolvePath(path: string, workspaceDir?: string): string {
  if (path.startsWith("/") || path.match(/^[A-Za-z]:\\/)) {
    return path;
  }
  const baseDir = workspaceDir ?? process.cwd();
  return `${baseDir}/${path}`.replace(/\\/g, "/");
}

/**
 * Parse a unified diff and extract hunks
 */
interface DiffHunk {
  oldStart: number;
  oldCount: number;
  newStart: number;
  newCount: number;
  lines: string[];
}

function parseUnifiedDiff(diff: string): DiffHunk[] {
  const lines = diff.split("\n");
  const hunks: DiffHunk[] = [];
  let currentHunk: DiffHunk | null = null;

  for (const line of lines) {
    // Skip --- and +++ header lines
    if (line.startsWith("---") || line.startsWith("+++")) {
      continue;
    }

    // Match hunk header: @@ -oldStart,oldCount +newStart,newCount @@
    const hunkMatch = line.match(/^@@ -(\d+)(?:,(\d+))? \+(\d+)(?:,(\d+))? @@/);
    if (hunkMatch) {
      if (currentHunk) {
        hunks.push(currentHunk);
      }
      currentHunk = {
        oldStart: parseInt(hunkMatch[1] as string, 10),
        oldCount: hunkMatch[2] ? parseInt(hunkMatch[2] as string, 10) : 1,
        newStart: parseInt(hunkMatch[3] as string, 10),
        newCount: hunkMatch[4] ? parseInt(hunkMatch[4] as string, 10) : 1,
        lines: [],
      };
      continue;
    }

    // Collect hunk content lines
    if (
      currentHunk &&
      (line.startsWith(" ") || line.startsWith("-") || line.startsWith("+") || line === "")
    ) {
      currentHunk.lines.push(line);
    }
  }

  if (currentHunk) {
    hunks.push(currentHunk);
  }

  return hunks;
}

/**
 * Apply hunks to file content
 */
function applyHunks(content: string, hunks: DiffHunk[]): string {
  const lines = content.split("\n");

  // Remove trailing empty line if present (from final newline)
  if (lines.length > 0 && lines[lines.length - 1] === "") {
    lines.pop();
  }

  // Process hunks in reverse order to preserve line numbers
  const sortedHunks = [...hunks].sort((a, b) => b.oldStart - a.oldStart);

  for (const hunk of sortedHunks) {
    const newLines: string[] = [];
    let oldLineIndex = hunk.oldStart - 1; // Convert to 0-indexed

    for (const line of hunk.lines) {
      if (line.startsWith("-")) {
        // Removed line - skip in output, but verify it matches
        const expectedLine = line.substring(1);
        const actualLine = lines[oldLineIndex] ?? "";
        if (actualLine !== expectedLine) {
          throw new Error(
            `Line mismatch at line ${oldLineIndex + 1}: expected "${expectedLine}", got "${actualLine}"`,
          );
        }
        oldLineIndex++;
      } else if (line.startsWith("+")) {
        // Added line
        newLines.push(line.substring(1));
      } else {
        // Context line (starts with space or is empty)
        const contextLine = line.startsWith(" ") ? line.substring(1) : "";
        const actualLine = lines[oldLineIndex] ?? "";
        if (actualLine !== contextLine) {
          throw new Error(
            `Context mismatch at line ${oldLineIndex + 1}: expected "${contextLine}", got "${actualLine}"`,
          );
        }
        newLines.push(contextLine);
        oldLineIndex++;
      }
    }

    // Replace the old lines with new lines
    const replaceStart = hunk.oldStart - 1;
    const replaceCount = hunk.oldCount;
    lines.splice(replaceStart, replaceCount, ...newLines);
  }

  // Ensure file ends with newline
  if (lines.length === 0 || lines[lines.length - 1] !== "") {
    lines.push("");
  }

  return lines.join("\n");
}

/**
 * Create the `apply_diff` tool execution function
 */
export function createApplyDiffHandler(config: ReadFileConfig = {}) {
  return async (params: Record<string, unknown>): Promise<ToolOutput> => {
    try {
      const { path, diff } = params as { path: string; diff: string };
      const filePath = resolvePath(path, config.workspaceDir);

      // Initialize protect controller if enabled
      const workspaceDir = config.workspaceDir ?? process.cwd();
      const protectController = config.enableProtect
        ? new ProtectController({ cwd: workspaceDir })
        : undefined;

      // Check if file is write-protected
      if (protectController?.isWriteProtected(filePath)) {
        return {
          success: false,
          content: "",
          error: `${SHIELD_SYMBOL} File is write-protected: ${path}`,
        };
      }

      if (!existsSync(filePath)) {
        return {
          success: false,
          content: "",
          error: `File not found: ${path}`,
        };
      }

      if (!diff || typeof diff !== "string") {
        return {
          success: false,
          content: "",
          error: "Missing or invalid 'diff' parameter",
        };
      }

      const hunks = parseUnifiedDiff(diff);

      if (hunks.length === 0) {
        return {
          success: false,
          content: "",
          error: "No valid hunks found in diff",
        };
      }

      const originalContent = await readFile(filePath, "utf-8");
      const newContent = applyHunks(originalContent, hunks);

      await writeFile(filePath, newContent, "utf-8");

      return {
        success: true,
        content: `Diff applied successfully to ${path}\n${hunks.length} hunk(s) processed`,
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
