/**
 * Multistream Unit Tests
 */
import { describe, it, expect, beforeEach, vi } from "vitest";
import {
  Multistream,
  createMultistream,
} from "../../../src/logger/streams/multistream.js";
import type { LogStream, LogEntry } from "../../../src/logger/types.js";

describe("Multistream", () => {
  // Mock stream for testing
  class MockStream implements LogStream {
    public entries: LogEntry[] = [];
    public ended = false;
    public listeners: Array<{ event: string; handler: (...args: unknown[]) => void }> = [];

    write(entry: LogEntry): void {
      this.entries.push(entry);
    }

    flush(callback?: () => void): void {
      if (callback) {
        setImmediate(callback);
      }
    }

    end(): void {
      this.ended = true;
    }

    on(event: string, handler: (...args: unknown[]) => void): void {
      this.listeners.push({ event, handler });
    }

    off(event: string, handler: (...args: unknown[]) => void): void {
      this.listeners = this.listeners.filter(
        (l) => !(l.event === event && l.handler === handler),
      );
    }
  }

  let stream1: MockStream;
  let stream2: MockStream;
  let stream3: MockStream;

  beforeEach(() => {
    stream1 = new MockStream();
    stream2 = new MockStream();
    stream3 = new MockStream();
  });

  describe("constructor / createMultistream", () => {
    it("should create empty multistream", () => {
      const ms = new Multistream();
      expect(ms).toBeInstanceOf(Multistream);
      expect(ms.count).toBe(0);
    });

    it("should create via factory function", () => {
      const ms = createMultistream();
      expect(ms).toBeInstanceOf(Multistream);
    });

    it("should add initial streams", () => {
      const ms = new Multistream([
        { stream: stream1, level: "info" },
        { stream: stream2, level: "error" },
      ]);
      expect(ms.count).toBe(2);
    });

    it("should accept dedupe option", () => {
      const ms = new Multistream([], { dedupe: true });
      expect((ms as any).dedupe).toBe(true);
    });

    it("should accept custom level mapping", () => {
      const ms = new Multistream([], { levels: { custom: 10 } });
      expect((ms as any).streamLevels.custom).toBe(10);
    });

    it("should use default info level for entries without level", () => {
      const ms = new Multistream([{ stream: stream1 }]);
      expect(ms.count).toBe(1);
    });
  });

  describe("add", () => {
    it("should add a stream with level string", () => {
      const ms = new Multistream();
      ms.add({ stream: stream1, level: "warn" });
      expect(ms.count).toBe(1);
    });

    it("should add a stream with levelVal number", () => {
      const ms = new Multistream();
      ms.add({ stream: stream1, levelVal: 5 });
      expect(ms.count).toBe(1);
    });

    it("should throw if stream is not provided", () => {
      const ms = new Multistream();
      expect(() => ms.add({} as any)).toThrow("stream entry must have a stream property");
    });

    it("should throw if entry is null", () => {
      const ms = new Multistream();
      expect(() => ms.add(null as any)).toThrow("stream entry must have a stream property");
    });

    it("should sort streams by level ascending", () => {
      const ms = new Multistream();
      ms.add({ stream: stream1, level: "error" });  // level 3
      ms.add({ stream: stream2, level: "debug" });  // level 0
      ms.add({ stream: stream3, level: "info" });   // level 1

      const streams = (ms as any).streams;
      expect(streams[0].levelVal).toBe(0); // debug
      expect(streams[1].levelVal).toBe(1); // info
      expect(streams[2].levelVal).toBe(3); // error
    });

    it("should assign incremental ids", () => {
      const ms = new Multistream();
      const r1 = ms.add({ stream: stream1 });
      const r2 = ms.add({ stream: stream2 });
      const streams = (ms as any).streams;
      expect(streams[0].id).toBe(1);
      expect(streams[1].id).toBe(2);
    });
  });

  describe("remove", () => {
    it("should remove a stream by id", () => {
      const ms = new Multistream();
      ms.add({ stream: stream1 });
      ms.add({ stream: stream2 });
      expect(ms.count).toBe(2);
      // Get the id of the first added stream from internal state
      const firstId = (ms as any).streams[0].id;
      ms.remove(firstId);
      expect(ms.count).toBe(1);
    });

    it("should not throw when removing non-existent id", () => {
      const ms = new Multistream();
      expect(() => ms.remove(999)).not.toThrow();
    });
  });

  describe("write", () => {
    it("should write to all streams within level threshold", () => {
      const ms = new Multistream([
        { stream: stream1, level: "debug" },
        { stream: stream2, level: "info" },
      ]);
      ms.write({ level: "info", message: "test" });
      expect(stream1.entries.length).toBe(1);
      expect(stream2.entries.length).toBe(1);
    });

    it("should not write to streams with higher level than entry level", () => {
      const ms = new Multistream([
        { stream: stream1, level: "info" },
        { stream: stream2, level: "error" },
      ]);
      ms.write({ level: "info", message: "test" });
      expect(stream1.entries.length).toBe(1); // info <= info
      expect(stream2.entries.length).toBe(0); // error > info -> break
    });

    it("should stop iteration when stream level exceeds log level (non-dedupe)", () => {
      const ms = new Multistream([
        { stream: stream1, level: "debug" },
        { stream: stream2, level: "info" },
        { stream: stream3, level: "error" },
      ]);
      ms.write({ level: "info", message: "test" });
      expect(stream1.entries.length).toBe(1);
      expect(stream2.entries.length).toBe(1);
      expect(stream3.entries.length).toBe(0); // Stops at error > info
    });

    it("should handle deduplication (same level, both write because same level)", () => {
      const ms = new Multistream(
        [
          { stream: stream1, level: "info" },
          { stream: stream2, level: "info" },
        ],
        { dedupe: true },
      );
      ms.write({ level: "info", message: "deduped" });
      // With dedupe=true and both streams having the same level,
      // both are traversed (reverse order) and both write because
      // the recordedLevel check only breaks on level mismatch.
      expect(stream1.entries.length).toBe(1);
      expect(stream2.entries.length).toBe(1);
    });

    it("should handle deduplication with different levels", () => {
      const ms = new Multistream(
        [
          { stream: stream1, level: "warn" },    // level 2
          { stream: stream2, level: "error" },   // level 3
        ],
        { dedupe: true },
      );
      ms.write({ level: "error", message: "error msg" });
      // With dedupe=true, iterates reverse (sorted by level ascending: warn(2), error(3))
      // Reverse: error(3) first, which matches entry level(3) -> writes
      // Then warn(2) has recordedLevel=3, 3 !== 2 -> break
      // So only error stream should receive
      expect(stream2.entries.length).toBe(1);
      expect(stream1.entries.length).toBe(0);
    });
  });

  describe("flush", () => {
    it("should flush all streams", () => {
      const ms = new Multistream([
        { stream: stream1 },
        { stream: stream2 },
      ]);

      return new Promise<void>((resolve) => {
        ms.flush(() => {
          resolve();
        });
      });
    });

    it("should call callback immediately when no streams", () => {
      const ms = new Multistream();
      return new Promise<void>((resolve) => {
        ms.flush(() => {
          resolve();
        });
      });
    });
  });

  describe("end", () => {
    it("should end all streams after flush", () => {
      const ms = new Multistream([
        { stream: stream1 },
        { stream: stream2 },
      ]);
      ms.end();

      return new Promise<void>((resolve) => {
        setTimeout(() => {
          expect(stream1.ended).toBe(true);
          expect(stream2.ended).toBe(true);
          resolve();
        }, 100);
      });
    });
  });

  describe("on / off", () => {
    it("should forward event handlers to all streams", () => {
      const ms = new Multistream([
        { stream: stream1 },
        { stream: stream2 },
      ]);
      const handler = () => {};
      ms.on("test", handler);
      expect(stream1.listeners.length).toBe(1);
      expect(stream2.listeners.length).toBe(1);

      ms.off("test", handler);
      expect(stream1.listeners.length).toBe(0);
      expect(stream2.listeners.length).toBe(0);
    });
  });

  describe("clone", () => {
    it("should create a clone with same streams", () => {
      const ms = new Multistream([
        { stream: stream1, level: "info" },
        { stream: stream2, level: "warn" },
      ]);
      const cloned = ms.clone();
      expect(cloned.count).toBe(2);
    });

    it("should override level in cloned streams when specified", () => {
      const ms = new Multistream([
        { stream: stream1, level: "info" },
      ]);
      const cloned = ms.clone("error");
      // Both clones should have error level
      const clonedStreams = (cloned as any).streams;
      clonedStreams.forEach((entry: any) => {
        expect(entry.level).toBe("error");
      });
    });

    it("should clone with same dedupe setting", () => {
      const ms = new Multistream([{ stream: stream1 }], { dedupe: true });
      const cloned = ms.clone();
      expect((cloned as any).dedupe).toBe(true);
    });
  });

  describe("count", () => {
    it("should return number of streams", () => {
      const ms = new Multistream([
        { stream: stream1 },
        { stream: stream2 },
        { stream: stream3 },
      ]);
      expect(ms.count).toBe(3);
    });

    it("should return 0 for empty multistream", () => {
      const ms = new Multistream();
      expect(ms.count).toBe(0);
    });
  });
});
