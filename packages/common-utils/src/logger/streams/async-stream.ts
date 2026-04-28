/**
 * Asynchronous Output Stream
 * Queue-based asynchronous batch processing, without blocking the main thread
 * Includes safeguards against log loss during process exit
 */

import type { LogStream, LogEntry, StreamOptions } from "../types.js";

/**
 * log queue entry
 */
interface QueueItem {
  entry: LogEntry;
  callback?: () => void;
}

/**
 * Global exit handler state
 * Used to avoid registering multiple handlers for the same event
 */
const globalExitHandlerState = {
  registered: false,
  streams: new Set<AsyncStream>(),
};

/**
 * Register global exit handlers once
 * All AsyncStream instances share the same handlers
 */
function registerGlobalExitHandlers(): void {
  if (globalExitHandlerState.registered) {
    return;
  }
  globalExitHandlerState.registered = true;

  const flushAllStreams = () => {
    for (const stream of globalExitHandlerState.streams) {
      stream.flushSync();
    }
  };

  // Handle normal exit
  process.on("exit", flushAllStreams);

  // Handle signals
  const signals = ["SIGINT", "SIGTERM", "SIGUSR2"] as const;
  signals.forEach(signal => {
    process.once(signal, () => {
      flushAllStreams();
      // Give a small window for flush to complete
      setTimeout(() => process.exit(0), 100);
    });
  });

  // Handle uncaught exceptions
  process.once("uncaughtException", () => {
    flushAllStreams();
  });

  // Handle unhandled promise rejections
  process.once("unhandledRejection", () => {
    flushAllStreams();
  });
}

/**
 * AsyncStream class
 */
export class AsyncStream implements LogStream {
  private targetStream: LogStream;
  private queue: QueueItem[] = [];
  private batchSize: number;
  private isProcessing: boolean = false;
  private flushTimer?: NodeJS.Timeout;
  private isShuttingDown: boolean = false;
  private maxQueueSize: number;
  private droppedLogsCount: number = 0;

  constructor(targetStream: LogStream, options: StreamOptions = {}) {
    this.targetStream = targetStream;
    this.batchSize = options.batchSize ?? 10;
    this.maxQueueSize = (options as StreamOptions & { maxQueueSize?: number }).maxQueueSize ?? 10000;

    // Register this stream to the global handler set
    globalExitHandlerState.streams.add(this);
    // Ensure global handlers are registered (only once)
    registerGlobalExitHandlers();
  }

  /**
   * Writing log entries
   */
  write(entry: LogEntry): void {
    // If shutting down, write directly to avoid queue buildup
    if (this.isShuttingDown) {
      this.targetStream.write(entry);
      return;
    }

    // Check queue size limit to prevent memory issues
    if (this.queue.length >= this.maxQueueSize) {
      this.droppedLogsCount++;
      // Log dropped count periodically
      if (this.droppedLogsCount % 1000 === 1) {
        process.stderr.write(`[AsyncStream] Queue full, dropped ${this.droppedLogsCount} logs\n`);
      }
      return;
    }

    this.queue.push({ entry });

    // If the queue reaches the batch size, process it immediately
    if (this.queue.length >= this.batchSize) {
      this.processQueue();
    } else {
      // Otherwise delayed processing
      this.scheduleFlush();
    }
  }

  /**
   * Arranging for a delayed refresh
   */
  private scheduleFlush(): void {
    if (this.flushTimer || this.isShuttingDown) {
      return;
    }

    this.flushTimer = setTimeout(() => {
      this.flushTimer = undefined;
      this.processQueue();
    }, 100); // 100ms delay
  }

  /**
   * processing queue
   */
  private processQueue(): void {
    if (this.isProcessing || this.queue.length === 0) {
      return;
    }

    this.isProcessing = true;

    // Take out a batch of logs
    const batch = this.queue.splice(0, this.batchSize);

    // asynchronous processing
    setImmediate(() => {
      batch.forEach(item => {
        this.targetStream.write(item.entry);
      });

      this.isProcessing = false;

      // If there is still a surplus, continue processing
      if (this.queue.length > 0 && !this.isShuttingDown) {
        this.processQueue();
      }
    });
  }

  /**
   * refresh buffer (async)
   */
  flush(callback?: () => void): void {
    // Clear Timer
    if (this.flushTimer) {
      clearTimeout(this.flushTimer);
      this.flushTimer = undefined;
    }

    // Process all remaining logs
    const processAll = () => {
      if (this.queue.length > 0) {
        const batch = this.queue.splice(0, this.batchSize);
        batch.forEach(item => {
          this.targetStream.write(item.entry);
        });

        if (this.queue.length > 0) {
          setImmediate(processAll);
          return;
        }
      }

      // Refresh target stream
      if (this.targetStream.flush) {
        this.targetStream.flush(callback);
      } else if (callback) {
        setImmediate(callback);
      }
    };

    processAll();
  }

  /**
   * Synchronous flush - used during process exit
   */
  flushSync(): void {
    // Clear timer
    if (this.flushTimer) {
      clearTimeout(this.flushTimer);
      this.flushTimer = undefined;
    }

    // Process remaining queue items synchronously
    while (this.queue.length > 0) {
      const batch = this.queue.splice(0, this.batchSize);
      batch.forEach(item => {
        this.targetStream.write(item.entry);
      });
    }

    // Try to flush target stream synchronously if supported
    const targetStream = this.targetStream as LogStream & { flushSync?: () => void };
    if (targetStream.flushSync) {
      targetStream.flushSync();
    } else if (this.targetStream.flush) {
      // Fallback to async flush with synchronous wait
      let flushed = false;
      this.targetStream.flush(() => {
        flushed = true;
      });
      // Small busy-wait to allow flush to complete
      const start = Date.now();
      while (!flushed && Date.now() - start < 1000) {
        // Busy wait for up to 1 second
      }
    }
  }

  /**
   * End stream
   */
  end(): void {
    this.flush(() => {
      if (this.targetStream.end) {
        this.targetStream.end();
      }
    });
  }

  /**
   * event listener
   */
  on(event: string, handler: (...args: unknown[]) => void): void {
    if (this.targetStream.on) {
      this.targetStream.on(event, handler);
    }
  }

  /**
   * Removing event listeners
   */
  off(event: string, handler: (...args: unknown[]) => void): void {
    if (this.targetStream.off) {
      this.targetStream.off(event, handler);
    }
  }

  /**
   * Get queue size for monitoring
   */
  getQueueSize(): number {
    return this.queue.length;
  }

  /**
   * Get dropped logs count for monitoring
   */
  getDroppedCount(): number {
    return this.droppedLogsCount;
  }
}

/**
 * Creating an asynchronous stream
 */
export function createAsyncStream(targetStream: LogStream, options: StreamOptions = {}): LogStream {
  return new AsyncStream(targetStream, options);
}
