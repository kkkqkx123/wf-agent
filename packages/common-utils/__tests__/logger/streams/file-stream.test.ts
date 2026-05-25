/**
 * FileStream Unit Tests
 */
import { describe, it, expect, beforeEach, afterEach } from "vitest";
import * as fs from "fs";
import * as path from "path";
import * as os from "os";
import {
  FileStream,
  createFileStream,
} from "../../../src/logger/streams/file-stream.js";
import type { LogEntry } from "../../../src/logger/types.js";

describe("FileStream", () => {
  const tmpDir = path.join(os.tmpdir(), "wf-agent-file-stream-test-" + Date.now());
  const testFilePath = path.join(tmpDir, "test.log");

  beforeEach(() => {
    // Clean up before each test
    if (fs.existsSync(tmpDir)) {
      fs.rmSync(tmpDir, { recursive: true, force: true });
    }
  });

  afterEach(() => {
    // Clean up after each test
    if (fs.existsSync(tmpDir)) {
      fs.rmSync(tmpDir, { recursive: true, force: true });
    }
  });

  describe("constructor / createFileStream", () => {
    it("should create a file stream and create directory", () => {
      const stream = new FileStream({ filePath: testFilePath });
      expect(stream).toBeInstanceOf(FileStream);
      expect(fs.existsSync(tmpDir)).toBe(true);
      stream.end();
    });

    it("should create via factory function", () => {
      const stream = createFileStream({ filePath: testFilePath });
      expect(stream).toBeInstanceOf(FileStream);
      stream.end();
    });

    it("should throw if filePath is not provided", () => {
      expect(() => new FileStream({} as any)).toThrow("filePath is required");
    });

    it("should create write stream in append mode by default", () => {
      const stream = new FileStream({ filePath: testFilePath });
      stream.write({ level: "info", message: "line 1" });
      stream.flushSync();
      stream.end();

      const stream2 = new FileStream({ filePath: testFilePath });
      stream2.write({ level: "info", message: "line 2" });
      stream2.flushSync();
      stream2.end();

      const content = fs.readFileSync(testFilePath, "utf8");
      expect(content).toContain("line 1");
      expect(content).toContain("line 2");
    });

    it("should overwrite file when append is false", () => {
      // First write with append
      const stream = new FileStream({ filePath: testFilePath, append: true });
      stream.write({ level: "info", message: "line 1" });
      stream.end(); // use async end → goes through write stream

      return new Promise<void>((resolve1) => {
        setTimeout(() => {
          // Second write with overwrite - "w" flag truncates file at WriteStream creation
          const stream2 = new FileStream({ filePath: testFilePath, append: false });
          stream2.write({ level: "info", message: "line 2" });
          stream2.end();

          setTimeout(() => {
            const content = fs.readFileSync(testFilePath, "utf8");
            // The WriteStream with "w" flag truncates the file,
            // and the buffered write goes through the WriteStream
            const lines = content.trim().split("\n");
            expect(lines.length).toBe(1);
            expect(lines[0]).toContain("line 2");
            resolve1();
          }, 200);
        }, 200);
      });
    });
  });

  describe("write", () => {
    it("should buffer log entries and flush to file", () => {
      const stream = new FileStream({ filePath: testFilePath, maxBufferSize: 1024 });
      stream.write({ level: "info", message: "hello world" });
      stream.flushSync();

      const content = fs.readFileSync(testFilePath, "utf8");
      expect(content).toContain("hello world");
      stream.end();
    });

    it("should format entry as JSON by default", () => {
      const stream = new FileStream({ filePath: testFilePath });
      stream.write({ level: "info", message: "json test" });
      stream.flushSync();

      const content = fs.readFileSync(testFilePath, "utf8");
      const parsed = JSON.parse(content.trim());
      expect(parsed.message).toBe("json test");
      expect(parsed.level).toBe("info");
      stream.end();
    });

    it("should format entry as plain text when json is false", () => {
      const stream = new FileStream({ filePath: testFilePath, json: false });
      stream.write({ level: "info", message: "plain text" });
      stream.flushSync();

      const content = fs.readFileSync(testFilePath, "utf8");
      expect(content).toContain("[INFO]");
      expect(content).toContain("plain text");
      stream.end();
    });

    it("should auto-flush when buffer exceeds maxBufferSize", () => {
      const stream = new FileStream({
        filePath: testFilePath,
        maxBufferSize: 1, // Very small buffer triggers immediate flush
      });
      stream.write({ level: "info", message: "auto flush test" });

      // Give time for async flush
      return new Promise<void>((resolve) => {
        setTimeout(() => {
          const content = fs.readFileSync(testFilePath, "utf8");
          expect(content).toContain("auto flush test");
          stream.end();
          resolve();
        }, 200);
      });
    });

    it("should drop logs when in error state and no fallback", () => {
      const stream = new FileStream({ filePath: testFilePath, enableFallback: false });
      // Force error state
      (stream as any).hasError = true;
      stream.write({ level: "info", message: "dropped" });
      stream.flushSync();

      // No file should be created since the log was dropped
      expect(fs.existsSync(testFilePath)).toBe(false);
      stream.end();
    });
  });

  describe("flush", () => {
    it("should flush buffered entries to file", () => {
      const stream = new FileStream({ filePath: testFilePath });
      stream.write({ level: "info", message: "flush test" });

      return new Promise<void>((resolve) => {
        stream.flush(() => {
          const content = fs.readFileSync(testFilePath, "utf8");
          expect(content).toContain("flush test");
          stream.end();
          resolve();
        });
      });
    });

    it("should call callback immediately when buffer is empty", () => {
      const stream = new FileStream({ filePath: testFilePath });
      return new Promise<void>((resolve) => {
        stream.flush(() => {
          resolve();
        });
      }).then(() => {
        stream.end();
      });
    });
  });

  describe("flushSync", () => {
    it("should synchronously flush buffered entries to file", () => {
      const stream = new FileStream({ filePath: testFilePath });
      stream.write({ level: "info", message: "sync flush test" });
      stream.flushSync();

      const content = fs.readFileSync(testFilePath, "utf8");
      expect(content).toContain("sync flush test");
      stream.end();
    });

    it("should handle empty buffer gracefully", () => {
      const stream = new FileStream({ filePath: testFilePath });
      expect(() => stream.flushSync()).not.toThrow();
      stream.end();
    });
  });

  describe("end", () => {
    it("should flush and close the write stream", () => {
      const stream = new FileStream({ filePath: testFilePath });
      stream.write({ level: "info", message: "end test" });
      stream.end();

      // Give time for flush in end callback
      return new Promise<void>((resolve) => {
        setTimeout(() => {
          const content = fs.readFileSync(testFilePath, "utf8");
          expect(content).toContain("end test");
          resolve();
        }, 200);
      });
    });
  });

  describe("getMetrics", () => {
    it("should return metrics object", () => {
      const stream = new FileStream({ filePath: testFilePath });
      const metrics = stream.getMetrics();
      expect(metrics).toHaveProperty("filePath");
      expect(metrics).toHaveProperty("hasError");
      expect(metrics).toHaveProperty("droppedLogsCount");
      expect(metrics).toHaveProperty("bufferSize");
      expect(metrics).toHaveProperty("maxBufferSize");
      expect(metrics).toHaveProperty("bufferUtilization");
      expect(metrics.filePath).toBe(testFilePath);
      stream.end();
    });
  });

  describe("resetErrorState", () => {
    it("should reset error state", () => {
      const stream = new FileStream({ filePath: testFilePath, enableFallback: false });
      (stream as any).hasError = true;
      (stream as any).droppedLogsCount = 5;

      stream.resetErrorState();
      expect((stream as any).hasError).toBe(false);
      expect((stream as any).droppedLogsCount).toBe(0);
      stream.end();
    });
  });

  describe("on / off", () => {
    it("should forward events from write stream", () => {
      const stream = new FileStream({ filePath: testFilePath });
      const handler = vi.fn();
      stream.on("error", handler);
      stream.off("error", handler);
      expect(handler).not.toHaveBeenCalled();
      stream.end();
    });
  });
});
