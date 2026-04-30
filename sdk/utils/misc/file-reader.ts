/**
 * File reading utilities with support for different modes.
 * 
 * Provides:
 * - Slice mode: Read contiguous line ranges with offset/limit
 * - Indentation mode: Extract semantic code blocks based on indentation hierarchy
 */

import { formatLineNumbers } from "../tool-utils.js";

/**
 * Result structure for slice mode reading
 */
export interface SliceReadResult {
  content: string;
  returnedLines: number;
  totalLines: number;
  wasTruncated: boolean;
  wasCharTruncated?: boolean; // Indicates if truncation was due to character limit
}

/**
 * Options for indentation mode reading
 */
export interface IndentationOptions {
  anchorLine: number;        // 1-indexed anchor line
  maxLevels?: number;        // Maximum indentation levels to include
  includeSiblings?: boolean; // Include sibling blocks at same level
  includeHeader?: boolean;   // Include parent context headers
  limit?: number;           // Maximum lines to return
  maxLines?: number;        // Alternative max lines parameter
  maxChars?: number;        // Maximum total characters to return
}

/**
 * Result structure for indentation mode reading
 */
export interface IndentationReadResult {
  content: string;
  includedRanges: Array<[number, number]>; // Array of [start, end] line ranges (1-indexed)
  totalLines: number;
  wasTruncated: boolean;
  wasCharTruncated?: boolean; // Indicates if truncation was due to character limit
}

/**
 * Default line limit for file reading
 */
const DEFAULT_LINE_LIMIT = 100;

/**
 * Default character limit for file reading (protects against extremely long lines)
 */
const DEFAULT_CHAR_LIMIT = 50000;

/**
 * Options for slice mode reading with character limit support
 */
export interface SliceReadOptions {
  offset?: number;        // Starting line (0-based)
  limit?: number;         // Maximum lines to read
  maxChars?: number;      // Maximum total characters to return
}

/**
 * Read file content using slice mode (contiguous line ranges).
 * 
 * @param content - Full file content as string
 * @param options - Slice read options or offset number (for backward compatibility)
 * @param limit - Number of lines to read (only used when options is a number)
 * @returns SliceReadResult with formatted content and metadata
 */
export function readWithSlice(
  content: string,
  options: number | SliceReadOptions = {},
  limit?: number
): SliceReadResult {
  // Support both old API (offset, limit) and new API (options object)
  let offset: number;
  let lineLimit: number;
  let maxChars: number | undefined;
  
  if (typeof options === 'number') {
    // Old API: readWithSlice(content, offset, limit)
    offset = options;
    lineLimit = limit ?? DEFAULT_LINE_LIMIT;
    maxChars = undefined;
  } else {
    // New API: readWithSlice(content, { offset, limit, maxChars })
    offset = options.offset ?? 0;
    lineLimit = options.limit ?? DEFAULT_LINE_LIMIT;
    maxChars = options.maxChars;
  }
  
  const lines = content.split("\n");
  const totalLines = lines.length;
  
  // Ensure offset is within bounds
  const startLine = Math.max(0, Math.min(offset, totalLines));
  const endLine = Math.min(totalLines, startLine + lineLimit);
  
  let selectedLines = lines.slice(startLine, endLine);
  
  // Format with line numbers (convert back to 1-indexed for display)
  let formattedContent = formatLineNumbers(selectedLines, startLine + 1);
  
  // Apply character limit if specified
  let wasCharTruncated = false;
  const charLimit = maxChars ?? DEFAULT_CHAR_LIMIT;
  
  if (formattedContent.length > charLimit) {
    // Truncate by characters, but try to end at a line boundary
    const truncatedContent = formattedContent.substring(0, charLimit);
    const lastNewLineIndex = truncatedContent.lastIndexOf("\n");
    
    if (lastNewLineIndex > 0) {
      // End at complete line
      formattedContent = truncatedContent.substring(0, lastNewLineIndex);
      // Recalculate how many lines we actually included
      const actualLines = formattedContent.split("\n").length;
      selectedLines = selectedLines.slice(0, actualLines);
    } else {
      // No newline found in truncated content, just use it
      formattedContent = truncatedContent;
    }
    
    wasCharTruncated = true;
  }
  
  const returnedLines = selectedLines.length;
  const wasTruncated = endLine < totalLines || wasCharTruncated;
  
  return {
    content: formattedContent,
    returnedLines,
    totalLines,
    wasTruncated,
    wasCharTruncated,
  };
}

/**
 * Read file content using indentation mode (semantic block extraction).
 * 
 * This analyzes code structure based on indentation levels and extracts
 * related blocks around the anchor line, similar to IDE code folding.
 * 
 * @param content - Full file content as string
 * @param options - Indentation mode options
 * @returns IndentationReadResult with semantic blocks and metadata
 */
export function readWithIndentation(
  content: string,
  options: IndentationOptions
): IndentationReadResult {
  const {
    anchorLine,
    maxLevels = 3,
    includeSiblings = true,
    includeHeader = true,
    limit = DEFAULT_LINE_LIMIT,
    maxLines,
    maxChars,
  } = options;

  const lines = content.split("\n");
  const totalLines = lines.length;
  
  // Convert 1-indexed anchor to 0-indexed
  const anchorIndex = Math.max(0, Math.min(anchorLine - 1, totalLines - 1));
  
  // Analyze indentation structure
  const indentLevels = analyzeIndentation(lines);
  
  // Find the block containing the anchor line
  const anchorBlock = findBlockAtLine(indentLevels, anchorIndex);
  
  if (!anchorBlock) {
    // Fallback: return just the anchor line
    const line = lines[anchorIndex];
    return {
      content: formatLineNumbers([line || ""], anchorLine),
      includedRanges: [[anchorLine, anchorLine]],
      totalLines,
      wasTruncated: false,
    };
  }
  
  // Collect related blocks based on options
  const rangesToInclude = collectRelatedBlocks(
    indentLevels,
    anchorBlock,
    maxLevels,
    includeSiblings,
    includeHeader
  );
  
  // Merge overlapping ranges and sort
  const mergedRanges = mergeRanges(rangesToInclude);
  
  // Extract lines from all ranges
  const extractedLines: string[] = [];
  let currentLine = 0;
  
  for (const [start, end] of mergedRanges) {
    // Add blank line between non-contiguous ranges
    if (start > currentLine && extractedLines.length > 0) {
      extractedLines.push("...");
    }
    
    for (let i = start; i <= end && i < totalLines; i++) {
      const line = lines[i];
      if (line !== undefined) {
        extractedLines.push(line);
      }
      currentLine = i + 1;
    }
  }
  
  // Apply line limit
  const effectiveLimit = maxLines || limit;
  let limitedLines = extractedLines.slice(0, effectiveLimit);
  let wasTruncated = extractedLines.length > effectiveLimit;
  
  // Calculate actual included ranges after limiting
  let finalRanges = calculateFinalRanges(limitedLines, mergedRanges);
  
  // Format with line numbers
  let formattedContent = formatLineNumbers(limitedLines, finalRanges[0]?.[0] || 1);
  
  // Apply character limit if specified
  let wasCharTruncated = false;
  const charLimit = maxChars ?? DEFAULT_CHAR_LIMIT;
  
  if (formattedContent.length > charLimit) {
    // Truncate by characters, but try to end at a line boundary
    const truncatedContent = formattedContent.substring(0, charLimit);
    const lastNewLineIndex = truncatedContent.lastIndexOf("\n");
    
    if (lastNewLineIndex > 0) {
      // End at complete line
      formattedContent = truncatedContent.substring(0, lastNewLineIndex);
      // Recalculate how many lines we actually included
      const actualLines = formattedContent.split("\n").length;
      limitedLines = limitedLines.slice(0, actualLines);
    } else {
      // No newline found in truncated content, just use it
      formattedContent = truncatedContent;
    }
    
    wasCharTruncated = true;
    wasTruncated = true;
    
    // Recalculate ranges after character truncation
    finalRanges = calculateFinalRanges(limitedLines, mergedRanges);
  }
  
  return {
    content: formattedContent,
    includedRanges: finalRanges,
    totalLines,
    wasTruncated,
    wasCharTruncated,
  };
}

/**
 * Analyze indentation levels for each line
 */
interface IndentInfo {
  lineIndex: number;
  indentLevel: number;
  indentChars: number;
  isEmpty: boolean;
}

function analyzeIndentation(lines: string[]): IndentInfo[] {
  return lines.map((line, index) => {
    const trimmed = line.trim();
    const isEmpty = trimmed.length === 0;
    
    if (isEmpty) {
      return { lineIndex: index, indentLevel: 0, indentChars: 0, isEmpty: true };
    }
    
    // Count leading spaces/tabs
    const match = line.match(/^(\s*)/);
    const indentChars = match && match[1] ? match[1].length : 0;
    
    // Normalize: treat 2 or 4 spaces as one level, tabs as one level
    const indentLevel = Math.floor(indentChars / 2); // Assume 2-space indentation
    
    return { lineIndex: index, indentLevel, indentChars, isEmpty };
  });
}

/**
 * Find the block structure at a given line
 */
interface BlockInfo {
  startLine: number;
  endLine: number;
  indentLevel: number;
  type: "block" | "statement";
}

function findBlockAtLine(indents: IndentInfo[], lineIndex: number): BlockInfo | null {
  const currentIndent = indents[lineIndex];
  if (!currentIndent || currentIndent.isEmpty) {
    return null;
  }
  
  // Find block boundaries
  let startLine = lineIndex;
  let endLine = lineIndex;
  
  // Expand upward to find block start
  for (let i = lineIndex - 1; i >= 0; i--) {
    const prev = indents[i];
    if (!prev) continue;
    if (prev.isEmpty) continue;
    if (prev.indentLevel >= currentIndent.indentLevel) {
      startLine = i;
    } else {
      break;
    }
  }
  
  // Expand downward to find block end
  for (let i = lineIndex + 1; i < indents.length; i++) {
    const next = indents[i];
    if (!next) break;
    if (next.isEmpty) {
      endLine = i;
      continue;
    }
    if (next.indentLevel >= currentIndent.indentLevel) {
      endLine = i;
    } else {
      break;
    }
  }
  
  return {
    startLine,
    endLine,
    indentLevel: currentIndent.indentLevel,
    type: currentIndent.indentLevel > 0 ? "block" : "statement",
  };
}

/**
 * Collect related blocks based on configuration
 */
function collectRelatedBlocks(
  indents: IndentInfo[],
  anchorBlock: BlockInfo,
  maxLevels: number,
  includeSiblings: boolean,
  includeHeader: boolean
): Array<[number, number]> {
  const ranges: Array<[number, number]> = [[anchorBlock.startLine, anchorBlock.endLine]];
  
  // Include parent blocks (headers)
  if (includeHeader && anchorBlock.indentLevel > 0) {
    const parentRange = findParentBlock(indents, anchorBlock.startLine, anchorBlock.indentLevel);
    if (parentRange) {
      ranges.unshift(parentRange);
    }
  }
  
  // Include sibling blocks
  if (includeSiblings) {
    const siblings = findSiblingBlocks(indents, anchorBlock, maxLevels);
    ranges.push(...siblings);
  }
  
  return ranges;
}

/**
 * Find parent block above current block
 */
function findParentBlock(
  indents: IndentInfo[],
  startLine: number,
  currentLevel: number
): [number, number] | null {
  for (let i = startLine - 1; i >= 0; i--) {
    const info = indents[i];
    if (!info) continue;
    if (info.isEmpty) continue;
    
    if (info.indentLevel < currentLevel) {
      // Found parent level, find its extent
      let parentStart = i;
      let parentEnd = i;
      
      for (let j = i + 1; j < indents.length; j++) {
        const next = indents[j];
        if (!next) break;
        if (next.isEmpty) {
          parentEnd = j;
          continue;
        }
        if (next.indentLevel >= info.indentLevel) {
          parentEnd = j;
        } else {
          break;
        }
      }
      
      return [parentStart, parentEnd];
    }
  }
  
  return null;
}

/**
 * Find sibling blocks at similar indentation levels
 */
function findSiblingBlocks(
  indents: IndentInfo[],
  anchorBlock: BlockInfo,
  maxLevels: number
): Array<[number, number]> {
  const siblings: Array<[number, number]> = [];
  const targetLevel = anchorBlock.indentLevel;
  
  // Search before anchor block
  for (let i = anchorBlock.startLine - 1; i >= 0; i--) {
    const info = indents[i];
    if (!info) break;
    if (info.isEmpty) continue;
    
    if (info.indentLevel === targetLevel) {
      // Found sibling, find its extent
      const siblingBlock = findBlockAtLine(indents, i);
      if (siblingBlock) {
        siblings.unshift([siblingBlock.startLine, siblingBlock.endLine]);
        i = siblingBlock.startLine; // Skip to before this block
      }
    } else if (info.indentLevel < targetLevel) {
      break; // Reached parent level, stop
    }
  }
  
  // Search after anchor block
  for (let i = anchorBlock.endLine + 1; i < indents.length; i++) {
    const info = indents[i];
    if (!info) break;
    if (info.isEmpty) continue;
    
    if (info.indentLevel === targetLevel) {
      const siblingBlock = findBlockAtLine(indents, i);
      if (siblingBlock) {
        siblings.push([siblingBlock.startLine, siblingBlock.endLine]);
        i = siblingBlock.endLine; // Skip past this block
      }
    } else if (info.indentLevel < targetLevel) {
      break; // Reached parent level, stop
    }
  }
  
  return siblings.slice(0, maxLevels * 2); // Limit number of siblings
}

/**
 * Merge overlapping or adjacent ranges
 */
function mergeRanges(ranges: Array<[number, number]>): Array<[number, number]> {
  if (ranges.length === 0) return [];
  
  // Sort by start position
  const sorted = [...ranges].sort((a, b) => a[0] - b[0]);
  const firstRange = sorted[0];
  if (!firstRange) return [];
  
  const merged: Array<[number, number]> = [firstRange];
  
  for (let i = 1; i < sorted.length; i++) {
    const current = sorted[i];
    if (!current) continue;
    
    const last = merged[merged.length - 1];
    
    if (last && current[0] <= last[1] + 1) {
      // Overlapping or adjacent, merge
      last[1] = Math.max(last[1], current[1]);
    } else {
      merged.push(current);
    }
  }
  
  return merged;
}

/**
 * Calculate final ranges after line limiting
 */
function calculateFinalRanges(
  limitedLines: string[],
  originalRanges: Array<[number, number]>
): Array<[number, number]> {
  if (limitedLines.length === 0) return [];
  
  // For simplicity, return a single range representing the displayed content
  // In a more sophisticated implementation, track exact line mappings
  const firstRange = originalRanges[0];
  if (!firstRange) return [[1, limitedLines.length]];
  
  return [[firstRange[0] + 1, firstRange[0] + limitedLines.length]];
}
