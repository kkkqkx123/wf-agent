/**
 * OutputRouter Unit Tests
 */
import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { Writable } from "stream";
import { OutputRouter, resetRouter, initializeRouter, getRouter } from "../output-router.js";

// --------------------------------------------------
// Mock mode-detector
// --------------------------------------------------
const mockModeDetector = vi.hoisted(() => ({
  isJsonMode: false,
  isSilentMode: false,
}));

vi.mock("../mode-detector.js", () => ({
  isJsonMode: () => mockModeDetector.isJsonMode,
  isSilentMode: () => mockModeDetector.isSilentMode,
}));

// --------------------------------------------------
// Helpers
// --------------------------------------------------
function createMockOutput() {
  const chunks: string[] = [];
  const stdout = new Writable({
    write(chunk: Buffer, _encoding: string, callback: () => void) {
      chunks.push(chunk.toString());
      callback();
    },
  });
  // Override write to also push
  const originalWrite = stdout.write.bind(stdout);
  stdout.write = ((chunk: Buffer | string, ...args: unknown[]) => {
    const str = typeof chunk === "string" ? chunk : chunk.toString();
    chunks.push(str);
    return originalWrite(chunk, args[0] as BufferEncoding, args[1] as (error: Error | null | undefined) => void);
  }) as typeof stdout.write;

  const stderr = new Writable({
    write(chunk: Buffer, _encoding: string, callback: () => void) {
      chunks.push(chunk.toString());
      callback();
    },
  });

  return {
    stdout,
    stderr,
    chunks: () => chunks.join(""),
    output: vi.fn((msg: string) => chunks.push(msg + "\n")),
    success: vi.fn((msg: string) => chunks.push(`✓ ${msg}\n`)),
    fail: vi.fn((msg: string) => chunks.push(`✗ ${msg}\n`)),
    write: vi.fn((msg: string) => chunks.push(msg)),
    structuredOutput: vi.fn((data: unknown) => chunks.push(JSON.stringify(data) + "\n")),
    close: vi.fn().mockResolvedValue(undefined),
    flush: vi.fn().mockResolvedValue(undefined),
    ensureDrained: vi.fn().mockResolvedValue(undefined),
    log: vi.fn(),
    infoLog: vi.fn(),
    warnLog: vi.fn(),
    errorLog: vi.fn(),
    debugLog: vi.fn(),
    verboseLog: vi.fn(),
    error: vi.fn(),
    errorWithLabel: vi.fn(),
    info: vi.fn(),
    warn: vi.fn(),
    newLine: vi.fn(),
    stream: vi.fn(),
    get logFile() { return "test.log"; },
    get logStream() { return null; },
    get colorEnabled() { return true; },
    get verbose() { return false; },
    get debug() { return false; },
  };
}

describe("OutputRouter", () => {
  let router: OutputRouter;
  let mockOutput: ReturnType<typeof createMockOutput>;

  beforeEach(() => {
    mockOutput = createMockOutput();
    router = new OutputRouter(mockOutput as unknown as any);
  });

  afterEach(() => {
    resetRouter();
    mockModeDetector.isJsonMode = false;
    mockModeDetector.isSilentMode = false;
  });

  describe("text mode (default)", () => {
    it("should render list with format callback", () => {
      const data = [{ id: "1" }, { id: "2" }];
      router.render(data, {
        type: "list",
        entity: "test",
        format: () => "item1\nitem2",
      });

      expect(mockOutput.output).toHaveBeenCalledWith("item1\nitem2");
    });

    it("should render detail with format callback", () => {
      const data = { id: "1", name: "test" };
      router.render(data, {
        type: "detail",
        entity: "test",
        format: () => "detail: test",
      });

      expect(mockOutput.output).toHaveBeenCalledWith("detail: test");
    });

    it("should render action with success message", () => {
      router.render(null, {
        type: "action",
        message: "Created successfully",
      });

      expect(mockOutput.success).toHaveBeenCalledWith("Created successfully");
    });

    it("should fall back to JSON.stringify for objects when no format callback", () => {
      router.render({ foo: "bar" }, { type: "detail" });

      expect(mockOutput.output).toHaveBeenCalledWith('{\n  "foo": "bar"\n}');
    });

    it("should fall back to empty string for null data", () => {
      router.render(null, { type: "detail" });

      expect(mockOutput.output).toHaveBeenCalledWith("");
    });

    it("should fall back to empty string for undefined data", () => {
      router.render(undefined, { type: "detail" });

      expect(mockOutput.output).toHaveBeenCalledWith("");
    });

    it("should wrap format callback error with render context", () => {
      const format = () => {
        throw new Error("something broke");
      };

      expect(() => {
        router.render("data", { type: "detail", entity: "test", format });
      }).toThrow('format callback failed for detail (test): something broke');
    });

    it("should wrap format callback error without entity", () => {
      const format = () => {
        throw new Error("oops");
      };

      expect(() => {
        router.render("data", { type: "list", format });
      }).toThrow('format callback failed for list: oops');
    });

    it("should throw CLIError when format callback throws non-Error", () => {
      const format = () => {
        // eslint-disable-next-line @typescript-eslint/no-throw-literal
        throw "string error";
      };

      expect(() => {
        router.render("data", { type: "detail", format });
      }).toThrow('format callback failed for detail');
    });
  });

  describe("json mode", () => {
    beforeEach(() => {
      mockModeDetector.isJsonMode = true;
    });

    it("should output structured JSON envelope", () => {
      const data = [{ id: "1" }, { id: "2" }];
      router.render(data, {
        type: "list",
        entity: "test",
        format: () => "should-not-be-called",
        metadata: { total: 2 },
      });

      const output = mockOutput.chunks();
      const parsed = JSON.parse(output.trim());
      expect(parsed).toMatchObject({
        success: true,
        type: "list",
        entity: "test",
        data: [{ id: "1" }, { id: "2" }],
        metadata: { total: 2 },
      });
      expect(parsed.timestamp).toBeDefined();
    });

    it("should not call format callback in json mode", () => {
      const format = vi.fn(() => "should-not-be-called");
      router.render("data", { type: "detail", format });

      expect(format).not.toHaveBeenCalled();
    });

    it("should output structured error", () => {
      router.error({ message: "Not found", code: "NOT_FOUND" });

      const output = mockOutput.chunks();
      const parsed = JSON.parse(output.trim());
      expect(parsed).toMatchObject({
        success: false,
        error: { message: "Not found", code: "NOT_FOUND" },
      });
    });
  });

  describe("silent mode", () => {
    beforeEach(() => {
      mockModeDetector.isSilentMode = true;
    });

    it("should not produce any output for render", () => {
      const format = vi.fn(() => "should-not-be-called");
      router.render("data", { type: "detail", format });

      expect(format).not.toHaveBeenCalled();
      expect(mockOutput.chunks()).toBe("");
    });

    it("should not produce any output for error", () => {
      router.error({ message: "Error" });

      expect(mockOutput.chunks()).toBe("");
    });
  });

  describe("global instance management", () => {
    it("should initialize and get router", () => {
      resetRouter();
      const r1 = initializeRouter();
      const r2 = getRouter();

      expect(r1).toBe(r2);
    });

    it("should reset router", () => {
      resetRouter();
      const r1 = initializeRouter();
      resetRouter();
      const r2 = getRouter();

      expect(r1).not.toBe(r2);
    });
  });
});