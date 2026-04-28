/**
 * Transport factory function
 * Supports the creation of various transport streams
 */

import type { LogStream, StreamEntry, LogLevel } from "../types.js";
import { destination, type Destination } from "./destination.js";
import { createMultistream } from "../streams/index.js";

/**
 * Log Transport Configuration
 */
export interface LogTransportOptions {
  /**
   * Transport target
   * Can be a stream instance, a file path, or a predefined transport type.
   */
  target: string | LogStream;

  /**
   * Log level
   */
  level?: string;

  /**
   * Transport options
   */
  options?: Record<string, unknown>;

  /**
   * Whether to synchronize
   */
  sync?: boolean;
}

/**
 * Multi-Target Transport Configuration
 */
export interface MultiLogTransportOptions {
  /**
   * Transport target list
   */
  targets?: LogTransportOptions[];

  /**
   * Pipeline Configuration
   */
  pipeline?: LogTransportOptions[];

  /**
   * Whether to remove duplicates
   */
  dedupe?: boolean;

  /**
   * Custom level mapping
   */
  levels?: Record<string, number>;

  /**
   * Worker option (for worker thread transport)
   */
  worker?: Record<string, unknown>;

  /**
   * Whether to synchronize
   */
  sync?: boolean;
}

/**
 * Gender-neutral names
 */
export type TransportOptions = LogTransportOptions;
export type MultiTransportOptions = MultiLogTransportOptions;

/**
 * Create a transport stream
 * @param options Transport configuration
 * @returns LogStream instance
 */
export function transport(options: LogTransportOptions | MultiLogTransportOptions): LogStream {
  // If it's a multi-target configuration
  if (isMultiTransportOptions(options)) {
    return createMultiTransport(options);
  }

  // Single Objective Configuration
  return createSingleTransport(options);
}

/**
 * Create a single-target transport
 */
function createSingleTransport(options: LogTransportOptions): LogStream {
  const { target, options: transportOptions } = options;

  // If `target` is already a `LogStream`, return it directly.
  if (isLogStream(target)) {
    return target;
  }

  // If it's a string, parse it into the specific target.
  if (typeof target === "string") {
    return resolveTransportTarget(target, transportOptions);
  }

  throw new Error(`Invalid transport target: ${target}`);
}

/**
 * Create a multi-target transport
 */
function createMultiTransport(options: MultiLogTransportOptions): LogStream {
  const { targets, pipeline, dedupe, levels } = options;

  const streams: StreamEntry[] = [];

  // Process targets
  if (targets && targets.length > 0) {
    targets.forEach(target => {
      const stream = createSingleTransport(target);
      streams.push({
        stream,
        level: target.level ? (target.level as LogLevel | number) : undefined,
      });
    });
  }

  // Processing pipeline
  if (pipeline && pipeline.length > 0) {
    pipeline.forEach(target => {
      const stream = createSingleTransport(target);
      streams.push({
        stream,
        level: target.level ? (target.level as LogLevel | number) : undefined,
      });
    });
  }

  // Create a multistream
  return createMultistream(streams, {
    dedupe,
    levels,
  });
}

/**
 * Parse the transport target
 */
function resolveTransportTarget(target: string, options?: Record<string, unknown>): LogStream {
  // File path
  if (target.startsWith("file://") || target.endsWith(".log") || target.endsWith(".json")) {
    const filePath = target.startsWith("file://") ? target.slice(7) : target;
    return destination(filePath);
  }

  // Predefined transport types
  switch (target) {
    case "console":
    case "stdout":
      return destination(process.stdout);

    case "stderr":
      return destination(process.stderr);

    case "pino/file":
      if (options && options["destination"] !== undefined) {
        return destination(options["destination"] as Destination);
      }
      throw new Error("pino/file requires destination option");

    default:
      // Try to handle it as a file path.
      try {
        return destination(target);
      } catch (err) {
        throw new Error(`Unknown transport target: ${target}`, { cause: err });
      }
  }
}

/**
 * Check if it is a multi-target configuration.
 */
function isMultiTransportOptions(options: unknown): options is MultiLogTransportOptions {
  return !!options && ("targets" in (options as Record<string, unknown>) || "pipeline" in (options as Record<string, unknown>));
}

/**
 * Check if it is a LogStream
 */
function isLogStream(obj: unknown): obj is LogStream {
  return !!obj && typeof (obj as LogStream).write === "function";
}
