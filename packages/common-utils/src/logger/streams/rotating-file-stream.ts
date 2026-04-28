/**
 * Rotating File Stream
 * Supports log rotation based on file size
 * Automatically archives old logs and maintains a maximum number of backup files
 */

import * as fs from "fs";
import * as path from "path";
import { BaseFileStream, type BaseFileStreamOptions } from "./base-file-stream.js";
import type { LogStream, LogEntry } from "../types.js";

/**
 * Rotating File Stream Options
 */
export interface RotatingFileStreamOptions extends BaseFileStreamOptions {
  /** Maximum file size in bytes before rotation (default: 100MB) */
  maxSize?: number;
  /** Maximum number of backup files to keep (default: 10) */
  maxFiles?: number;
  /** Whether to compress rotated files (default: false) */
  compress?: boolean;
}

/**
 * RotatingFileStream class
 * Implements log rotation based on file size
 */
export class RotatingFileStream extends BaseFileStream implements LogStream {
  private maxSize: number;
  private maxFiles: number;
  private compress: boolean;
  private writeStream: fs.WriteStream;
  private currentSize: number = 0;

  constructor(options: RotatingFileStreamOptions = {}) {
    super(options);

    this.maxSize = options.maxSize ?? 100 * 1024 * 1024; // Default 100MB
    this.maxFiles = options.maxFiles ?? 10;
    this.compress = options.compress ?? false;

    // Get current file size if appending
    this.currentSize = this.getCurrentFileSize();

    // Create write stream
    this.writeStream = this.createWriteStream();
    this.setupErrorHandling();
  }

  /**
   * Get current file size
   */
  private getCurrentFileSize(): number {
    try {
      if (fs.existsSync(this.filePath)) {
        const stats = fs.statSync(this.filePath);
        return stats.size;
      }
    } catch {
      // Ignore errors
    }
    return 0;
  }

  /**
   * Create write stream
   */
  private createWriteStream(): fs.WriteStream {
    return fs.createWriteStream(this.filePath, {
      flags: "a",
      encoding: "utf8",
    });
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
   * Write log entry
   */
  write(entry: LogEntry): void {
    const line = this.formatEntry(entry) + "\n";
    const lineSize = Buffer.byteLength(line, "utf8");

    // Check if rotation is needed
    if (this.currentSize + lineSize > this.maxSize) {
      this.rotate();
    }

    // Add to buffer using base class method
    this.addToBuffer(line);

    // Update current size
    this.currentSize += lineSize;
  }

  /**
   * Rotate log files
   */
  private rotate(): void {
    // Flush current buffer
    this.flushSync();

    // Close current stream
    this.writeStream.end();

    // Rotate existing files
    this.rotateFiles();

    // Create new stream
    this.writeStream = this.createWriteStream();
    this.setupErrorHandling();
    this.currentSize = 0;
  }

  /**
   * Rotate existing log files
   */
  private rotateFiles(): void {
    const dir = path.dirname(this.filePath);
    const basename = path.basename(this.filePath);
    const ext = path.extname(basename);
    const name = basename.slice(0, -ext.length) || basename;

    // Remove oldest file if maxFiles reached
    const oldestFile = path.join(dir, `${name}.${this.maxFiles}${ext}`);
    if (fs.existsSync(oldestFile)) {
      try {
        fs.unlinkSync(oldestFile);
      } catch {
        // Ignore errors
      }
    }

    // Shift existing files
    for (let i = this.maxFiles - 1; i >= 1; i--) {
      const oldFile = path.join(dir, `${name}.${i}${ext}`);
      const newFile = path.join(dir, `${name}.${i + 1}${ext}`);

      if (fs.existsSync(oldFile)) {
        try {
          fs.renameSync(oldFile, newFile);
        } catch {
          // Ignore errors
        }
      }
    }

    // Rename current file to .1
    if (fs.existsSync(this.filePath)) {
      const newFile = path.join(dir, `${name}.1${ext}`);
      try {
        fs.renameSync(this.filePath, newFile);

        // Compress if enabled
        if (this.compress) {
          this.compressFile(newFile).catch(() => {});
        }
      } catch {
        // Ignore errors
      }
    }
  }

  /**
   * Compress a file using gzip
   */
  private async compressFile(filePath: string): Promise<void> {
    try {
      const zlib = await import("zlib");
      const gzip = zlib.createGzip();
      const input = fs.createReadStream(filePath);
      const output = fs.createWriteStream(`${filePath}.gz`);

      input.pipe(gzip).pipe(output);

      await new Promise<void>((resolve, reject) => {
        output.on("finish", resolve);
        output.on("error", reject);
      });

      try {
        fs.unlinkSync(filePath);
      } catch {
        // Ignore errors
      }
    } catch {
      // If compression fails (e.g., zlib not available), leave file uncompressed
    }
  }

  /**
   * Flush buffer to file (async)
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
    this.writeStream.on(event as string, handler);
  }

  /**
   * Remove event listener
   */
  off(event: string, handler: (...args: unknown[]) => void): void {
    this.writeStream.off(event as string, handler);
  }

  /**
   * Get current file path
   */
  getFilePath(): string {
    return this.filePath;
  }

  /**
   * Get current file size
   */
  getCurrentSize(): number {
    return this.currentSize;
  }
}

/**
 * Create rotating file stream
 */
export function createRotatingFileStream(options: RotatingFileStreamOptions): LogStream {
  return new RotatingFileStream(options);
}
