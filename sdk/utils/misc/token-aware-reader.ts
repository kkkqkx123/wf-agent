/**
 * Token-aware file reading utilities with budget management.
 * 
 * Provides functionality for:
 * - Counting lines and estimating tokens incrementally
 * - Reading files with token budget limits (single-pass operation)
 * - Early exit when token budget is exceeded
 */

import { createReadStream } from "fs";
import fs from "fs/promises";
import { createInterface } from "readline";
import { estimateTokens } from "../token-estimator.js";

/**
 * Result of line and token counting
 */
export interface LineAndTokenCountResult {
  /** Total number of lines counted */
  lineCount: number;
  /** Estimated token count */
  tokenEstimate: number;
  /** Whether the full file was scanned (false if early exit occurred) */
  complete: boolean;
}

/**
 * Options for line and token counting
 */
export interface LineAndTokenCountOptions {
  /** Maximum tokens allowed before early exit. If undefined, scans entire file */
  budgetTokens?: number;
  /** Number of lines to buffer before running token estimation (default: 256) */
  chunkLines?: number;
}

/**
 * Efficiently counts lines and estimates tokens in a file using streams with incremental token estimation.
 * Processes file in chunks to avoid memory issues and can early-exit when budget is exceeded.
 * 
 * @param filePath - Path to the file to analyze
 * @param options - Configuration options for counting
 * @returns A promise that resolves to line count, token estimate, and completion status
 */
export async function countFileLinesAndTokens(
  filePath: string,
  options: LineAndTokenCountOptions = {}
): Promise<LineAndTokenCountResult> {
  const { budgetTokens, chunkLines = 256 } = options;

  // Check if file exists
  try {
    await fs.access(filePath);
  } catch {
    throw new Error(`File not found: ${filePath}`);
  }

  return new Promise((resolve, reject) => {
    let lineCount = 0;
    let tokenEstimate = 0;
    let lineBuffer: string[] = [];
    let complete = true;
    let isProcessing = false;
    let shouldClose = false;

    const readStream = createReadStream(filePath);
    const rl = createInterface({
      input: readStream,
      crlfDelay: Infinity,
    });

    const processBuffer = async () => {
      if (lineBuffer.length === 0) return;

      const bufferText = lineBuffer.join("\n");
      lineBuffer = []; // Clear buffer before processing

      try {
        const chunkTokens = await estimateTokens(bufferText);
        tokenEstimate += chunkTokens;
      } catch (error) {
        // On tokenizer error, use conservative estimate: 2 char ≈ 1 token
        tokenEstimate += Math.ceil(bufferText.length / 2);
      }

      // Check if we've exceeded budget
      if (budgetTokens !== undefined && tokenEstimate > budgetTokens) {
        complete = false;
        shouldClose = true;
        rl.close();
        readStream.destroy();
      }
    };

    rl.on("line", (line) => {
      lineCount++;
      lineBuffer.push(line);

      // Process buffer when it reaches chunk size
      if (lineBuffer.length >= chunkLines && !isProcessing) {
        isProcessing = true;
        rl.pause();
        processBuffer()
          .then(() => {
            isProcessing = false;
            if (!shouldClose) {
              rl.resume();
            }
          })
          .catch((err) => {
            isProcessing = false;
            reject(err);
          });
      }
    });

    rl.on("close", async () => {
      // Wait for any ongoing processing to complete
      while (isProcessing) {
        await new Promise((r) => setTimeout(r, 10));
      }

      // Process any remaining lines in buffer
      try {
        await processBuffer();
        resolve({ lineCount, tokenEstimate, complete });
      } catch (err) {
        reject(err);
      }
    });

    rl.on("error", (err) => {
      reject(err);
    });

    readStream.on("error", (err) => {
      reject(err);
    });
  });
}

/**
 * Result of reading file with token budget
 */
export interface ReadWithBudgetResult {
  /** The content read up to the token budget */
  content: string;
  /** Actual token count of returned content */
  tokenCount: number;
  /** Total lines in the returned content */
  lineCount: number;
  /** Whether the entire file was read (false if truncated) */
  complete: boolean;
}

/**
 * Options for reading file with token budget
 */
export interface ReadWithBudgetOptions {
  /** Maximum tokens allowed. Required. */
  budgetTokens: number;
  /** Number of lines to buffer before token counting (default: 256) */
  chunkLines?: number;
}

/**
 * Reads a file while incrementally counting tokens, stopping when budget is reached.
 * 
 * Unlike validateFileTokenBudget + extractTextFromFile, this is a single-pass
 * operation that returns the actual content up to the token limit.
 * 
 * @param filePath - Path to the file to read
 * @param options - Budget and chunking options
 * @returns Content read, token count, and completion status
 */
export async function readFileWithTokenBudget(
  filePath: string,
  options: ReadWithBudgetOptions
): Promise<ReadWithBudgetResult> {
  const { budgetTokens, chunkLines = 256 } = options;

  // Verify file exists
  try {
    await fs.access(filePath);
  } catch {
    throw new Error(`File not found: ${filePath}`);
  }

  return new Promise((resolve, reject) => {
    let content = "";
    let lineCount = 0;
    let tokenCount = 0;
    let lineBuffer: string[] = [];
    let complete = true;
    let isProcessing = false;
    let shouldClose = false;

    const readStream = createReadStream(filePath);
    const rl = createInterface({
      input: readStream,
      crlfDelay: Infinity,
    });

    const processBuffer = async (): Promise<boolean> => {
      if (lineBuffer.length === 0) return true;

      const bufferText = lineBuffer.join("\n");
      const currentBuffer = [...lineBuffer];
      lineBuffer = [];

      // Count tokens for this chunk
      let chunkTokens: number;
      try {
        chunkTokens = await estimateTokens(bufferText);
      } catch {
        // Fallback: conservative estimate (2 chars per token)
        chunkTokens = Math.ceil(bufferText.length / 2);
      }

      // Check if adding this chunk would exceed budget
      if (tokenCount + chunkTokens > budgetTokens) {
        // Need to find cutoff within this chunk using binary search
        let low = 0;
        let high = currentBuffer.length;
        let bestFit = 0;
        let bestTokens = 0;

        while (low < high) {
          const mid = Math.floor((low + high + 1) / 2);
          const testContent = currentBuffer.slice(0, mid).join("\n");
          let testTokens: number;
          try {
            testTokens = await estimateTokens(testContent);
          } catch {
            testTokens = Math.ceil(testContent.length / 2);
          }

          if (tokenCount + testTokens <= budgetTokens) {
            bestFit = mid;
            bestTokens = testTokens;
            low = mid;
          } else {
            high = mid - 1;
          }
        }

        // Add best fit lines
        if (bestFit > 0) {
          const fitContent = currentBuffer.slice(0, bestFit).join("\n");
          content += (content.length > 0 ? "\n" : "") + fitContent;
          tokenCount += bestTokens;
          lineCount += bestFit;
        }
        complete = false;
        return false;
      }

      // Entire chunk fits - add it all
      content += (content.length > 0 ? "\n" : "") + bufferText;
      tokenCount += chunkTokens;
      lineCount += currentBuffer.length;
      return true;
    };

    rl.on("line", (line) => {
      lineBuffer.push(line);

      if (lineBuffer.length >= chunkLines && !isProcessing) {
        isProcessing = true;
        rl.pause();

        processBuffer()
          .then((continueReading) => {
            isProcessing = false;
            if (!continueReading) {
              shouldClose = true;
              rl.close();
              readStream.destroy();
            } else if (!shouldClose) {
              rl.resume();
            }
          })
          .catch((err) => {
            isProcessing = false;
            shouldClose = true;
            rl.close();
            readStream.destroy();
            reject(err);
          });
      }
    });

    rl.on("close", async () => {
      // Wait for any ongoing processing with timeout
      const maxWaitTime = 30000; // 30 seconds
      const startWait = Date.now();
      while (isProcessing) {
        if (Date.now() - startWait > maxWaitTime) {
          reject(new Error("Timeout waiting for buffer processing to complete"));
          return;
        }
        await new Promise((r) => setTimeout(r, 10));
      }

      // Process remaining buffer
      if (!shouldClose) {
        try {
          await processBuffer();
        } catch (err) {
          reject(err);
          return;
        }
      }

      resolve({ content, tokenCount, lineCount, complete });
    });

    rl.on("error", reject);
    readStream.on("error", reject);
  });
}
