/**
 * Parser for the apply_patch tool format
 * Converts patch text into structured hunks following the Codex apply_patch specification
 *
 * Grammar:
 * Patch := Begin { FileOp } End
 * Begin := "*** Begin Patch" NEWLINE
 * End := "*** End Patch" NEWLINE
 * FileOp := AddFile | DeleteFile | UpdateFile
 * AddFile := "*** Add File: " path NEWLINE { "+" line NEWLINE }
 * DeleteFile := "*** Delete File: " path NEWLINE
 * UpdateFile := "*** Update File: " path NEWLINE [ MoveTo ] { Hunk }
 * MoveTo := "*** Move to: " newPath NEWLINE
 * Hunk := "@@" [ header ] NEWLINE { HunkLine } [ "*** End of File" NEWLINE ]
 * HunkLine := (" " | "-" | "+") text NEWLINE
 */

import { PatchParseError, PatchErrors, ToolErrorCode } from "@wf-agent/types";
import type { Hunk, UpdateFileChunk, ApplyPatchArgs } from "./types.js";

const BEGIN_PATCH_MARKER = "*** Begin Patch";
const END_PATCH_MARKER = "*** End Patch";
const ADD_FILE_MARKER = "*** Add File: ";
const DELETE_FILE_MARKER = "*** Delete File: ";
const UPDATE_FILE_MARKER = "*** Update File: ";
const MOVE_TO_MARKER = "*** Move to: ";
const EOF_MARKER = "*** End of File";
const CHANGE_CONTEXT_MARKER = "@@ ";
const EMPTY_CHANGE_CONTEXT_MARKER = "@@";

/**
 * Validate a file path for security and correctness
 * @throws PatchParseError if the path is invalid
 */
function validatePath(path: string, lineNumber: number): void {
  if (!path || path.trim().length === 0) {
    throw new PatchParseError("Empty file path", ToolErrorCode.PATCH_INVALID_PATH, lineNumber);
  }

  const trimmedPath = path.trim();

  // Check for absolute paths
  if (trimmedPath.startsWith("/") || /^[A-Za-z]:/.test(trimmedPath)) {
    throw PatchErrors.absolutePathNotAllowed(path, lineNumber);
  }

  // Check for path traversal
  if (trimmedPath.includes("..")) {
    throw PatchErrors.pathTraversalDetected(path, lineNumber);
  }

  // Check for invalid characters (basic check)
  const invalidChars = /[<>:"|?*]/g;
  if (invalidChars.test(trimmedPath)) {
    throw PatchErrors.invalidFilenameCharacters(path, lineNumber);
  }
}

/**
 * Check if lines start and end with correct patch markers
 * @throws PatchParseError if markers are missing
 */
function checkPatchBoundaries(lines: string[]): void {
  if (lines.length === 0) {
    throw new PatchParseError("Empty patch", ToolErrorCode.PATCH_INVALID_FORMAT);
  }

  const firstLine = lines[0]?.trim();
  const lastLine = lines[lines.length - 1]?.trim();

  if (firstLine !== BEGIN_PATCH_MARKER) {
    throw new PatchParseError(
      "The first line of the patch must be '*** Begin Patch'",
      ToolErrorCode.PATCH_MISSING_BEGIN_MARKER,
      1,
    );
  }

  if (lastLine !== END_PATCH_MARKER) {
    throw new PatchParseError(
      "The last line of the patch must be '*** End Patch'",
      ToolErrorCode.PATCH_MISSING_END_MARKER,
      lines.length,
    );
  }
}

/**
 * Parse a single UpdateFileChunk from lines
 * Returns the parsed chunk and number of lines consumed
 * @throws PatchParseError if the chunk is invalid
 */
function parseUpdateFileChunk(
  lines: string[],
  lineNumber: number,
  allowMissingContext: boolean,
): { chunk: UpdateFileChunk; linesConsumed: number } {
  if (lines.length === 0) {
    throw new PatchParseError(
      "Update hunk does not contain any lines",
      ToolErrorCode.PATCH_INVALID_HUNK_FORMAT,
      lineNumber,
    );
  }

  let changeContext: string | null = null;
  let startIndex = 0;

  // Check for context marker
  if (lines[0] === EMPTY_CHANGE_CONTEXT_MARKER) {
    changeContext = null;
    startIndex = 1;
  } else if (lines[0]?.startsWith(CHANGE_CONTEXT_MARKER)) {
    changeContext = lines[0].substring(CHANGE_CONTEXT_MARKER.length);
    startIndex = 1;
  } else if (!allowMissingContext) {
    throw new PatchParseError(
      `Expected update hunk to start with a @@ context marker, got: '${lines[0]}'`,
      ToolErrorCode.PATCH_INVALID_HUNK_FORMAT,
      lineNumber,
    );
  }

  if (startIndex >= lines.length) {
    throw new PatchParseError(
      "Update hunk does not contain any lines",
      ToolErrorCode.PATCH_INVALID_HUNK_FORMAT,
      lineNumber + 1,
    );
  }

  const chunk: UpdateFileChunk = {
    changeContext,
    oldLines: [],
    newLines: [],
    isEndOfFile: false,
  };

  let parsedLines = 0;
  for (let i = startIndex; i < lines.length; i++) {
    const line = lines[i];
    if (!line) continue;

    if (line === EOF_MARKER) {
      if (parsedLines === 0) {
        throw new PatchParseError(
          "Update hunk does not contain any lines",
          ToolErrorCode.PATCH_INVALID_HUNK_FORMAT,
          lineNumber + 1,
        );
      }
      chunk.isEndOfFile = true;
      parsedLines++;
      break;
    }

    const firstChar = line.charAt(0);

    // Empty line is treated as context
    if (line === "") {
      chunk.oldLines.push("");
      chunk.newLines.push("");
      parsedLines++;
      continue;
    }

    switch (firstChar) {
      case " ":
        // Context line
        chunk.oldLines.push(line.substring(1));
        chunk.newLines.push(line.substring(1));
        parsedLines++;
        break;
      case "+":
        // Added line
        chunk.newLines.push(line.substring(1));
        parsedLines++;
        break;
      case "-":
        // Removed line
        chunk.oldLines.push(line.substring(1));
        parsedLines++;
        break;
      default:
        // If we haven't parsed any lines yet, it's an error
        if (parsedLines === 0) {
          throw new PatchParseError(
            `Unexpected line found in update hunk: '${line}'. Every line should start with ' ' (context line), '+' (added line), or '-' (removed line)`,
            ToolErrorCode.PATCH_INVALID_HUNK_FORMAT,
            lineNumber + 1,
          );
        }
        // Otherwise, assume this is the start of the next hunk
        return { chunk, linesConsumed: parsedLines + startIndex };
    }
  }

  return { chunk, linesConsumed: parsedLines + startIndex };
}

/**
 * Parse a single hunk (file operation) from lines
 * Returns the parsed hunk and number of lines consumed
 * @throws PatchParseError if the hunk is invalid
 */
function parseOneHunk(lines: string[], lineNumber: number): { hunk: Hunk; linesConsumed: number } {
  const firstLine = lines[0]?.trim();

  // Add File
  if (firstLine?.startsWith(ADD_FILE_MARKER)) {
    const path = firstLine.substring(ADD_FILE_MARKER.length);
    validatePath(path, lineNumber);

    let contents = "";
    let parsedLines = 1;

    for (let i = 1; i < lines.length; i++) {
      const line = lines[i];
      if (line?.startsWith("+")) {
        contents += line.substring(1) + "\n";
        parsedLines++;
      } else if (line !== undefined && line.trim() !== "" && !line.startsWith("***")) {
        // Non-empty line that doesn't start with + is invalid in Add File section
        // Unless it's a new file operation marker
        throw PatchErrors.invalidAddFileContent(line, lineNumber + i);
      } else {
        break;
      }
    }

    return {
      hunk: { type: "AddFile", path, contents },
      linesConsumed: parsedLines,
    };
  }

  // Delete File
  if (firstLine?.startsWith(DELETE_FILE_MARKER)) {
    const path = firstLine.substring(DELETE_FILE_MARKER.length);
    validatePath(path, lineNumber);

    return {
      hunk: { type: "DeleteFile", path },
      linesConsumed: 1,
    };
  }

  // Update File
  if (firstLine?.startsWith(UPDATE_FILE_MARKER)) {
    const path = firstLine.substring(UPDATE_FILE_MARKER.length);
    validatePath(path, lineNumber);

    let remainingLines = lines.slice(1);
    let parsedLines = 1;

    // Check for optional Move to line
    let movePath: string | null = null;
    if (remainingLines[0]?.startsWith(MOVE_TO_MARKER)) {
      movePath = remainingLines[0].substring(MOVE_TO_MARKER.length);
      validatePath(movePath, lineNumber + parsedLines);
      remainingLines = remainingLines.slice(1);
      parsedLines++;
    }

    const chunks: UpdateFileChunk[] = [];

    while (remainingLines.length > 0) {
      // Skip blank lines between chunks
      if (remainingLines[0]?.trim() === "") {
        parsedLines++;
        remainingLines = remainingLines.slice(1);
        continue;
      }

      // Stop if we hit another file operation marker
      if (remainingLines[0]?.startsWith("***")) {
        break;
      }

      const { chunk, linesConsumed } = parseUpdateFileChunk(
        remainingLines,
        lineNumber + parsedLines,
        chunks.length === 0, // Allow missing context for first chunk
      );
      chunks.push(chunk);
      parsedLines += linesConsumed;
      remainingLines = remainingLines.slice(linesConsumed);
    }

    if (chunks.length === 0) {
      throw PatchErrors.emptyUpdateFile(path, lineNumber);
    }

    return {
      hunk: { type: "UpdateFile", path, movePath, chunks },
      linesConsumed: parsedLines,
    };
  }

  throw PatchErrors.invalidFileHeader(firstLine ?? "", lineNumber);
}

/**
 * Parse a patch string into structured hunks
 *
 * @param patch - The patch text to parse
 * @returns Parsed patch with hunks
 * @throws PatchParseError if the patch is invalid
 */
export function parsePatch(patch: string): ApplyPatchArgs {
  const trimmedPatch = patch.trim();
  const lines = trimmedPatch.split("\n");

  // Handle heredoc-wrapped patches (lenient mode)
  let effectiveLines = lines;
  if (lines.length >= 4) {
    const firstLine = lines[0];
    const lastLine = lines[lines.length - 1];
    if (
      (firstLine === "<<EOF" || firstLine === "<<'EOF'" || firstLine === '<<"EOF"') &&
      lastLine?.endsWith("EOF")
    ) {
      effectiveLines = lines.slice(1, lines.length - 1);
    }
  }

  checkPatchBoundaries(effectiveLines);

  const hunks: Hunk[] = [];
  const lastLineIndex = effectiveLines.length - 1;
  let remainingLines = effectiveLines.slice(1, lastLineIndex); // Skip Begin and End markers
  let lineNumber = 2; // Start at line 2 (after Begin Patch)

  while (remainingLines.length > 0) {
    const { hunk, linesConsumed } = parseOneHunk(remainingLines, lineNumber);
    hunks.push(hunk);
    lineNumber += linesConsumed;
    remainingLines = remainingLines.slice(linesConsumed);
  }

  return {
    hunks,
    patch: effectiveLines.join("\n"),
  };
}
