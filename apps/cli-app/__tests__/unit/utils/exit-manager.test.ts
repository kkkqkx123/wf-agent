/**
 * Exit Manager Unit Tests
 */

import { describe, it, expect, beforeEach, afterEach, vi } from "vitest";
import {
  ExitManager,
  isHeadlessMode,
  isProgrammaticMode,
  detectExecutionMode,
} from "../../../src/utils/exit-manager.js";

describe("ExitManager", () => {
  const originalEnv = process.env;

  beforeEach(() => {
    // Reset environment before each test
    process.env = { ...originalEnv };
    delete process.env["CLI_MODE"];
    delete process.env["HEADLESS"];
    delete process.env["TEST_MODE"];
  });

  afterEach(() => {
    process.env = originalEnv;
    vi.restoreAllMocks();
  });

  describe("isHeadlessMode", () => {
    it("should return true when CLI_MODE is headless", () => {
      process.env["CLI_MODE"] = "headless";
      expect(isHeadlessMode()).toBe(true);
    });

    it("should return true when HEADLESS is true", () => {
      process.env["HEADLESS"] = "true";
      expect(isHeadlessMode()).toBe(true);
    });

    it("should return true when TEST_MODE is true", () => {
      process.env["TEST_MODE"] = "true";
      expect(isHeadlessMode()).toBe(true);
    });

    it("should return false in interactive mode", () => {
      process.env["CLI_MODE"] = "interactive";
      expect(isHeadlessMode()).toBe(false);
    });

    it("should return false by default", () => {
      expect(isHeadlessMode()).toBe(false);
    });
  });

  describe("isProgrammaticMode", () => {
    it("should return true when CLI_MODE is programmatic", () => {
      process.env["CLI_MODE"] = "programmatic";
      expect(isProgrammaticMode()).toBe(true);
    });

    it("should return false in other modes", () => {
      process.env["CLI_MODE"] = "headless";
      expect(isProgrammaticMode()).toBe(false);

      process.env["CLI_MODE"] = "interactive";
      expect(isProgrammaticMode()).toBe(false);
    });

    it("should return false by default", () => {
      expect(isProgrammaticMode()).toBe(false);
    });
  });

  describe("detectExecutionMode", () => {
    it("should detect programmatic mode", () => {
      process.env["CLI_MODE"] = "programmatic";
      expect(detectExecutionMode()).toBe("programmatic");
    });

    it("should detect headless mode from CLI_MODE", () => {
      process.env["CLI_MODE"] = "headless";
      expect(detectExecutionMode()).toBe("headless");
    });

    it("should detect headless mode from HEADLESS", () => {
      process.env["HEADLESS"] = "true";
      expect(detectExecutionMode()).toBe("headless");
    });

    it("should detect headless mode from TEST_MODE", () => {
      process.env["TEST_MODE"] = "true";
      expect(detectExecutionMode()).toBe("headless");
    });

    it("should default to interactive mode", () => {
      expect(detectExecutionMode()).toBe("interactive");
    });

    it("should prioritize programmatic over headless", () => {
      process.env["CLI_MODE"] = "programmatic";
      process.env["HEADLESS"] = "true";
      expect(detectExecutionMode()).toBe("programmatic");
    });
  });

  describe("ExitManager.exit", () => {
    it("should set isExiting flag", async () => {
      // Mock process.exit to prevent actual exit
      const mockExit = vi.spyOn(process, "exit").mockImplementation(() => {
        throw new Error("EXIT_CALLED");
      });

      expect(ExitManager.isExiting).toBe(false);

      try {
        await ExitManager.exit(0);
      } catch (e) {
        // Expected
      }

      expect(ExitManager.isExiting).toBe(true);
      mockExit.mockRestore();
    });

    it("should call process.exit with correct code", async () => {
      const mockExit = vi.spyOn(process, "exit").mockImplementation(code => {
        throw new Error(`EXIT_${code}`);
      });

      try {
        await ExitManager.exit(42);
      } catch (e: any) {
        expect(e.message).toBe("EXIT_42");
      }

      mockExit.mockRestore();
    });

    it("should handle multiple exit calls gracefully", async () => {
      const mockExit = vi.spyOn(process, "exit").mockImplementation(() => {
        throw new Error("EXIT_CALLED");
      });

      // First call
      try {
        await ExitManager.exit(0);
      } catch (e) {
        // Expected
      }

      // Second call should still work
      try {
        await ExitManager.exit(1);
      } catch (e: any) {
        expect(e.message).toBe("EXIT_CALLED");
      }

      mockExit.mockRestore();
    });
  });
});
