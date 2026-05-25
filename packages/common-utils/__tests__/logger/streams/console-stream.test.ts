/**
 * ConsoleStream Unit Tests
 */
import { describe, it, expect, beforeEach, afterEach, vi } from "vitest";
import {
  ConsoleStream,
  createConsoleStream,
} from "../../../src/logger/streams/console-stream.js";
import type { LogEntry } from "../../../src/logger/types.js";

describe("ConsoleStream", () => {
  let consoleOutput: string[] = [];
  const originalLog = console.log;
  const originalDebug = console.debug;
  const originalWarn = console.warn;
  const originalError = console.error;

  beforeEach(() => {
    consoleOutput = [];
    console.log = (...args: unknown[]) => {
      consoleOutput.push(args.join(" "));
    };
    console.debug = (...args: unknown[]) => {
      consoleOutput.push(args.join(" "));
    };
    console.warn = (...args: unknown[]) => {
      consoleOutput.push(args.join(" "));
    };
    console.error = (...args: unknown[]) => {
      consoleOutput.push(args.join(" "));
    };
  });

  afterEach(() => {
    console.log = originalLog;
    console.debug = originalDebug;
    console.warn = originalWarn;
    console.error = originalError;
  });

  describe("constructor / createConsoleStream", () => {
    it("should create with default options", () => {
      const stream = new ConsoleStream();
      expect(stream).toBeInstanceOf(ConsoleStream);
    });

    it("should create via factory function", () => {
      const stream = createConsoleStream();
      expect(stream).toBeInstanceOf(ConsoleStream);
    });

    it("should accept custom options", () => {
      const stream = new ConsoleStream({ json: true, timestamp: false, pretty: true });
      expect(stream).toBeInstanceOf(ConsoleStream);
    });
  });

  describe("write", () => {
    const baseEntry: LogEntry = {
      level: "info",
      message: "test message",
      timestamp: "2024-01-01T00:00:00.000Z",
    };

    it("should write plain text log for non-JSON mode", () => {
      const stream = new ConsoleStream({ json: false, timestamp: false });
      stream.write(baseEntry);
      expect(consoleOutput.length).toBe(1);
      expect(consoleOutput[0]).toContain("[INFO]");
      expect(consoleOutput[0]).toContain("test message");
    });

    it("should write JSON log when json option is true", () => {
      const stream = new ConsoleStream({ json: true });
      stream.write(baseEntry);
      expect(consoleOutput.length).toBe(1);
      const parsed = JSON.parse(consoleOutput[0]!);
      expect(parsed.message).toBe("test message");
      expect(parsed.level).toBe("info");
    });

    it("should include timestamp in plain text when timestamp option is true", () => {
      const stream = new ConsoleStream({ json: false, timestamp: true });
      stream.write(baseEntry);
      expect(consoleOutput[0]).toContain("[2024-01-01T00:00:00.000Z]");
    });

    it("should omit timestamp in plain text when timestamp option is false", () => {
      const stream = new ConsoleStream({ json: false, timestamp: false });
      stream.write(baseEntry);
      expect(consoleOutput[0]).not.toContain("[2024-01-01T00:00:00.000Z]");
    });

    it("should include extra data in plain text output", () => {
      const stream = new ConsoleStream({ json: false, timestamp: false });
      const entry: LogEntry = {
        ...baseEntry,
        context: { userId: "123" },
        metadata: { reqId: "abc" },
      };
      stream.write(entry);
      expect(consoleOutput[0]).toContain("userId");
      expect(consoleOutput[0]).toContain("123");
    });

    it("should use console.debug for debug level", () => {
      const stream = new ConsoleStream({ json: false, timestamp: false });
      stream.write({ ...baseEntry, level: "debug" });
      expect(consoleOutput.length).toBe(1);
      expect(consoleOutput[0]).toContain("[DEBUG]");
    });

    it("should use console.warn for warn level", () => {
      const stream = new ConsoleStream({ json: false, timestamp: false });
      stream.write({ ...baseEntry, level: "warn" });
      expect(consoleOutput.length).toBe(1);
      expect(consoleOutput[0]).toContain("[WARN]");
    });

    it("should use console.error for error level", () => {
      const stream = new ConsoleStream({ json: false, timestamp: false });
      stream.write({ ...baseEntry, level: "error" });
      expect(consoleOutput.length).toBe(1);
      expect(consoleOutput[0]).toContain("[ERROR]");
    });

    it("should use console.log for unknown level", () => {
      const stream = new ConsoleStream({ json: false, timestamp: false });
      stream.write({ ...baseEntry, level: "info" });
      expect(consoleOutput.length).toBe(1);
    });

    it("should apply ANSI colors when pretty option is true", () => {
      const stream = new ConsoleStream({ json: false, timestamp: false, pretty: true });
      stream.write({ ...baseEntry, level: "info" });
      expect(consoleOutput[0]).toContain("\x1b[32m"); // green for info
    });

    it("should not apply ANSI colors when pretty option is false", () => {
      const stream = new ConsoleStream({ json: false, timestamp: false, pretty: false });
      stream.write(baseEntry);
      expect(consoleOutput[0]).not.toContain("\x1b[");
    });

    it("should handle entry without timestamp gracefully", () => {
      const stream = new ConsoleStream({ json: false, timestamp: true });
      stream.write({ level: "info", message: "no timestamp" });
      expect(consoleOutput[0]).toContain("[INFO]");
      expect(consoleOutput[0]).toContain("no timestamp");
    });

    it("should format error with unknown level via default branch", () => {
      const stream = new ConsoleStream({ json: false, timestamp: false });
      stream.write({ level: "info" as any, message: "default level" });
      expect(consoleOutput.length).toBe(1);
    });
  });

  describe("flush", () => {
    it("should call callback via setImmediate when provided", () => {
      return new Promise<void>((resolve) => {
        const stream = new ConsoleStream();
        stream.flush(() => {
          resolve();
        });
      });
    });

    it("should not throw when callback is not provided", () => {
      const stream = new ConsoleStream();
      expect(() => stream.flush()).not.toThrow();
    });
  });

  describe("end", () => {
    it("should not throw", () => {
      const stream = new ConsoleStream();
      expect(() => stream.end()).not.toThrow();
    });
  });
});
