/**
 * The logic executed by the read_file tool
 */

import * as path from "path";
import type { ToolOutput } from "@wf-agent/types";
import type { ReadFileConfig } from "../../../types.js";
import { IgnoreController, ProtectController } from "@wf-agent/sdk/services";
import { resolveFilePath, formatFileSize, isLikelyTextFile } from "@wf-agent/sdk/utils";
import { readWithSlice, readWithIndentation } from "@wf-agent/sdk/utils";
import type { SliceReadOptions, IndentationOptions } from "@wf-agent/sdk/utils";
import { HostFSAdapter } from "../../../utils/host-fs-adapter.js";

/**
 * Create the `read_file` tool execution function
 */
export function createReadFileHandler(config: ReadFileConfig = {}) {
  const maxFileSize = config.maxFileSize ?? 500000;
  const defaultCharLimit = config.maxChars ?? 50000;
  const defaultLineLimit = config.maxLines ?? 2000;

  return async (params: Record<string, unknown>): Promise<ToolOutput> => {
    try {
      const {
        path: filePath,
        mode,
        offset,
        limit,
        max_chars,
        indentation: indentationRaw,
      } = params as {
        path: string;
        mode?: string;
        offset?: number;
        limit?: number;
        max_chars?: number;
        indentation?: Record<string, unknown>;
      };

      // Validate 1-indexed line number parameters
      if (offset !== undefined && offset < 1) {
        return {
          success: false,
          content: "",
          error: `offset must be a 1-indexed line number (got ${offset}). Line numbers start at 1.`,
        };
      }

      const absolutePath = resolveFilePath(filePath, config.workspaceDir);
      const workspaceDir = config.workspaceDir ?? process.cwd();

      // Initialize controllers if enabled
      let ignoreController: IgnoreController | undefined;
      let protectController: ProtectController | undefined;

      if (config.enableIgnore) {
        ignoreController = new IgnoreController({
          cwd: workspaceDir,
          mode: "all",
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

      // Check if file exists (via VFS or Host FS)
      const vfs = config.vfs ?? new HostFSAdapter();
      const vfsPath = absolutePath.replace(/\\/g, "/");
      const vfsStat = await vfs.stat(vfsPath);
      const fileExists = vfsStat !== null;
      const fileSize = vfsStat?.size ?? 0;
      const isDirectory = vfsStat?.type === "directory";

      if (!fileExists) {
        return {
          success: false,
          content: "",
          error: `File not found: ${filePath}`,
        };
      }

      if (isDirectory) {
        return {
          success: false,
          content: "",
          error: `Cannot read '${filePath}' because it is a directory. Use list_files tool instead.`,
        };
      }

      // Check file size against maxFileSize limit
      if (fileSize > maxFileSize) {
        return {
          success: false,
          content: "",
          error: `File too large: ${filePath} (${formatFileSize(fileSize)}). Maximum allowed size is ${formatFileSize(maxFileSize)}.`,
        };
      }

      // Quick check: reject obvious non-text files by extension
      if (!isLikelyTextFile(filePath)) {
        return {
          success: false,
          content: "",
          error: `Cannot read '${filePath}' with read_file. This appears to be a special format file (extension: ${path.extname(filePath)}).

For special format files, use dedicated extraction tools or scripts:
- PDF files: Use extract_pdf script or pdfplumber
- Word documents (.docx): Use extract_docx script or python-docx
- Excel files (.xlsx): Use extract_xlsx script or openpyxl/pandas
- Images: Use analyze_image script or PIL/OpenCV
- Archives (.zip, .tar, etc.): Use appropriate archive tools
- Other binary formats: Use specialized parsers

If this is actually a text file with an unusual extension, you can try reading it with a script executor.`,
        };
      }

      // Read the contents of the file as buffer first for graceful UTF-8 handling
      const vfsBuf = await vfs.readFile(vfsPath);
      const buffer = vfsBuf ? Buffer.from(vfsBuf) : Buffer.from("");
      const content = buffer.toString("utf-8");
      const lines = content.split("\n");
      const totalLines = lines.length;

      const charLimit = max_chars ?? defaultCharLimit;

      // Dispatch based on reading mode
      const readingMode = mode || "slice";
      let resultContent: string;
      let wasTruncated = false;
      let shownLines = 0;
      let actualEnd = 0;

      if (readingMode === "indentation") {
        // Indentation mode: semantic code block extraction
        const indentation = indentationRaw || {};

        // Validate anchor_line (must be 1-indexed)
        const anchorLine = (indentation["anchor_line"] as number | undefined) ?? offset ?? 1;
        if (anchorLine < 1) {
          return {
            success: false,
            content: "",
            error: `indentation.anchor_line must be a 1-indexed line number (got ${anchorLine}). Line numbers start at 1.`,
          };
        }

        const indentationOptions: IndentationOptions = {
          anchorLine,
          maxLevels: indentation["max_levels"] as number | undefined,
          includeSiblings: indentation["include_siblings"] as boolean | undefined,
          includeHeader: indentation["include_header"] as boolean | undefined,
          limit: limit ?? defaultLineLimit,
          maxLines: indentation["max_lines"] as number | undefined,
          maxChars: charLimit,
        };

        const result = readWithIndentation(content, indentationOptions);
        resultContent = result.content;
        wasTruncated = result.wasTruncated || result.wasCharTruncated || false;
        shownLines = result.includedRanges.reduce(
          (sum, [start, end]) => sum + (end - start + 1),
          0,
        );
        const lastRange = result.includedRanges[result.includedRanges.length - 1];
        actualEnd = lastRange ? lastRange[1] : anchorLine;
      } else {
        // Slice mode (default): simple offset/limit reading
        const start0 = offset ? Math.max(0, offset - 1) : 0; // Convert 1-indexed to 0-indexed
        const sliceOptions: SliceReadOptions = {
          offset: start0,
          limit: limit ?? defaultLineLimit,
          maxChars: charLimit,
        };

        const result = readWithSlice(content, sliceOptions);
        resultContent = result.content;
        wasTruncated = result.wasTruncated || result.wasCharTruncated || false;
        shownLines = result.returnedLines;
        actualEnd = start0 + shownLines;
      }

      // Build truncation notice if needed
      let finalContent = resultContent;
      if (wasTruncated || actualEnd < totalLines) {
        const nextOffset = actualEnd + 1;
        const effectiveLimit = limit || defaultLineLimit;

        let truncationMessage = "IMPORTANT: File content truncated.\n";
        truncationMessage += `Status: Showing lines ${actualEnd - shownLines + 1}-${actualEnd} of ${totalLines} total lines.\n`;
        truncationMessage += `To read more: Use the read_file tool with offset=${nextOffset} and limit=${effectiveLimit}.\n`;

        finalContent = `${truncationMessage}\n${resultContent}`;
      } else if (!resultContent || resultContent.trim().length === 0) {
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
