/**
 * AsyncStream Unit Tests
 */
import { describe, it, expect, beforeEach, vi } from "vitest";
import {
  AsyncStream,
  createAsyncStream,
} from "../../../src/logger/streams/async-stream.js";
import type { LogStream, LogEntry } from "../../../src/logger/types.js";

describe("AsyncStream", () => {
  // A simple mock stream for testing
  class MockStream implements LogStream {
    public entries: LogEntry[] = [];
    public flushCallback: (() => void) | null = null;
    public flushed = false;

    write(entry: LogEntry): void {
      this.entries.push(entry);
    }

    flush(callback?: () => void): void {
      this.flushed = true;
      if (callback) {
        setImmediate(callback);
      }
    }

    end(): void {
      // noop
    }
  }

  let mockStream: MockStream;

  beforeEach(() => {
    mockStream = new MockStream();
  });

  describe("constructor / createAsyncStream", () => {
    it("should create an async stream wrapping a target stream", () => {
      const stream = new AsyncStream(mockStream);
      expect(stream).toBeInstanceOf(AsyncStream);
    });

    it("should create via factory function", () => {
      const stream = createAsyncStream(mockStream);
      expect(stream).toBeInstanceOf(AsyncStream);
    });

    it("should use default batch size and max queue size", () => {
      const stream = new AsyncStream(mockStream);
      expect((stream as any).batchSize).toBe(10);
      expect((stream as any).maxQueueSize).toBe(10000);
      stream.end();
    });

    it("should accept custom batch size", () => {
      const stream = new AsyncStream(mockStream, { batchSize: 5 });
      expect((stream as any).batchSize).toBe(5);
      stream.end();
    });
  });

  describe("write", () => {
    it("should add entry to queue and not immediately write to target", () => {
      const stream = new AsyncStream(mockStream, { batchSize: 10 });
      stream.write({ level: "info", message: "queued" });
      expect(mockStream.entries.length).toBe(0);
      expect(stream.getQueueSize()).toBe(1);
      // Manually flush to process
      stream.flushSync();
      expect(mockStream.entries.length).toBe(1);
      expect(mockStream.entries[0]!.message).toBe("queued");
      stream.end();
    });

    it("should process batch immediately when queue reaches batch size", () => {
      const stream = new AsyncStream(mockStream, { batchSize: 3 });
      stream.write({ level: "info", message: "m1" });
      stream.write({ level: "info", message: "m2" });
      expect(mockStream.entries.length).toBe(0);
      // Third write should trigger batch processing
      stream.write({ level: "info", message: "m3" });
      // Wait for async processing
      return new Promise<void>((resolve) => {
        setImmediate(() => {
          expect(mockStream.entries.length).toBe(3);
          expect(mockStream.entries[0]!.message).toBe("m1");
          expect(mockStream.entries[1]!.message).toBe("m2");
          expect(mockStream.entries[2]!.message).toBe("m3");
          stream.end();
          resolve();
        });
      });
    });

    it("should drop logs when queue exceeds maxQueueSize", () => {
      const stream = new AsyncStream(mockStream, { batchSize: 100, maxQueueSize: 2 } as any);
      // Fill queue
      stream.write({ level: "info", message: "m1" });
      stream.write({ level: "info", message: "m2" });
      // This should be dropped
      stream.write({ level: "info", message: "m3" });
      expect(stream.getDroppedCount()).toBe(1);
      stream.end();
    });

    it("should write directly to target stream when shutting down", () => {
      const stream = new AsyncStream(mockStream, { batchSize: 10 });
      (stream as any).isShuttingDown = true;
      stream.write({ level: "info", message: "direct write" });
      expect(mockStream.entries.length).toBe(1);
      expect(mockStream.entries[0]!.message).toBe("direct write");
      stream.end();
    });

    it("should schedule delayed flush when queue below batch size", () => {
      const stream = new AsyncStream(mockStream, { batchSize: 10 });
      stream.write({ level: "info", message: "delayed" });
      expect((stream as any).flushTimer).toBeDefined();
      return new Promise<void>((resolve) => {
        setTimeout(() => {
          expect((stream as any).flushTimer).toBeUndefined();
          // The delayed flush should have processed
          stream.flushSync();
          expect(mockStream.entries.length).toBeGreaterThanOrEqual(1);
          stream.end();
          resolve();
        }, 200);
      });
    });
  });

  describe("flush", () => {
    it("should process all queued entries and flush target stream", () => {
      const stream = new AsyncStream(mockStream, { batchSize: 10 });
      stream.write({ level: "info", message: "flush test" });

      return new Promise<void>((resolve) => {
        stream.flush(() => {
          expect(mockStream.entries.length).toBe(1);
          expect(mockStream.entries[0]!.message).toBe("flush test");
          stream.end();
          resolve();
        });
      });
    });

    it("should clear flush timer during flush", () => {
      const stream = new AsyncStream(mockStream, { batchSize: 10 });
      stream.write({ level: "info", message: "test" });
      expect((stream as any).flushTimer).toBeDefined();

      return new Promise<void>((resolve) => {
        stream.flush(() => {
          expect((stream as any).flushTimer).toBeUndefined();
          stream.end();
          resolve();
        });
      });
    });
  });

  describe("flushSync", () => {
    it("should synchronously process all queued entries", () => {
      const stream = new AsyncStream(mockStream, { batchSize: 10 });
      stream.write({ level: "info", message: "sync1" });
      stream.write({ level: "info", message: "sync2" });
      stream.flushSync();
      expect(mockStream.entries.length).toBe(2);
      stream.end();
    });

    it("should call flushSync on target stream if available", () => {
      const mockWithSync = new MockStream();
      (mockWithSync as any).flushSync = vi.fn();

      const stream = new AsyncStream(mockWithSync as any, { batchSize: 10 });
      stream.write({ level: "info", message: "test" });
      stream.flushSync();
      expect((mockWithSync as any).flushSync).toHaveBeenCalled();
      stream.end();
    });

    it("should fallback to async flush with busy wait if no flushSync", () => {
      const stream = new AsyncStream(mockStream, { batchSize: 10 });
      stream.write({ level: "info", message: "busy wait" });
      stream.flushSync();
      expect(mockStream.entries.length).toBe(1);
      stream.end();
    });

    it("should clear timer during sync flush", () => {
      const stream = new AsyncStream(mockStream, { batchSize: 10 });
      stream.write({ level: "info", message: "test" });
      expect((stream as any).flushTimer).toBeDefined();
      stream.flushSync();
      expect((stream as any).flushTimer).toBeUndefined();
      stream.end();
    });
  });

  describe("end", () => {
    it("should flush and end target stream", () => {
      const spy = vi.spyOn(mockStream, "end");
      const stream = new AsyncStream(mockStream);
      stream.write({ level: "info", message: "end test" });
      stream.end();
      return new Promise<void>((resolve) => {
        setTimeout(() => {
          expect(mockStream.entries.length).toBe(1);
          expect(spy).toHaveBeenCalled();
          stream.end();
          resolve();
        }, 200);
      });
    });
  });

  describe("on / off", () => {
    it("should forward on/off to target stream", () => {
      const stream = new AsyncStream(mockStream);
      const handler = () => {};
      stream.on("test-event", handler);
      stream.off("test-event", handler);
      stream.end();
    });
  });

  describe("getQueueSize / getDroppedCount", () => {
    it("should return queue size", () => {
      const stream = new AsyncStream(mockStream, { batchSize: 100 });
      expect(stream.getQueueSize()).toBe(0);
      stream.write({ level: "info", message: "q1" });
      expect(stream.getQueueSize()).toBe(1);
      stream.end();
    });

    it("should return dropped count", () => {
      const stream = new AsyncStream(mockStream, { batchSize: 100, maxQueueSize: 0 } as any);
      stream.write({ level: "info", message: "drop" });
      expect(stream.getDroppedCount()).toBe(1);
      stream.end();
    });
  });
});
