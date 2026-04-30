/**
 * The logic executed by the read_file tool
 */

import { readFile, stat } from "fs/promises";
import { existsSync } from "fs";
import type { ToolOutput } from "@wf-agent/types";
import { detectBinaryFile } from "@wf-agent/sdk";
import type { ReadFileConfig } from "../../../types.js";
import { IgnoreController, IgnoreMode, ProtectController } from "@wf-agent/sdk/services";

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
 * Create the `read_file` tool execution function
 */
export function createReadFileHandler(config: ReadFileConfig = {}) {
  const maxFileSize = config.maxFileSize ?? 500000;
  const DEFAULT_CHAR_LIMIT = 50000; // Default character limit

  return async (params: Record<string, unknown>): Promise<ToolOutput> => {
    try {
      const {
        path: filePath,
        offset,
        limit,
        max_chars,
      } = params as { path: string; offset?: number; limit?: number; max_chars?: number };

      // Validate 1-indexed line number parameters
      if (offset !== undefined && offset < 1) {
        return {
          success: false,
          content: "",
          error: `offset must be a 1-indexed line number (got ${offset}). Line numbers start at 1.`,
        };
      }

      const absolutePath = resolvePath(filePath, config.workspaceDir);
      const workspaceDir = config.workspaceDir ?? process.cwd();

      // Initialize controllers if enabled
      let ignoreController: IgnoreController | undefined;
      let protectController: ProtectController | undefined;

      if (config.enableIgnore) {
        ignoreController = new IgnoreController({
          cwd: workspaceDir,
          mode: IgnoreMode.All,
        });
        await ignoreController.initialize();
      }

      if (config.enableProtect) {
        protectController = new ProtectController({ cwd: workspaceDir });
      }

      // Check if file is write-protected (informational only for read operations)
      const isProtected = protectController?.isWriteProtected(filePath) ?? false;

      // Check if file is accessible (ignore filtering)
      if (ignoreController && !ignoreController.validateAccess(absolutePath)) {
        return {
          success: false,
          content: "",
          error: `Access denied: ${filePath} is in ignore list`,
        };
      }

      // Check if file exists
      if (!existsSync(absolutePath)) {
        return {
          success: false,
          content: "",
          error: `File not found: ${filePath}`,
        };
      }

      // Check if path is a directory
      const stats = await stat(absolutePath);
      if (stats.isDirectory()) {
        return {
          success: false,
          content: "",
          error: `Cannot read '${filePath}' because it is a directory. Use list_files tool instead.`,
        };
      }

      // Check for binary files
      const binaryDetection = await detectBinaryFile(absolutePath, filePath);
      if (binaryDetection.isBinary) {
        return {
          success: false,
          content: "",
          error: `${binaryDetection.message || "Binary file detected"}. Text reading not supported for this file type.`,
        };
      }

      // Read the contents of the file as buffer first for graceful UTF-8 handling
      const buffer = await readFile(absolutePath);
      const content = buffer.toString("utf-8");
      const lines = content.split("\n");

      // Apply offsets and restrictions
      const start = offset ? Math.max(0, offset - 1) : 0; // Convert 1-indexed to 0-indexed
      const end = limit ? Math.min(lines.length, start + limit) : lines.length;
      let selectedLines = lines.slice(start, end);

      // Format with line numbers (convert back to 1-indexed for display)
      const formattedLines = selectedLines.map(
        (line, index) => `${(start + index + 1).toString().padStart(6, " ")}|${line}`
      );
      let numberedContent = formattedLines.join("\n");

      // Apply character limit if specified or use default
      const charLimit = max_chars ?? DEFAULT_CHAR_LIMIT;
      let wasCharTruncated = false;
      
      if (numberedContent.length > charLimit) {
        // Truncate by characters, but try to end at a line boundary
        const truncatedContent = numberedContent.substring(0, charLimit);
        const lastNewLineIndex = truncatedContent.lastIndexOf("\n");
        
        if (lastNewLineIndex > 0) {
          // End at complete line
          numberedContent = truncatedContent.substring(0, lastNewLineIndex);
          // Recalculate how many lines we actually included
          const actualLines = numberedContent.split("\n").length;
          selectedLines = selectedLines.slice(0, actualLines);
        } else {
          // No newline found in truncated content, just use it
          numberedContent = truncatedContent;
        }
        
        wasCharTruncated = true;
      }

      // Add truncation notice if needed
      let finalContent = numberedContent;
      const totalLines = lines.length;
      const shownLines = selectedLines.length;
      const actualEnd = start + shownLines;
      
      if (wasCharTruncated || actualEnd < totalLines) {
        const nextOffset = actualEnd + 1;
        const effectiveLimit = limit || 100;
        
        let truncationMessage = "IMPORTANT: File content truncated.\n";
        
        if (wasCharTruncated) {
          truncationMessage += `Status: Content truncated due to character limit (${charLimit} chars). Showing ${shownLines} of ${totalLines} total lines.\n`;
          if (actualEnd < totalLines) {
            truncationMessage += `To read more: Use the read_file tool with offset=${nextOffset} and limit=${effectiveLimit}.\n`;
          } else {
            truncationMessage += `To read remaining content: Use the read_file tool with offset=${nextOffset}.\n`;
          }
        } else {
          truncationMessage += `Status: Showing lines ${start + 1}-${actualEnd} of ${totalLines} total lines.\n`;
          truncationMessage += `To read more: Use the read_file tool with offset=${nextOffset} and limit=${effectiveLimit}.\n`;
        }
        
        finalContent = `${truncationMessage}\n${numberedContent}`;
      } else if (selectedLines.length === 0) {
        finalContent = "Note: File is empty";
      }

      // Add protection notice if applicable
      if (isProtected) {
        const protectionNotice = `\n\n[This file is write-protected and requires approval for modifications]`;
        finalContent = finalContent + protectionNotice;
      }

      return {
        success: true,
        content: finalContent,
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
