/**
 * File Output Stream
 * Supports file write and append modes
 */

import * as fs from "fs";
import { BaseFileStream, type BaseFileStreamOptions } from "./base-file-stream.js";
import type { LogStream, LogEntry } from "../types.js";

/**
 * FileStream Options
 */
export interface FileStreamOptions extends BaseFileStreamOptions {
  /** Whether to append to file (default: true) */
  append?: boolean;
}

/**
 * FileStream class
 */
export class FileStream extends BaseFileStream implements LogStream {
  private writeStream: fs.WriteStream;

  constructor(options: FileStreamOptions = {}) {
    super(options);

    // Creating a Write Stream
    const flags = options.append !== false ? "a" : "w";
    this.writeStream = fs.createWriteStream(this.filePath, {
      flags,
      encoding: "utf8",
    });

    // Setup error handling
    this.setupErrorHandling();
  }

  /**
   * Setup error handling
   */
  private setupErrorHandling(): void {
    this.writeStream.on("error", (err: Error) => {
      this.handleError(err);
    });
  }

  /**
   * Writing log entries
   */
  write(entry: LogEntry): void {
    // If in error state with no fallback, drop the log
    if (this.hasError && !this.fallbackStream) {
      this.droppedLogsCount++;
      return;
    }

    const line = this.formatEntry(entry) + "\n";
    this.addToBuffer(line);
  }

  /**
   * Flush buffer to file
   */
  flush(callback?: () => void): void {
    if (this.buffer.length === 0) {
      if (callback) {
        setImmediate(callback);
      }
      return;
    }

    const lines = this.drainBuffer();
    this.writeToFallback(lines);

    this.writeStream.write(lines.join(""), "utf8", err => {
      if (err) {
        this.handleError(err);
      }
      if (callback) {
        callback();
      }
    });
  }

  /**
   * Flush buffer synchronously
   */
  flushSync(): void {
    if (this.buffer.length === 0) {
      return;
    }

    const lines = this.drainBuffer().join("");
    this.writeToFallback([lines]);

    try {
      // Use fs.appendFileSync for synchronous write
      fs.appendFileSync(this.filePath, lines, "utf8");
    } catch (err) {
      this.handleError(err as Error);
    }
  }

  /**
   * End stream
   */
  end(): void {
    this.flush(() => {
      this.writeStream.end();
    });
  }

  /**
   * Event listener
   */
  on(event: string, handler: (...args: unknown[]) => void): void {
    this.writeStream.on(event as Parameters<typeof this.writeStream.on>[0], handler);
  }

  /**
   * Removing event listeners
   */
  off(event: string, handler: (...args: unknown[]) => void): void {
    this.writeStream.off(event as Parameters<typeof this.writeStream.off>[0], handler);
  }
}

/**
 * Create file stream
 */
export function createFileStream(options: FileStreamOptions): LogStream {
  return new FileStream(options);
}
