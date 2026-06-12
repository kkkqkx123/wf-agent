/**
 * The logic executed by the apply_patch tool
 */

import { dirname } from "path";
import type { ToolOutput } from "@wf-agent/types";
import type { ReadFileConfig } from "../../../types.js";
import { parsePatch } from "./utils/parser.js";
import { applyChunksToContent } from "./utils/apply.js";
import type { ApplyPatchFileResult, ApplyPatchResult, ApplyPatchSummary } from "./utils/types.js";
import { PatchErrors, PatchToolError, ToolErrorCode } from "@wf-agent/types";
import { ProtectController, SHIELD_SYMBOL } from "@wf-agent/sdk/services";
import { resolveFilePath } from "@wf-agent/sdk/utils";
import { HostFSAdapter } from "../../../utils/host-fs-adapter.js";

/**
 * Check if a path is a file (not a directory) via VFS
 */
async function isFileOnVFS(
  filePath: string,
  vfs: import("../../../types.js").VFSFileIO,
): Promise<boolean> {
  try {
    const entry = await vfs.stat(filePath);
    return entry !== null && entry.type === "file";
  } catch {
    return false;
  }
}

/**
 * Create the `apply_patch` tool execution function
 */
export function createApplyPatchHandler(config: ReadFileConfig = {}) {
  return async (params: Record<string, unknown>): Promise<ToolOutput> => {
    try {
      const { patch } = params as { patch: string };

      if (!patch || typeof patch !== "string") {
        return {
          success: false,
          content: "",
          error: "Missing or invalid 'patch' parameter",
        };
      }

      // Initialize VFS and protect controller
      const vfs = config.vfs ?? new HostFSAdapter();
      const workspaceDir = config.workspaceDir ?? process.cwd();
      const protectController = config.enableProtect
        ? new ProtectController({ cwd: workspaceDir })
        : undefined;

      // Parse the patch
      const { hunks } = parsePatch(patch);
      const results: ApplyPatchFileResult[] = [];

      // Process each hunk
      for (const hunk of hunks) {
        const filePath = resolveFilePath(hunk.path, config.workspaceDir);

        try {
          switch (hunk.type) {
            case "AddFile": {
              // Check if file is write-protected
              if (protectController?.isWriteProtected(filePath)) {
                results.push({
                  path: hunk.path,
                  operation: "add",
                  success: false,
                  error: `${SHIELD_SYMBOL} File is write-protected`,
                  errorCode: ToolErrorCode.PATCH_FILE_PROTECTED,
                });
                continue;
              }

              // Check if file already exists
              if (await vfs.exists(filePath)) {
                results.push({
                  path: hunk.path,
                  operation: "add",
                  success: false,
                  error: "File already exists",
                  errorCode: ToolErrorCode.PATCH_FILE_ALREADY_EXISTS,
                });
                continue;
              }

              // Create parent directory if needed
              const parentDir = dirname(filePath);
              try {
                await vfs.mkdir(parentDir);
              } catch (error) {
                const err = error instanceof Error ? error : new Error(String(error));
                throw PatchErrors.parentDirCreateFailed(filePath, err);
              }

              // Write the file
              try {
                await vfs.writeFile(filePath, Buffer.from(hunk.contents, "utf-8"));
              } catch (error) {
                const err = error instanceof Error ? error : new Error(String(error));
                throw PatchErrors.writeFailed(filePath, err);
              }

              results.push({
                path: hunk.path,
                operation: "add",
                success: true,
              });
              break;
            }

            case "DeleteFile": {
              // Check if file is write-protected
              if (protectController?.isWriteProtected(filePath)) {
                results.push({
                  path: hunk.path,
                  operation: "delete",
                  success: false,
                  error: `${SHIELD_SYMBOL} File is write-protected`,
                  errorCode: ToolErrorCode.PATCH_FILE_PROTECTED,
                });
                continue;
              }

              // Check if file exists
              if (!(await vfs.exists(filePath))) {
                results.push({
                  path: hunk.path,
                  operation: "delete",
                  success: false,
                  error: "File not found",
                  errorCode: ToolErrorCode.PATCH_FILE_NOT_FOUND,
                });
                continue;
              }

              // Check if it's a file (not a directory)
              if (!(await isFileOnVFS(filePath, vfs))) {
                results.push({
                  path: hunk.path,
                  operation: "delete",
                  success: false,
                  error: "Path is not a file",
                  errorCode: ToolErrorCode.PATCH_FILE_NOT_FOUND,
                });
                continue;
              }

              // Delete the file
              try {
                await vfs.remove(filePath);
              } catch (error) {
                const err = error instanceof Error ? error : new Error(String(error));
                throw PatchErrors.deleteFailed(filePath, err);
              }

              results.push({
                path: hunk.path,
                operation: "delete",
                success: true,
              });
              break;
            }

            case "UpdateFile": {
              // Check if file is write-protected
              if (protectController?.isWriteProtected(filePath)) {
                results.push({
                  path: hunk.path,
                  operation: "update",
                  success: false,
                  error: `${SHIELD_SYMBOL} File is write-protected`,
                  errorCode: ToolErrorCode.PATCH_FILE_PROTECTED,
                });
                continue;
              }

              // Check if file exists
              if (!(await vfs.exists(filePath))) {
                results.push({
                  path: hunk.path,
                  operation: "update",
                  success: false,
                  error: "File not found",
                  errorCode: ToolErrorCode.PATCH_FILE_NOT_FOUND,
                });
                continue;
              }

              // Check if it's a file (not a directory)
              if (!(await isFileOnVFS(filePath, vfs))) {
                results.push({
                  path: hunk.path,
                  operation: "update",
                  success: false,
                  error: "Path is not a file",
                  errorCode: ToolErrorCode.PATCH_FILE_NOT_FOUND,
                });
                continue;
              }

              // Read the original content
              let originalBuf: Buffer | null;
              try {
                originalBuf = await vfs.readFile(filePath);
              } catch {
                throw PatchErrors.fileNotFound(filePath);
              }
              if (!originalBuf) {
                throw PatchErrors.fileNotFound(filePath);
              }
              const originalContent = originalBuf.toString("utf-8");

              // Apply the chunks
              const newContent = applyChunksToContent(originalContent, filePath, hunk.chunks);

              // Handle move/rename if specified
              if (hunk.movePath) {
                const newFilePath = resolveFilePath(hunk.movePath, config.workspaceDir);

                // Check if destination is write-protected
                if (protectController?.isWriteProtected(newFilePath)) {
                  results.push({
                    path: hunk.path,
                    operation: "rename",
                    success: false,
                    error: `${SHIELD_SYMBOL} Destination file is write-protected`,
                    errorCode: ToolErrorCode.PATCH_FILE_PROTECTED,
                  });
                  continue;
                }

                // Check if destination already exists
                if (await vfs.exists(newFilePath)) {
                  throw PatchErrors.destinationExists(newFilePath);
                }

                // Create parent directory for new path if needed
                const newParentDir = dirname(newFilePath);
                try {
                  await vfs.mkdir(newParentDir);
                } catch (error) {
                  const err = error instanceof Error ? error : new Error(String(error));
                  throw PatchErrors.parentDirCreateFailed(newFilePath, err);
                }

                // Use VFS rename for move operation
                try {
                  await vfs.writeFile(newFilePath, Buffer.from(newContent, "utf-8"));
                  await vfs.remove(filePath);
                } catch (error) {
                  const err = error instanceof Error ? error : new Error(String(error));
                  throw PatchErrors.writeFailed(newFilePath, err);
                }

                results.push({
                  path: hunk.path,
                  operation: "rename",
                  success: true,
                  oldPath: hunk.path,
                  newPath: hunk.movePath,
                });
              } else {
                // Write the updated content
                try {
                  await vfs.writeFile(filePath, Buffer.from(newContent, "utf-8"));
                } catch (error) {
                  const err = error instanceof Error ? error : new Error(String(error));
                  throw PatchErrors.writeFailed(filePath, err);
                }

                results.push({
                  path: hunk.path,
                  operation: "update",
                  success: true,
                });
              }
              break;
            }
          }
        } catch (error) {
          // Handle structured errors
          if (error instanceof PatchToolError) {
            results.push({
              path: hunk.path,
              operation:
                hunk.type === "AddFile" ? "add" : hunk.type === "DeleteFile" ? "delete" : "update",
              success: false,
              error: error.message,
              errorCode: error.code,
            });
          } else {
            // Handle unexpected errors
            const err = error instanceof Error ? error : new Error(String(error));
            results.push({
              path: hunk.path,
              operation:
                hunk.type === "AddFile" ? "add" : hunk.type === "DeleteFile" ? "delete" : "update",
              success: false,
              error: err.message,
            });
          }
        }
      }

      // Calculate summary
      const summary: ApplyPatchSummary = {
        total: results.length,
        succeeded: results.filter(r => r.success).length,
        failed: results.filter(r => !r.success).length,
      };

      // Build result
      const result: ApplyPatchResult = {
        success: summary.failed === 0,
        results,
        summary,
      };

      // Format output
      const outputLines: string[] = [];
      for (const r of results) {
        if (r.success) {
          if (r.operation === "rename") {
            outputLines.push(`✓ Renamed '${r.oldPath}' to '${r.newPath}'`);
          } else {
            outputLines.push(
              `✓ ${r.operation === "add" ? "Added" : r.operation === "delete" ? "Deleted" : "Updated"} '${r.path}'`,
            );
          }
        } else {
          outputLines.push(`✗ Failed to ${r.operation} '${r.path}': ${r.error}`);
        }
      }

      outputLines.push("");
      outputLines.push(`Summary: ${summary.succeeded}/${summary.total} operations succeeded`);

      return {
        success: result.success,
        content: outputLines.join("\n"),
        error: result.success ? undefined : `${summary.failed} operation(s) failed`,
      };
    } catch (error) {
      // Handle top-level errors
      if (error instanceof PatchToolError) {
        return {
          success: false,
          content: "",
          error: error.message,
        };
      }

      const err = error instanceof Error ? error : new Error(String(error));
      return {
        success: false,
        content: "",
        error: `Unexpected error: ${err.message}`,
      };
    }
  };
}
