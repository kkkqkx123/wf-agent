/**
 * Streaming file reader for optimized large file handling.
 * 
 * Uses Node.js streams to read large files efficiently without loading
 * the entire file into memory.
 */

import { createReadStream } from "fs";
import { formatLineNumbers } from "../tool-utils.js";

/**
 * Result structure for streaming reads
 */
export interface StreamReadResult {
  content: string;
  returnedLines: number;
  totalLines: number;
  wasTruncated: boolean;
}

/**
 * Read specific line ranges from a large file using streams.
 * 
 * This is more memory-efficient than reading the entire file,
 * especially for very large files (100MB+).
 * 
 * @param filePath - Absolute path to the file
 * @param startLine - Starting line number (1-indexed)
 * @param lineCount - Number of lines to read
 * @returns StreamReadResult with formatted content
 */
export async function readLinesWithStream(
  filePath: string,
  startLine: number = 1,
  lineCount: number = 100
): Promise<StreamReadResult> {
  return new Promise((resolve, reject) => {
    const stream = createReadStream(filePath, { encoding: "utf-8" });
    
    let currentLine = 1;
    let collectedLines: string[] = [];
    let buffer = "";
    let totalLinesRead = 0;
    let isInTargetRange = false;
    
    const targetStartLine = Math.max(1, startLine);
    const targetEndLine = targetStartLine + lineCount - 1;
    
    stream.on("data", (chunk: string) => {
      buffer += chunk;
      
      // Process complete lines in the buffer
      let newlineIndex: number;
      while ((newlineIndex = buffer.indexOf("\n")) !== -1) {
        const line = buffer.substring(0, newlineIndex);
        buffer = buffer.substring(newlineIndex + 1);
        
        totalLinesRead++;
        
        // Check if we're in the target range
        if (currentLine >= targetStartLine && currentLine <= targetEndLine) {
          collectedLines.push(line);
          isInTargetRange = true;
        } else if (isInTargetRange && currentLine > targetEndLine) {
          // We've passed the target range, stop reading
          stream.destroy();
          break;
        }
        
        currentLine++;
      }
    });
    
    stream.on("end", () => {
      // Handle last line without newline
      if (buffer.length > 0 && currentLine <= targetEndLine) {
        totalLinesRead++;
        if (currentLine >= targetStartLine) {
          collectedLines.push(buffer);
        }
      }
      
      resolve({
        content: formatLineNumbers(collectedLines, targetStartLine),
        returnedLines: collectedLines.length,
        totalLines: totalLinesRead,
        wasTruncated: totalLinesRead > targetEndLine,
      });
    });
    
    stream.on("error", (error) => {
      reject(error);
    });
  });
}

/**
 * Count total lines in a file efficiently using streams.
 * 
 * @param filePath - Absolute path to the file
 * @returns Total number of lines in the file
 */
export async function countFileLines(filePath: string): Promise<number> {
  return new Promise((resolve, reject) => {
    const stream = createReadStream(filePath, { encoding: "utf-8" });
    
    let lineCount = 0;
    let buffer = "";
    
    stream.on("data", (chunk: string) => {
      buffer += chunk;
      
      let newlineIndex: number;
      while ((newlineIndex = buffer.indexOf("\n")) !== -1) {
        buffer = buffer.substring(newlineIndex + 1);
        lineCount++;
      }
    });
    
    stream.on("end", () => {
      // Count last line if it doesn't end with newline
      if (buffer.length > 0) {
        lineCount++;
      }
      resolve(lineCount);
    });
    
    stream.on("error", (error) => {
      reject(error);
    });
  });
}
