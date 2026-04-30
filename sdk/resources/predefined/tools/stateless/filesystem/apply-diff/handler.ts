/**
 * The logic executed by the apply_diff tool
 */

import { readFile, writeFile } from "fs/promises";
import { existsSync } from "fs";
import type { ToolOutput } from "@wf-agent/types";
import type { ReadFileConfig } from "../../../types.js";
import { ProtectController, SHIELD_SYMBOL } from "@wf-agent/sdk/services";
import {
  unescapeHtmlEntities,
  computeDiffStats,
  sanitizeUnifiedDiff,
} from "./utils/index.js";

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
 * Module-level consecutive mistake tracking per file path
 */
const consecutiveMistakes = new Map<string, number>();

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
 * Result of applying diff hunks with partial success support
 */
interface DiffApplyResult {
  success: boolean;
  content?: string;
  error?: string;
  failParts?: Array<{ success: boolean; error?: string }>;
}

/**
 * Apply hunks to file content with partial success tracking
 */
function applyHunksWithTracking(
  content: string,
  hunks: DiffHunk[],
): DiffApplyResult {
  const lines = content.split("\n");

  // Remove trailing empty line if present (from final newline)
  if (lines.length > 0 && lines[lines.length - 1] === "") {
    lines.pop();
  }

  // Process hunks in reverse order to preserve line numbers
  const sortedHunks = [...hunks].sort((a, b) => b.oldStart - a.oldStart);
  const failParts: Array<{ success: boolean; error?: string }> = [];
  let hasFailure = false;

  for (const hunk of sortedHunks) {
    try {
      const newLines: string[] = [];
      let oldLineIndex = hunk.oldStart - 1; // Convert to 0-indexed

      for (const line of hunk.lines) {
        if (line.startsWith("-")) {
          // Removed line - skip in output, but verify it matches
          const expectedLine = line.substring(1);
          const actualLine = lines[oldLineIndex] ?? "";
          if (actualLine !== expectedLine) {
            throw new Error(
              `Search block not found at line ${oldLineIndex + 1}: expected "${expectedLine}", got "${actualLine}"`,
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
      failParts.push({ success: true });
    } catch (error) {
      hasFailure = true;
      failParts.push({
        success: false,
        error: error instanceof Error ? error.message : String(error),
      });
    }
  }

  // Ensure file ends with newline
  if (lines.length === 0 || lines[lines.length - 1] !== "") {
    lines.push("");
  }

  const resultContent = lines.join("\n");

  if (hasFailure) {
    return {
      success: false,
      content: resultContent,
      error: "Some hunks failed to apply",
      failParts,
    };
  }

  return {
    success: true,
    content: resultContent,
  };
}

/**
 * Create the `apply_diff` tool execution function
 */
export function createApplyDiffHandler(config: ReadFileConfig = {}) {
  return async (params: Record<string, unknown>): Promise<ToolOutput> => {
    try {
      const { path, diff: rawDiff } = params as { path: string; diff: string };

      // Validate required parameters first
      if (!path || typeof path !== "string") {
        return {
          success: false,
          content: "",
          error: "Missing or invalid 'path' parameter",
        };
      }

      if (!rawDiff || typeof rawDiff !== "string") {
        return {
          success: false,
          content: "",
          error: "Missing or invalid 'diff' parameter",
        };
      }

      // Unescape HTML entities for non-Claude models
      let diffContent = rawDiff;
      if (config.modelId && !config.modelId.includes("claude")) {
        diffContent = unescapeHtmlEntities(rawDiff);
      }

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
        // Track consecutive mistakes for file not found
        const currentCount = (consecutiveMistakes.get(path) || 0) + 1;
        consecutiveMistakes.set(path, currentCount);

        return {
          success: false,
          content: "",
          error: `File not found: ${path}. Use read_file to verify the file exists.`,
        };
      }

      const hunks = parseUnifiedDiff(diffContent);

      if (hunks.length === 0) {
        return {
          success: false,
          content: "",
          error: "No valid hunks found in diff. Ensure diff format is correct with @@ headers.",
        };
      }

      const originalContent = await readFile(filePath, "utf-8");
      const diffResult = applyHunksWithTracking(originalContent, hunks);

      if (!diffResult.success || !diffResult.content) {
        // Track consecutive mistakes
        const currentCount = (consecutiveMistakes.get(path) || 0) + 1;
        consecutiveMistakes.set(path, currentCount);

        // Extract specific error from failParts if available
        let errorReason = diffResult.error || "Failed to apply diff";
        if (diffResult.failParts && diffResult.failParts.length > 0) {
          const firstFailure = diffResult.failParts.find(
            (p: { success: boolean; error?: string }) => !p.success,
          );
          if (firstFailure?.error) {
            errorReason = firstFailure.error;
          }
        }

        const hint = currentCount >= 2
          ? "\nHint: Multiple failures detected. Use read_file to get the current file content and retry."
          : "";

        return {
          success: false,
          content: "",
          error: `${errorReason}${hint}`,
        };
      }

      // Reset consecutive mistakes on success
      consecutiveMistakes.delete(path);

      // Write the file
      await writeFile(filePath, diffResult.content, "utf-8");

      // Generate diff statistics for feedback
      const unifiedPatch = sanitizeUnifiedDiff(diffContent);
      const diffStats = computeDiffStats(unifiedPatch);
      const statsMessage = diffStats
        ? ` (+${diffStats.additions}/-${diffStats.deletions})`
        : "";

      // Check for partial application
      const partialHint =
        diffResult.failParts &&
        diffResult.failParts.some((p: { success: boolean; error?: string }) => !p.success)
          ? "\nWarning: Some hunks were partially applied. Use read_file to verify changes."
          : "";

      return {
        success: true,
        content: `Diff applied successfully to ${path}${statsMessage}\n${hunks.length} hunk(s) processed${partialHint}`,
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
