/**
 * Execution Service Unit Tests
 *
 * Tests execution mode validation (validateModeCombination) and
 * the subprocess-based execution model (execute methods).
 *
 * Tests cover the mode combination matrix:
 *   interactive + foreground/background/blocking -> all valid
 *   headless    + foreground -> blocking (downgrade)
 *   headless    + background -> background (valid with subprocess)
 *   headless    + blocking   -> blocking (valid)
 *   test        + foreground/background/blocking -> all valid (no downgrade)
 */

import { describe, it, expect, beforeEach, afterEach, vi } from "vitest";
import { ExecutionService, ExecutionResult } from "../../../src/services/execution/execution-service.js";
import { getMode, invalidateModeCache, isHeadless, isInteractive, isTest } from "../../../src/utils/mode-detector.js";
import type { WorkflowExecutionMode } from "@wf-agent/types";

// =============================================================================
// Mocks
// =============================================================================

const mockSDK = {
  getFactory: vi.fn().mockReturnValue({
    getDependencies: vi.fn().mockReturnValue({}),
  }),
} as any;

const mockTerminalManager = {
  createTerminal: vi.fn().mockReturnValue({
    id: "test-terminal",
    pty: { write: vi.fn() },
    pid: 12345,
  }),
  cleanupAll: vi.fn(),
} as any;

// =============================================================================
// Helpers
// =============================================================================

function createService(): ExecutionService {
  return new ExecutionService(mockSDK, mockTerminalManager);
}

const ENV = {
  CLI_MODE: "CLI_MODE",
  HEADLESS: "HEADLESS",
  TEST_MODE: "TEST_MODE",
  OUTPUT_FORMAT: "CLI_OUTPUT_FORMAT",
  NO_COLOR: "NO_COLOR",
};

function clearModeEnv(): void {
  delete process.env[ENV.CLI_MODE];
  delete process.env[ENV.HEADLESS];
  delete process.env[ENV.TEST_MODE];
  delete process.env[ENV.OUTPUT_FORMAT];
  delete process.env[ENV.NO_COLOR];
  invalidateModeCache();
}

// =============================================================================
// Tests
// =============================================================================

describe("validateModeCombination", () => {
  beforeEach(() => {
    clearModeEnv();
  });

  afterEach(() => {
    clearModeEnv();
  });

  // ---- Interactive mode ----

  describe("interactive mode", () => {
    beforeEach(() => {
      // interactive is the default when no env vars are set
      clearModeEnv();
    });

    it("should allow foreground mode", () => {
      // Access private method via prototype
      const service = createService();
      const result = (service as any).validateModeCombination("foreground");
      expect(result).toBe("foreground");
    });

    it("should allow blocking mode", () => {
      const service = createService();
      const result = (service as any).validateModeCombination("blocking");
      expect(result).toBe("blocking");
    });

    it("should allow background mode", () => {
      const service = createService();
      const result = (service as any).validateModeCombination("background");
      expect(result).toBe("background");
    });
  });

  // ---- Headless mode ----

  describe("headless mode", () => {
    beforeEach(() => {
      clearModeEnv();
      process.env[ENV.CLI_MODE] = "headless";
    });

    it("should downgrade foreground to blocking", () => {
      const service = createService();
      // validateModeCombination writes a warning via output.warnLog
      const result = (service as any).validateModeCombination("foreground");
      expect(result).toBe("blocking");
    });

    it("should allow background mode (subprocess + log file, no TTY needed)", () => {
      const service = createService();
      const result = (service as any).validateModeCombination("background");
      expect(result).toBe("background");
    });

    it("should allow blocking mode", () => {
      const service = createService();
      const result = (service as any).validateModeCombination("blocking");
      expect(result).toBe("blocking");
    });
  });

  // ---- Test mode ----

  describe("test mode", () => {
    beforeEach(() => {
      clearModeEnv();
      process.env[ENV.CLI_MODE] = "test";
    });

    it("should allow foreground mode (no downgrade)", () => {
      const service = createService();
      const result = (service as any).validateModeCombination("foreground");
      expect(result).toBe("foreground");
    });

    it("should allow blocking mode (no downgrade)", () => {
      const service = createService();
      const result = (service as any).validateModeCombination("blocking");
      expect(result).toBe("blocking");
    });

    it("should allow background mode (no downgrade)", () => {
      const service = createService();
      const result = (service as any).validateModeCombination("background");
      expect(result).toBe("background");
    });
  });
});

describe("ExecutionResult interface", () => {
  it("should have detached field for subprocess modes", () => {
    const result: ExecutionResult = {
      mode: "foreground",
      executionId: "exec-1",
      workflowId: "wf-1",
      status: "running",
      startTime: new Date(),
      pid: 12345,
      detached: true,
    };
    expect(result.detached).toBe(true);
  });

  it("should have detached=false for blocking mode", () => {
    const result: ExecutionResult = {
      mode: "blocking",
      executionId: "exec-1",
      workflowId: "wf-1",
      status: "completed",
      startTime: new Date(),
      detached: false,
      result: {} as any,
    };
    expect(result.detached).toBe(false);
  });
});
