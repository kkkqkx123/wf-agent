/**
 * The logic executed by the apply_diff tool
 * Uses SEARCH/REPLACE format with fuzzy matching
 */

import { readFile, writeFile } from "fs/promises";
import { existsSync } from "fs";
import type { ToolOutput } from "@wf-agent/types";
import type { ReadFileConfig } from "../../../types.js";
import { ProtectController, SHIELD_SYMBOL } from "@wf-agent/sdk/services";
import {
  parseSearchReplaceBlocks,
  validateMarkerSequencing,
  type SearchReplaceBlock,
} from "./utils/search-replace-parser.js";
import {
  getSimilarity,
  fuzzySearch,
  preserveIndentation,
  BUFFER_LINES,
} from "./utils/fuzzy-matcher.js";
import { unescapeHtmlEntities } from "./utils/text-normalization.js";

interface ApplyDiffConfig extends ReadFileConfig {
  fuzzyThreshold?: number;
  bufferLines?: number;
}

/**
 * Module-level consecutive mistake tracking per file path
 */
const consecutiveMistakes = new Map<string, number>();

/**
 * Create the `apply_diff` tool execution function
 */
export function createApplyDiffHandler(config: ApplyDiffConfig = {}) {
  const fuzzyThreshold = config.fuzzyThreshold ?? 0.9;
  const bufferLines = config.bufferLines ?? BUFFER_LINES;

  return async (params: Record<string, unknown>): Promise<ToolOutput> => {
    try {
      const { path, diff: rawDiff } = params as { path: string; diff: string };

      // Validate required parameters
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
      const workspaceDir = config.workspaceDir ?? process.cwd();

      // Initialize protect controller if enabled
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
        const currentCount = (consecutiveMistakes.get(path) || 0) + 1;
        consecutiveMistakes.set(path, currentCount);

        return {
          success: false,
          content: "",
          error: `File not found: ${path}. Use read_file to verify the file exists.`,
        };
      }

      // Validate marker sequencing
      const validation = validateMarkerSequencing(diffContent);
      if (!validation.success) {
        return {
          success: false,
          content: "",
          error: validation.error!,
        };
      }

      // Parse SEARCH/REPLACE blocks
      const { blocks, error: parseError } = parseSearchReplaceBlocks(diffContent);
      if (parseError || blocks.length === 0) {
        return {
          success: false,
          content: "",
          error: parseError || "No valid SEARCH/REPLACE blocks found",
        };
      }

      // Read original content
      const originalContent = await readFile(filePath, "utf-8");
      let resultLines = originalContent.split(/\r?\n/);
      let delta = 0;
      let appliedCount = 0;
      const failParts: Array<{ success: boolean; error?: string }> = [];

      // Sort blocks by startLine (if provided)
      const sortedBlocks = [...blocks].sort(
        (a, b) => (a.startLine || 0) - (b.startLine || 0),
      );

      // Apply each block
      for (const block of sortedBlocks) {
        const result = applyBlock(
          resultLines,
          block,
          delta,
          fuzzyThreshold,
          bufferLines,
        );

        if (result.success) {
          resultLines = result.lines;
          delta = result.delta;
          appliedCount++;
        } else {
          failParts.push({ success: false, error: result.error });
        }
      }

      if (appliedCount === 0) {
        const currentCount = (consecutiveMistakes.get(path) || 0) + 1;
        consecutiveMistakes.set(path, currentCount);

        const hint =
          currentCount >= 2
            ? "\nHint: Multiple failures detected. Use read_file to get the current file content and retry."
            : "";

        return {
          success: false,
          content: "",
          error: `Failed to apply any changes${hint}\n\nFailures:\n${failParts
            .map((f) => f.error)
            .join("\n")}`,
        };
      }

      // Reset consecutive mistakes on success
      consecutiveMistakes.delete(path);

      // Write the file
      const lineEnding = originalContent.includes("\r\n") ? "\r\n" : "\n";
      const finalContent = resultLines.join(lineEnding);
      await writeFile(filePath, finalContent, "utf-8");

      const partialHint =
        failParts.length > 0
          ? `\nWarning: ${failParts.length} block(s) failed to apply. Use read_file to verify changes.`
          : "";

      return {
        success: true,
        content: `Diff applied successfully to ${path}\n${appliedCount} block(s) processed${partialHint}`,
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

/**
 * Apply a single SEARCH/REPLACE block to the content lines.
 */
function applyBlock(
  resultLines: string[],
  block: SearchReplaceBlock,
  delta: number,
  fuzzyThreshold: number,
  bufferLines: number,
):
  | { success: true; lines: string[]; delta: number }
  | { success: false; error: string } {
  let { searchContent, replaceContent } = block;
  let startLine = block.startLine ? block.startLine + delta : 0;

  // Split content into lines
  let searchLines = searchContent === "" ? [] : searchContent.split(/\r?\n/);
  let replaceLines = replaceContent === "" ? [] : replaceContent.split(/\r?\n/);

  // Validate non-empty search
  if (searchLines.length === 0) {
    return {
      success: false,
      error: "Empty search content is not allowed",
    };
  }

  const searchChunk = searchLines.join("\n");

  // Determine search bounds
  let searchStartIndex = 0;
  let searchEndIndex = resultLines.length;
  let matchIndex = -1;
  let bestMatchScore = 0;
  let bestMatchContent = "";

  // If startLine provided, try exact match first
  if (startLine > 0) {
    const exactStartIndex = startLine - 1;
    const searchLen = searchLines.length;
    const exactEndIndex = exactStartIndex + searchLen - 1;

    const originalChunk = resultLines
      .slice(exactStartIndex, exactEndIndex + 1)
      .join("\n");
    const similarity = getSimilarity(originalChunk, searchChunk);

    if (similarity >= fuzzyThreshold) {
      matchIndex = exactStartIndex;
      bestMatchScore = similarity;
      bestMatchContent = originalChunk;
    } else {
      // Set bounds for buffered search
      searchStartIndex = Math.max(0, startLine - (bufferLines + 1));
      searchEndIndex = Math.min(
        resultLines.length,
        startLine + searchLines.length + bufferLines,
      );
    }
  }

  // If no match yet, try fuzzy search
  if (matchIndex === -1) {
    const {
      bestScore,
      bestMatchIndex,
      bestMatchContent: midContent,
    } = fuzzySearch(resultLines, searchChunk, searchStartIndex, searchEndIndex);

    matchIndex = bestMatchIndex;
    bestMatchScore = bestScore;
    bestMatchContent = midContent;
  }

  // Check if match is good enough
  if (matchIndex === -1 || bestMatchScore < fuzzyThreshold) {
    const lineRange = startLine ? ` at line: ${startLine}` : "";

    return {
      success: false,
      error: `No sufficiently similar match found${lineRange} (${Math.floor(bestMatchScore * 100)}% similar, needs ${Math.floor(fuzzyThreshold * 100)}%)`,
    };
  }

  // Get matched lines and preserve indentation
  const matchedLines = resultLines.slice(
    matchIndex,
    matchIndex + searchLines.length,
  );

  const indentedReplaceLines = preserveIndentation(
    matchedLines,
    searchLines,
    replaceLines,
  );

  // Apply replacement
  const beforeMatch = resultLines.slice(0, matchIndex);
  const afterMatch = resultLines.slice(matchIndex + searchLines.length);

  const newLines = [...beforeMatch, ...indentedReplaceLines, ...afterMatch];
  const newDelta = delta - matchedLines.length + replaceLines.length;

  return {
    success: true,
    lines: newLines,
    delta: newDelta,
  };
}

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
