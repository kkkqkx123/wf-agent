/**
 * Destination factory function
 * Used to create various target streams
 */

import type { LogStream, StreamOptions } from "../types.js";
import { createConsoleStream, createFileStream } from "../streams/index.js";

/**
 * Destination Type
 */
export type Destination =
  | string
  | number
  | NodeJS.WritableStream
  | LogStream
  | { dest: Destination; options?: StreamOptions };

/**
 * Create the target stream
 * @param dest: Target configuration
 * @returns: LogStream instance
 */
export function destination(dest: Destination = process.stdout): LogStream {
  // If it is already a LogStream, return it directly.
  if (isLogStream(dest)) {
    return dest;
  }

  // If it's a Node.js WritableStream, wrap it as a LogStream.
  if (isWritableStream(dest)) {
    return wrapWritableStream(dest);
  }

  // If it's object configuration
  if (typeof dest === "object" && "dest" in dest) {
    return destination(dest.dest);
  }

  // If it's a file path
  if (typeof dest === "string") {
    return createFileStream({
      filePath: dest,
      append: true,
      json: true,
      timestamp: true,
    });
  }

  // If it's a file descriptor
  if (typeof dest === "number") {
    // File descriptor 1 is stdout, and 2 is stderr.
    if (dest === 1 || dest === 2) {
      return createConsoleStream({
        json: true,
        timestamp: true,
      });
    }
    throw new Error(`Unsupported file descriptor: ${dest}`);
  }

  // The default behavior is to return the console stream.
  return createConsoleStream({
    json: true,
    timestamp: true,
  });
}

/**
 * Check if it is a LogStream
 */
function isLogStream(obj: unknown): obj is LogStream {
  return !!obj && typeof (obj as LogStream).write === "function";
}

/**
 * Check if it is Node.js WriableStream
 */
function isWritableStream(obj: unknown): obj is NodeJS.WritableStream {
  return !!obj && typeof (obj as NodeJS.WritableStream).write === "function" && typeof (obj as NodeJS.WritableStream).end === "function";
}

/**
 * Wrap Node.js WriteStream as LogStream
 */
function wrapWritableStream(stream: NodeJS.WritableStream): LogStream {
  return {
    write(entry: unknown): void {
      stream.write(JSON.stringify(entry) + "\n");
    },
    flush(callback?: () => void): void {
      if (callback) {
        setImmediate(callback);
      }
    },
    end(): void {
      stream.end();
    },
    on(event: string, handler: (...args: unknown[]) => void): void {
      (stream as NodeJS.EventEmitter).on(event, handler as () => void);
    },
    off(event: string, handler: (...args: unknown[]) => void): void {
      (stream as NodeJS.EventEmitter).off(event, handler as () => void);
    },
  };
}
