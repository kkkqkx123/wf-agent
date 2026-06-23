/**
 * The logic executed by the apply_diff tool
 * Uses SEARCH/REPLACE format with multi-pass sequence matching
 */

import type { ToolOutput } from "@wf-agent/types";
import { ProtectController, SHIELD_SYMBOL } from "@wf-agent/sdk/services";
import { parseSearchReplaceBlocks, validateMarkerSequencing } from "./utils/parser.js";
import { applyBlock } from "./utils/apply.js";
import type { ApplyDiffConfig } from "./utils/types.js";
import { resolveFilePath } from "@wf-agent/sdk/utils";
import { HostFSAdapter } from "../../../utils/host-fs-adapter.js";

/**
 * Create the `apply_diff` tool execution function
 */
export function createApplyDiffHandler(config: ApplyDiffConfig = {}) {
  return async (params: Record<string, unknown>): Promise<ToolOutput> => {
    try {
      const { path, diff } = params as { path: string; diff: string };

      // Validate required parameters
      if (!path || typeof path !== "string") {
        return {
          success: false,
          content: "",
          error: "Missing or invalid 'path' parameter",
        };
      }

      if (!diff || typeof diff !== "string") {
        return {
          success: false,
          content: "",
          error: "Missing or invalid 'diff' parameter",
        };
      }

      const filePath = resolveFilePath(path, config.workspaceDir);
      const workspaceDir = config.workspaceDir ?? process.cwd();

      // Initialize VFS and protect controller
      const vfs = config.vfs ?? new HostFSAdapter();
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

      if (!(await vfs.exists(filePath))) {
        return {
          success: false,
          content: "",
          error: `File not found: ${path}. Use read_file to verify the file exists.`,
        };
      }

      // Validate marker sequencing
      const validation = validateMarkerSequencing(diff);
      if (!validation.success) {
        return {
          success: false,
          content: "",
          error: validation.error!,
        };
      }

      // Parse SEARCH/REPLACE blocks
      const { blocks, error: parseError } = parseSearchReplaceBlocks(diff);
      if (parseError || blocks.length === 0) {
        return {
          success: false,
          content: "",
          error: parseError || "No valid SEARCH/REPLACE blocks found",
        };
      }

      // Read original content via VFS
      const originalBuf = await vfs.readFile(filePath);
      if (!originalBuf) {
        return {
          success: false,
          content: "",
          error: `File not found: ${path}. Use read_file to verify the file exists.`,
        };
      }
      const originalContent = originalBuf.toString("utf-8");
      let resultLines = originalContent.split(/\r?\n/);
      let delta = 0;
      let appliedCount = 0;
      const failedBlocks: string[] = [];

      // Sort blocks by startLine (if provided)
      const sortedBlocks = [...blocks].sort((a, b) => (a.startLine || 0) - (b.startLine || 0));

      // Apply each block
      for (const block of sortedBlocks) {
        const result = applyBlock(resultLines, block, delta);

        if (result.success && "lines" in result) {
          resultLines = result.lines;
          delta = result.delta;
          appliedCount++;
        } else {
          failedBlocks.push(result.error || "Unknown error");
        }
      }

      if (appliedCount === 0) {
        return {
          success: false,
          content: "",
          error: `Failed to apply any changes.\n\nFailures:\n${failedBlocks.join("\n")}`,
        };
      }

      // Write via VFS
      const lineEnding = originalContent.includes("\r\n") ? "\r\n" : "\n";
      const finalContent = resultLines.join(lineEnding);
      await vfs.writeFile(filePath, Buffer.from(finalContent, "utf-8"));

      const partialHint =
        failedBlocks.length > 0
          ? `\nWarning: ${failedBlocks.length} block(s) failed to apply. Use read_file to verify changes.`
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
