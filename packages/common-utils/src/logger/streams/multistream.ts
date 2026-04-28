/**
 * Multi-target output stream
 * Supports simultaneous output to multiple streams, supports level filtering and de-duplication.
 */

import type { LogStream, LogEntry, StreamEntry, MultistreamOptions, LogLevel } from "../types.js";
import { LOG_LEVEL_PRIORITY } from "../types.js";

/**
 * Multistream class
 */
export class Multistream implements LogStream {
  private streams: StreamEntry[] = [];
  private dedupe: boolean;
  private streamLevels: Record<string, number>;
  private nextId: number = 0;

  constructor(streams: StreamEntry[] = [], options: MultistreamOptions = {}) {
    this.dedupe = options.dedupe ?? false;
    this.streamLevels = { ...LOG_LEVEL_PRIORITY, ...options.levels };

    // Add initial streams
    streams.forEach(entry => this.add(entry));
  }

  /**
   * Writing log entries
   */
  write(entry: LogEntry): void {
    const level = this.streamLevels[entry.level] ?? 0;
    let recordedLevel = 0;

    // Determine traversal order based on dedupe
    const startIndex = this.dedupe ? this.streams.length - 1 : 0;
    const step = this.dedupe ? -1 : 1;
    const endIndex = this.dedupe ? -1 : this.streams.length;

    for (let i = startIndex; i !== endIndex; i += step) {
      const dest = this.streams[i];
      if (!dest) continue;

      const destLevel =
        dest.levelVal ??
        (typeof dest.level === "string" ? this.streamLevels[dest.level] : dest.level) ??
        0;

      if (destLevel <= level) {
        // If de-duplication is enabled and a different level has been logged, stop the
        if (this.dedupe && recordedLevel !== 0 && recordedLevel !== destLevel) {
          break;
        }

        dest.stream.write(entry);

        if (this.dedupe) {
          recordedLevel = destLevel;
        }
      } else if (!this.dedupe) {
        // If de-duplication is not enabled and the current stream level is higher than the log level, stop the
        break;
      }
    }
  }

  /**
   * Add stream
   */
  add(entry: StreamEntry): Multistream {
    if (!entry || !entry.stream) {
      throw new Error("stream entry must have a stream property");
    }

    // Determination of level values
    let levelVal: number;
    if (typeof entry.levelVal === "number") {
      levelVal = entry.levelVal;
    } else if (typeof entry.level === "string") {
      levelVal = this.streamLevels[entry.level] ?? 1;
    } else if (typeof entry.level === "number") {
      levelVal = entry.level;
    } else {
      levelVal = 1; // Default info level
    }

    const streamEntry: StreamEntry = {
      stream: entry.stream,
      level: entry.level,
      levelVal,
      id: ++this.nextId,
    };

    this.streams.push(streamEntry);

    // Sort by level (ascending)
    this.streams.sort((a, b) => (a.levelVal ?? 0) - (b.levelVal ?? 0));

    return this;
  }

  /**
   * Remove stream
   */
  remove(id: number): Multistream {
    const index = this.streams.findIndex(s => s.id === id);
    if (index >= 0) {
      this.streams.splice(index, 1);
    }
    return this;
  }

  /**
   * Refresh all streams
   */
  flush(callback?: () => void): void {
    let completed = 0;
    const total = this.streams.length;

    if (total === 0) {
      if (callback) {
        setImmediate(callback);
      }
      return;
    }

    this.streams.forEach(entry => {
      if (entry.stream.flush) {
        entry.stream.flush(() => {
          completed++;
          if (completed === total && callback) {
            callback();
          }
        });
      } else {
        completed++;
        if (completed === total && callback) {
          setImmediate(callback);
        }
      }
    });
  }

  /**
   * End all streams
   */
  end(): void {
    this.flush(() => {
      this.streams.forEach(entry => {
        if (entry.stream.end) {
          entry.stream.end();
        }
      });
    });
  }

  /**
   * Event listening (forwarded to all streams)
   */
  on(event: string, handler: (...args: unknown[]) => void): void {
    this.streams.forEach(entry => {
      if (entry.stream.on) {
        entry.stream.on(event, handler);
      }
    });
  }

  /**
   * Remove event listener (from all streams)
   */
  off(event: string, handler: (...args: unknown[]) => void): void {
    this.streams.forEach(entry => {
      if (entry.stream.off) {
        entry.stream.off(event, handler);
      }
    });
  }

  /**
   * Cloning multistream
   */
  clone(level?: LogLevel): Multistream {
    const cloned = new Multistream([], {
      dedupe: this.dedupe,
      levels: { ...this.streamLevels },
    });

    this.streams.forEach(entry => {
      cloned.add({
        stream: entry.stream,
        level: level ?? entry.level,
        levelVal: level ? this.streamLevels[level] : entry.levelVal,
      });
    });

    return cloned;
  }

  /**
   * Get the number of streams
   */
  get count(): number {
    return this.streams.length;
  }
}

/**
 * Create multistream
 */
export function createMultistream(
  streams: StreamEntry[] = [],
  options: MultistreamOptions = {},
): LogStream {
  return new Multistream(streams, options);
}
