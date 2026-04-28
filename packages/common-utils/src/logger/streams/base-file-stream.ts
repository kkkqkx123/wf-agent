/**
 * Base File Stream
 * Abstract base class for file-based log streams
 * Provides common functionality like buffering, formatting, and error handling
 */

import * as fs from "fs";
import * as path from "path";
import type { LogStream, LogEntry, StreamOptions } from "../types.js";

/**
 * Base file stream options
 */
export interface BaseFileStreamOptions extends StreamOptions {
  /** Error handler callback */
  onError?: (err: Error) => void;
  /** Whether to enable fallback to console on error */
  enableFallback?: boolean;
  /** Maximum buffer size in bytes (default: 64KB) */
  maxBufferSize?: number;
}

/**
 * Abstract base class for file streams
 */
export abstract class BaseFileStream implements LogStream {
  protected filePath: string;
  protected json: boolean;
  protected timestamp: boolean;
  protected buffer: string[] = [];
  protected bufferSize: number = 0;
  protected maxBufferSize: number;
  protected errorHandler?: (err: Error) => void;
  protected enableFallback: boolean;
  protected fallbackStream?: LogStream;
  protected hasError: boolean = false;
  protected droppedLogsCount: number = 0;
  protected lastErrorLogTime: number = 0;
  protected readonly ERROR_LOG_INTERVAL = 60000; // Log disk errors at most once per minute

  constructor(options: BaseFileStreamOptions = {}) {
    if (!options.filePath) {
      throw new Error("filePath is required for file stream");
    }

    this.filePath = options.filePath;
    this.json = options.json ?? true;
    this.timestamp = options.timestamp ?? true;
    this.errorHandler = options.onError;
    this.enableFallback = options.enableFallback ?? true;
    this.maxBufferSize = options.maxBufferSize ?? 64 * 1024; // 64KB default

    // Ensure directory exists
    this.ensureDirectoryExists();

    // Check disk space before creating stream (if possible)
    this.checkDiskSpace().catch(() => {});
  }

  /**
   * Ensure the directory for the log file exists
   */
  protected ensureDirectoryExists(): void {
    const dir = path.dirname(this.filePath);
    if (!fs.existsSync(dir)) {
      fs.mkdirSync(dir, { recursive: true });
    }
  }

  /**
   * Check available disk space (best effort)
   * Override in subclass for platform-specific implementations
   */
  protected async checkDiskSpace(): Promise<void> {
    try {
      if (process.platform !== "win32") {
        const { execSync } = await import("child_process");
        const dir = path.dirname(this.filePath);
        const result = execSync(`df -k "${dir}" 2>/dev/null | tail -1`, { encoding: "utf8" });
        const parts = result.trim().split(/\s+/);
        if (parts.length >= 4 && parts[3]) {
          const availableKB = parseInt(parts[3], 10);
          if (availableKB < 10240) {
            // Low disk space warning - log to stderr without using console
            process.stderr.write(
              `[BaseFileStream] Warning: Low disk space (${availableKB}KB available) for log file: ${this.filePath}\n`,
            );
          }
        }
      }
    } catch {
      // Ignore errors from disk space check
    }
  }

  /**
   * Handle stream errors with throttled logging
   */
  protected handleError(err: Error): void {
    this.hasError = true;
    this.droppedLogsCount++;

    // Throttle error logging to avoid spam
    const now = Date.now();
    if (now - this.lastErrorLogTime >= this.ERROR_LOG_INTERVAL) {
      process.stderr.write(
        `[BaseFileStream] Error writing to ${this.filePath}: ${err.message}. Dropped ${this.droppedLogsCount} logs since last error.\n`,
      );
      this.lastErrorLogTime = now;
      this.droppedLogsCount = 0;
    }

    // Call custom error handler if provided
    if (this.errorHandler) {
      try {
        this.errorHandler(err);
      } catch {
        // Ignore errors from custom handler
      }
    }

    // Setup fallback to console if enabled and not already done
    this.setupFallback().catch(() => {});
  }

  /**
   * Setup fallback stream for error recovery
   */
  protected async setupFallback(): Promise<void> {
    if (this.enableFallback && !this.fallbackStream) {
      try {
        const { createConsoleStream } = await import("./console-stream.js");
        this.fallbackStream = createConsoleStream({
          json: this.json,
          timestamp: this.timestamp,
        });
        process.stderr.write(`[BaseFileStream] Fallback to console logging activated for ${this.filePath}\n`);
      } catch {
        // If fallback also fails, we can't do much
      }
    }
  }

  /**
   * Check if buffer is approaching capacity
   */
  protected isBufferFull(): boolean {
    return this.bufferSize >= this.maxBufferSize * 0.9; // 90% threshold
  }

  /**
   * Format log entry as string
   */
  protected formatEntry(entry: LogEntry): string {
    if (this.json) {
      return JSON.stringify(entry);
    } else {
      const { level, message, timestamp, context, metadata, ...rest } = entry;
      const timestampStr = timestamp ? `[${timestamp}] ` : "";
      const levelStr = `[${level.toUpperCase()}] `;

      let output = `${timestampStr}${levelStr}${message}`;

      const extraData = { ...context, ...metadata, ...rest };
      if (Object.keys(extraData).length > 0) {
        output += ` ${JSON.stringify(extraData)}`;
      }

      return output;
    }
  }

  /**
   * Add entry to buffer
   */
  protected addToBuffer(line: string): void {
    // Check if buffer is approaching capacity
    if (this.isBufferFull()) {
      process.stderr.write(`[BaseFileStream] Buffer at 90% capacity, triggering emergency flush\n`);
      this.flush();
    }

    this.buffer.push(line);
    this.bufferSize += Buffer.byteLength(line, "utf8");

    // If buffer exceeds threshold, flush to file
    if (this.bufferSize >= this.maxBufferSize) {
      this.flush();
    }
  }

  /**
   * Write log entry - to be implemented by subclasses
   */
  abstract write(entry: LogEntry): void;

  /**
   * Flush buffer to file - to be implemented by subclasses
   */
  abstract flush(callback?: () => void): void;

  /**
   * Flush buffer synchronously - to be implemented by subclasses
   */
  abstract flushSync(): void;

  /**
   * End stream - to be implemented by subclasses
   */
  abstract end(): void;

  /**
   * Get buffered lines and clear buffer
   */
  protected drainBuffer(): string[] {
    const lines = this.buffer;
    this.buffer = [];
    this.bufferSize = 0;
    return lines;
  }

  /**
   * Write to fallback stream if available
   */
  protected writeToFallback(lines: string[]): void {
    if (this.fallbackStream) {
      lines.forEach(line => {
        this.fallbackStream!.write({
          level: "error",
          message: `Log fallback: ${line}`,
          timestamp: new Date().toISOString(),
        });
      });
    }
  }
}
