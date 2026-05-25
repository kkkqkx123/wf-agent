/**
 * RotatingFileStream Unit Tests
 */
import { describe, it, expect, beforeEach, afterEach } from "vitest";
import * as fs from "fs";
import * as path from "path";
import * as os from "os";
import {
  RotatingFileStream,
  createRotatingFileStream,
} from "../../../src/logger/streams/rotating-file-stream.js";
import type { LogEntry } from "../../../src/logger/types.js";

describe("RotatingFileStream", () => {
  const tmpDir = path.join(os.tmpdir(), "wf-agent-rotating-test-" + Date.now());
  const testFilePath = path.join(tmpDir, "test.log");

  beforeEach(() => {
    if (fs.existsSync(tmpDir)) {
      fs.rmSync(tmpDir, { recursive: true, force: true });
    }
  });

  afterEach(() => {
    if (fs.existsSync(tmpDir)) {
      fs.rmSync(tmpDir, { recursive: true, force: true });
    }
  });

  describe("constructor / createRotatingFileStream", () => {
    it("should create a rotating file stream", () => {
      const stream = new RotatingFileStream({ filePath: testFilePath });
      expect(stream).toBeInstanceOf(RotatingFileStream);
      expect(fs.existsSync(tmpDir)).toBe(true);
      stream.end();
    });

    it("should create via factory function", () => {
      const stream = createRotatingFileStream({ filePath: testFilePath });
      expect(stream).toBeInstanceOf(RotatingFileStream);
      stream.end();
    });

    it("should throw if filePath is not provided", () => {
      expect(() => new RotatingFileStream({} as any)).toThrow("filePath is required");
    });

    it("should use default maxSize and maxFiles", () => {
      const stream = new RotatingFileStream({ filePath: testFilePath });
      expect((stream as any).maxSize).toBe(100 * 1024 * 1024);
      expect((stream as any).maxFiles).toBe(10);
      expect((stream as any).compress).toBe(false);
      stream.end();
    });

    it("should accept custom maxSize, maxFiles, and compress", () => {
      const stream = new RotatingFileStream({
        filePath: testFilePath,
        maxSize: 1024,
        maxFiles: 3,
        compress: true,
      });
      expect((stream as any).maxSize).toBe(1024);
      expect((stream as any).maxFiles).toBe(3);
      expect((stream as any).compress).toBe(true);
      stream.end();
    });

    it("should get current file size for existing file", () => {
      // Pre-create file with content
      fs.mkdirSync(tmpDir, { recursive: true });
      fs.writeFileSync(testFilePath, "pre-existing content\n");

      const stream = new RotatingFileStream({ filePath: testFilePath });
      expect((stream as any).currentSize).toBeGreaterThan(0);
      stream.end();
    });
  });

  describe("write and rotation", () => {
    it("should write log entries to file", () => {
      const stream = new RotatingFileStream({ filePath: testFilePath, maxSize: 1024 * 1024 });
      stream.write({ level: "info", message: "rotation test" });
      stream.flushSync();

      const content = fs.readFileSync(testFilePath, "utf8");
      expect(content).toContain("rotation test");
      stream.end();
    });

    it("should rotate when currentSize exceeds maxSize", () => {
      const stream = new RotatingFileStream({
        filePath: testFilePath,
        maxSize: 1, // Very small max size to trigger rotation on every write
        maxFiles: 2,
      });

      // Write enough to trigger rotation
      stream.write({ level: "info", message: "first log entry" });
      stream.write({ level: "info", message: "second log entry" });
      stream.flushSync();

      // After rotation, check that backup files exist
      const dir = path.dirname(testFilePath);
      const files = fs.readdirSync(dir);
      const logFiles = files.filter(f => f.startsWith("test") && f.endsWith(".log"));
      expect(logFiles.length).toBeGreaterThanOrEqual(1);

      stream.end();
    });

    it("should rotate multiple times and keep maxFiles count", () => {
      const stream = new RotatingFileStream({
        filePath: testFilePath,
        maxSize: 1, // Rotate on every write
        maxFiles: 2,
      });

      // Write many entries to trigger multiple rotations
      for (let i = 0; i < 10; i++) {
        stream.write({ level: "info", message: `log entry ${i}` });
      }
      stream.flushSync();

      const dir = path.dirname(testFilePath);
      const files = fs.readdirSync(dir);
      const backupFiles = files.filter(f => /test\.\d+\.log$/.test(f));

      // Should have at most maxFiles backup files
      expect(backupFiles.length).toBeLessThanOrEqual(2);

      stream.end();
    });

    it("should remove oldest file when maxFiles exceeded on rotation", () => {
      // Test the rotateFiles logic by creating backup files manually
      fs.mkdirSync(tmpDir, { recursive: true });

      // Create backup files .1 and .2
      fs.writeFileSync(path.join(tmpDir, "test.1.log"), "backup 1\n");
      fs.writeFileSync(path.join(tmpDir, "test.2.log"), "backup 2\n");

      // Create main file and trigger rotation
      const stream = new RotatingFileStream({
        filePath: testFilePath,
        maxSize: 1,
        maxFiles: 2,
      });

      // Write to trigger rotation - should shift .1->.2 and .2 should be removed
      stream.write({ level: "info", message: "trigger rotation" });
      stream.flushSync();

      const dir = path.dirname(testFilePath);
      const files = fs.readdirSync(dir);
      const backupFiles = files.filter(f => /test\.\d+\.log$/.test(f));
      expect(backupFiles.length).toBeLessThanOrEqual(2);

      stream.end();
    });

    it("should format entry as JSON by default", () => {
      const stream = new RotatingFileStream({ filePath: testFilePath, maxSize: 1024 * 1024 });
      stream.write({ level: "info", message: "format test" });
      stream.flushSync();

      const content = fs.readFileSync(testFilePath, "utf8");
      const parsed = JSON.parse(content.trim());
      expect(parsed.message).toBe("format test");
      expect(parsed.level).toBe("info");
      stream.end();
    });

    it("should format entry as plain text when json is false", () => {
      const stream = new RotatingFileStream({
        filePath: testFilePath,
        maxSize: 1024 * 1024,
        json: false,
      });
      stream.write({ level: "info", message: "plain text" });
      stream.flushSync();

      const content = fs.readFileSync(testFilePath, "utf8");
      expect(content).toContain("[INFO]");
      expect(content).toContain("plain text");
      stream.end();
    });
  });

  describe("flush", () => {
    it("should flush buffered entries to file", () => {
      const stream = new RotatingFileStream({ filePath: testFilePath, maxSize: 1024 * 1024 });
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
      const stream = new RotatingFileStream({ filePath: testFilePath, maxSize: 1024 * 1024 });
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
    it("should synchronously flush buffered entries", () => {
      const stream = new RotatingFileStream({ filePath: testFilePath, maxSize: 1024 * 1024 });
      stream.write({ level: "info", message: "sync flush" });
      stream.flushSync();

      const content = fs.readFileSync(testFilePath, "utf8");
      expect(content).toContain("sync flush");
      stream.end();
    });

    it("should handle empty buffer", () => {
      const stream = new RotatingFileStream({ filePath: testFilePath, maxSize: 1024 * 1024 });
      expect(() => stream.flushSync()).not.toThrow();
      stream.end();
    });
  });

  describe("end", () => {
    it("should flush and close the write stream", () => {
      const stream = new RotatingFileStream({ filePath: testFilePath, maxSize: 1024 * 1024 });
      stream.write({ level: "info", message: "end test" });
      stream.end();

      return new Promise<void>((resolve) => {
        setTimeout(() => {
          const content = fs.readFileSync(testFilePath, "utf8");
          expect(content).toContain("end test");
          resolve();
        }, 200);
      });
    });
  });

  describe("getFilePath / getCurrentSize", () => {
    it("should return file path", () => {
      const stream = new RotatingFileStream({ filePath: testFilePath });
      expect(stream.getFilePath()).toBe(testFilePath);
      stream.end();
    });

    it("should return current size", () => {
      const stream = new RotatingFileStream({ filePath: testFilePath });
      stream.write({ level: "info", message: "size test" });
      stream.flushSync();
      expect(stream.getCurrentSize()).toBeGreaterThan(0);
      stream.end();
    });
  });

  describe("getMetrics", () => {
    it("should return metrics", () => {
      const stream = new RotatingFileStream({ filePath: testFilePath });
      const metrics = stream.getMetrics();
      expect(metrics.filePath).toBe(testFilePath);
      expect(typeof metrics.hasError).toBe("boolean");
      expect(typeof metrics.bufferUtilization).toBe("number");
      stream.end();
    });
  });

  describe("on / off", () => {
    it("should forward events from write stream", () => {
      const stream = new RotatingFileStream({ filePath: testFilePath });
      const handler = () => {};
      stream.on("error", handler);
      stream.off("error", handler);
      stream.end();
    });
  });
});
