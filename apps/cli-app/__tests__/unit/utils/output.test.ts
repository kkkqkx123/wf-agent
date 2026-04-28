/**
 * CLIOutput Unit Tests
 */

import { describe, it, expect, beforeEach, afterEach, vi } from "vitest";
import { CLIOutput, initializeOutput, getOutput, resetOutput } from "../../../src/utils/output.js";

describe("CLIOutput", () => {
  const originalEnv = process.env;

  beforeEach(() => {
    process.env = { ...originalEnv };
    delete process.env["CLI_MODE"];
    delete process.env["HEADLESS"];
    delete process.env["TEST_MODE"];
    delete process.env["CLI_OUTPUT_FORMAT"];
    resetOutput();
  });

  afterEach(() => {
    process.env = originalEnv;
    resetOutput();
    vi.restoreAllMocks();
  });

  describe("structuredOutput", () => {
    it("should output JSON string", () => {
      const output = initializeOutput({ color: false });
      const stdoutSpy = vi.spyOn(process.stdout, "write").mockImplementation(() => true);

      output.structuredOutput({ success: true, data: "test" });

      expect(stdoutSpy).toHaveBeenCalledWith('{"success":true,"data":"test"}\n');
      stdoutSpy.mockRestore();
    });

    it("should handle complex objects", () => {
      const output = initializeOutput({ color: false });
      const stdoutSpy = vi.spyOn(process.stdout, "write").mockImplementation(() => true);

      const complexData = {
        id: "123",
        nested: { value: 42 },
        array: [1, 2, 3],
      };
      output.structuredOutput(complexData);

      const callArg = stdoutSpy.mock.calls[0][0] as string;
      const parsed = JSON.parse(callArg.replace("\n", ""));
      expect(parsed).toEqual(complexData);
      stdoutSpy.mockRestore();
    });
  });

  describe("result", () => {
    it("should output text in interactive mode", () => {
      process.env["CLI_MODE"] = "interactive";
      const output = initializeOutput({ color: false });
      const stdoutSpy = vi.spyOn(process.stdout, "write").mockImplementation(() => true);

      output.result({ id: "123" }, { message: "Success", success: true });

      // Should contain success message
      const calls = stdoutSpy.mock.calls.map(call => call[0] as string);
      const hasSuccessMessage = calls.some(call => call.includes("Success") || call.includes("✓"));
      expect(hasSuccessMessage).toBe(true);
      stdoutSpy.mockRestore();
    });

    it("should output JSON in headless mode", () => {
      process.env["CLI_MODE"] = "headless";
      const output = initializeOutput({ color: false });
      const stdoutSpy = vi.spyOn(process.stdout, "write").mockImplementation(() => true);

      output.result({ id: "123" }, { message: "Success", success: true });

      const callArg = stdoutSpy.mock.calls[0][0] as string;
      const parsed = JSON.parse(callArg.replace("\n", ""));
      expect(parsed.success).toBe(true);
      expect(parsed.data).toEqual({ id: "123" });
      expect(parsed.message).toBe("Success");
      expect(parsed.timestamp).toBeDefined();
      stdoutSpy.mockRestore();
    });

    it("should output JSON when CLI_OUTPUT_FORMAT is json", () => {
      process.env["CLI_OUTPUT_FORMAT"] = "json";
      const output = initializeOutput({ color: false });
      const stdoutSpy = vi.spyOn(process.stdout, "write").mockImplementation(() => true);

      output.result({ test: "data" });

      const callArg = stdoutSpy.mock.calls[0][0] as string;
      const parsed = JSON.parse(callArg.replace("\n", ""));
      expect(parsed.success).toBe(true);
      expect(parsed.data).toEqual({ test: "data" });
      stdoutSpy.mockRestore();
    });

    it("should handle failure result", () => {
      process.env["CLI_MODE"] = "interactive";
      const output = initializeOutput({ color: false });
      const stderrSpy = vi.spyOn(process.stderr, "write").mockImplementation(() => true);

      output.result(null, { message: "Failed", success: false });

      const calls = stderrSpy.mock.calls.map(call => call[0] as string);
      const hasFailureMessage = calls.some(call => call.includes("Failed") || call.includes("✗"));
      expect(hasFailureMessage).toBe(true);
      stderrSpy.mockRestore();
    });
  });

  describe("errorResult", () => {
    it("should output error text in interactive mode", () => {
      process.env["CLI_MODE"] = "interactive";
      const output = initializeOutput({ color: false });
      const stderrSpy = vi.spyOn(process.stderr, "write").mockImplementation(() => true);

      output.errorResult("Something went wrong", "ERROR_CODE");

      const calls = stderrSpy.mock.calls.map(call => call[0] as string);
      const hasErrorMessage = calls.some(call => call.includes("Something went wrong"));
      expect(hasErrorMessage).toBe(true);
      stderrSpy.mockRestore();
    });

    it("should output JSON error in headless mode", () => {
      process.env["CLI_MODE"] = "headless";
      const output = initializeOutput({ color: false });
      const stdoutSpy = vi.spyOn(process.stdout, "write").mockImplementation(() => true);

      output.errorResult("Something went wrong", "ERROR_CODE");

      const callArg = stdoutSpy.mock.calls[0][0] as string;
      const parsed = JSON.parse(callArg.replace("\n", ""));
      expect(parsed.success).toBe(false);
      expect(parsed.error.message).toBe("Something went wrong");
      expect(parsed.error.code).toBe("ERROR_CODE");
      expect(parsed.error.timestamp).toBeDefined();
      stdoutSpy.mockRestore();
    });

    it("should handle Error object", () => {
      process.env["CLI_MODE"] = "headless";
      const output = initializeOutput({ color: false });
      const stdoutSpy = vi.spyOn(process.stdout, "write").mockImplementation(() => true);

      const error = new Error("Test error");
      output.errorResult(error);

      const callArg = stdoutSpy.mock.calls[0][0] as string;
      const parsed = JSON.parse(callArg.replace("\n", ""));
      expect(parsed.error.message).toBe("Test error");
      stdoutSpy.mockRestore();
    });
  });

  describe("ensureDrained", () => {
    it("should resolve when streams are already drained", async () => {
      const output = initializeOutput({ color: false });

      // Should not throw
      await expect(output.ensureDrained()).resolves.toBeUndefined();
    });

    it("should wait for stdout drain", async () => {
      // This test verifies the logic path exists
      // The actual drain behavior is tested in integration tests
      const output = initializeOutput({ color: false });

      // Mock the internal _stdout reference directly
      const onceSpy = vi.fn((event, callback) => {
        if (event === "drain") {
          setTimeout(() => callback(), 10);
        }
        return mockStdout;
      });

      const mockStdout = {
        writable: true,
        writableNeedDrain: true,
        once: onceSpy,
      } as any;

      // Replace internal _stdout reference
      (output as any)._stdout = mockStdout;

      const promise = output.ensureDrained();
      await expect(promise).resolves.toBeUndefined();
      expect(onceSpy).toHaveBeenCalledWith("drain", expect.any(Function));
    });
  });

  describe("getOutput singleton", () => {
    it("should return same instance", () => {
      const output1 = getOutput();
      const output2 = getOutput();
      expect(output1).toBe(output2);
    });

    it("should create new instance after reset", () => {
      const output1 = getOutput();
      resetOutput();
      const output2 = getOutput();
      expect(output1).not.toBe(output2);
    });
  });
});
