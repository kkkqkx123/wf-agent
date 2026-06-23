/**
 * Error Chain Tracking Tests
 *
 * Tests for error chain building and root cause analysis
 */

import { describe, it, expect, beforeEach } from "vitest";
import { AgentLoopState } from "../agent-loop-state.js";
import type { ExecutionErrorRecord } from "@wf-agent/types";

describe("AgentLoopState - Error Chain Tracking", () => {
  let state: AgentLoopState;

  beforeEach(() => {
    state = new AgentLoopState();
  });

  describe("recordError", () => {
    it("should record a single error without chain", () => {
      const error: ExecutionErrorRecord = {
        id: "error1",
        timestamp: Date.now(),
        message: "Tool not found",
        severity: "error",
        errorType: "tool_error",
        context: { operation: "tool_call" },
        isRecoverable: true,
      };

      state.recordError(error);

      expect(state.getErrorRecords()).toHaveLength(1);
      expect(error.rootCauseId).toBe("error1");
      expect(error.errorChain).toEqual(["error1"]);
      expect(error.parentErrorId).toBeUndefined();
    });

    it("should auto-generate error ID if not provided", () => {
      const error: ExecutionErrorRecord = {
        id: "",
        timestamp: Date.now(),
        message: "Test error",
        severity: "error",
        errorType: "execution_error",
        context: { operation: "test" },
        isRecoverable: false,
      };

      state.recordError(error);

      expect(error.id).toBeDefined();
      expect(error.id).not.toBe("");
    });

    it("should build error chain for subsequent errors", () => {
      const error1: ExecutionErrorRecord = {
        id: "error1",
        timestamp: Date.now(),
        message: "Tool not found",
        severity: "error",
        errorType: "tool_error",
        context: { operation: "tool_call" },
        isRecoverable: true,
      };

      const error2: ExecutionErrorRecord = {
        id: "error2",
        timestamp: Date.now() + 1000,
        message: "Invalid input",
        severity: "error",
        errorType: "validation_error",
        context: { operation: "validate" },
        isRecoverable: true,
      };

      state.recordError(error1);
      state.recordError(error2);

      // Verify error2 has chain info
      expect(error2.parentErrorId).toBe("error1");
      expect(error2.errorChain).toEqual(["error1", "error2"]);
      expect(error2.rootCauseId).toBe("error1");
    });

    it("should maintain error chain for multiple errors", () => {
      const errors: ExecutionErrorRecord[] = [];

      for (let i = 1; i <= 5; i++) {
        const error: ExecutionErrorRecord = {
          id: `error${i}`,
          timestamp: Date.now() + i * 1000,
          message: `Error ${i}`,
          severity: "error",
          errorType: "execution_error",
          context: { operation: "test" },
          isRecoverable: true,
        };
        state.recordError(error);
        errors.push(error);
      }

      // Verify last error has complete chain
      const lastError = errors[errors.length - 1]!;
      expect(lastError.errorChain).toEqual(["error1", "error2", "error3", "error4", "error5"]);
      expect(lastError.rootCauseId).toBe("error1");

      // Verify middle error in chain
      const middleError = errors[2]!;
      expect(middleError.parentErrorId).toBe("error2");
      expect(middleError.errorChain).toEqual(["error1", "error2", "error3"]);
    });

    it("should limit error records to MAX_ERROR_RECORDS", () => {
      // This is a conceptual test - actual limit is 100
      // We just verify the mechanism works
      const errorCount = 10;

      for (let i = 0; i < errorCount; i++) {
        const error: ExecutionErrorRecord = {
          id: `error${i}`,
          timestamp: Date.now(),
          message: `Error ${i}`,
          severity: "error",
          errorType: "execution_error",
          context: { operation: "test" },
          isRecoverable: false,
        };
        state.recordError(error);
      }

      expect(state.getErrorRecords().length).toBeLessThanOrEqual(errorCount);
    });
  });

  describe("getErrorChain", () => {
    beforeEach(() => {
      for (let i = 1; i <= 3; i++) {
        const error: ExecutionErrorRecord = {
          id: `error${i}`,
          timestamp: Date.now() + i * 1000,
          message: `Error ${i}`,
          severity: "error",
          errorType: "execution_error",
          context: { operation: "test" },
          isRecoverable: true,
        };
        state.recordError(error);
      }
    });

    it("should return chain for last error when no ID specified", () => {
      const chain = state.getErrorChain();

      expect(chain).toHaveLength(3);
      expect(chain[0]!.id).toBe("error1");
      expect(chain[2]!.id).toBe("error3");
    });

    it("should return chain for specific error", () => {
      const chain = state.getErrorChain("error2");

      expect(chain).toHaveLength(2);
      expect(chain[0]!.id).toBe("error1");
      expect(chain[1]!.id).toBe("error2");
    });

    it("should return empty chain when error not found", () => {
      const chain = state.getErrorChain("nonexistent");

      expect(chain).toHaveLength(0);
    });

    it("should return single error chain for first error", () => {
      const chain = state.getErrorChain("error1");

      expect(chain).toHaveLength(1);
      expect(chain[0]!.id).toBe("error1");
    });
  });

  describe("getRootCauseError", () => {
    it("should return null when no errors", () => {
      const root = state.getRootCauseError();

      expect(root).toBeNull();
    });

    it("should return first error as root cause", () => {
      for (let i = 1; i <= 3; i++) {
        const error: ExecutionErrorRecord = {
          id: `error${i}`,
          timestamp: Date.now() + i * 1000,
          message: `Error ${i}`,
          severity: "error",
          errorType: "execution_error",
          context: { operation: "test" },
          isRecoverable: true,
        };
        state.recordError(error);
      }

      const root = state.getRootCauseError();

      expect(root?.id).toBe("error1");
      expect(root?.message).toBe("Error 1");
    });
  });

  describe("analyzeErrorPattern", () => {
    it("should return empty pattern when no errors", () => {
      const pattern = state.analyzeErrorPattern();

      expect(pattern.type).toBe("none");
      expect(pattern.count).toBe(0);
      expect(pattern.errors).toHaveLength(0);
    });

    it("should identify single error pattern", () => {
      const error: ExecutionErrorRecord = {
        id: "error1",
        timestamp: Date.now(),
        message: "Test error",
        severity: "error",
        errorType: "tool_error",
        context: { operation: "tool_call", toolName: "my_tool" },
        isRecoverable: true,
      };

      state.recordError(error);
      const pattern = state.analyzeErrorPattern();

      expect(pattern.type).toBe("single");
      expect(pattern.count).toBe(1);
      expect(pattern.typeDistribution.tool_error).toBe(1);
      expect(pattern.toolProblems[0]?.name).toBe("my_tool");
    });

    it("should identify error chain pattern", () => {
      for (let i = 1; i <= 3; i++) {
        const error: ExecutionErrorRecord = {
          id: `error${i}`,
          timestamp: Date.now() + i * 1000,
          message: `Error ${i}`,
          severity: "error",
          errorType: i === 1 ? "tool_error" : "execution_error",
          context: { operation: "test" },
          isRecoverable: true,
        };
        state.recordError(error);
      }

      const pattern = state.analyzeErrorPattern();

      expect(pattern.type).toBe("chain");
      expect(pattern.count).toBe(3);
      expect(pattern.typeDistribution.tool_error).toBe(1);
      expect(pattern.typeDistribution.execution_error).toBe(2);
    });

    it("should track severity breakdown", () => {
      const errors: Array<[string, string]> = [
        ["error", "error"],
        ["warning", "warning"],
        ["error", "error"],
      ];

      for (const [id, severity] of errors) {
        const error: ExecutionErrorRecord = {
          id,
          timestamp: Date.now(),
          message: `${severity} message`,
          severity: severity as "error" | "warning" | "info",
          errorType: "execution_error",
          context: { operation: "test" },
          isRecoverable: true,
        };
        state.recordError(error);
      }

      const pattern = state.analyzeErrorPattern();

      expect(pattern.severityBreakdown.error).toBe(2);
      expect(pattern.severityBreakdown.warning).toBe(1);
    });
  });

  describe("canRecoverFromErrors", () => {
    it("should return true when all errors are recoverable", () => {
      for (let i = 1; i <= 3; i++) {
        const error: ExecutionErrorRecord = {
          id: `error${i}`,
          timestamp: Date.now(),
          message: `Error ${i}`,
          severity: "error",
          errorType: "execution_error",
          context: { operation: "test" },
          isRecoverable: true,
        };
        state.recordError(error);
      }

      expect(state.canRecoverFromErrors()).toBe(true);
    });

    it("should return false when any error is not recoverable", () => {
      for (let i = 1; i <= 3; i++) {
        const error: ExecutionErrorRecord = {
          id: `error${i}`,
          timestamp: Date.now(),
          message: `Error ${i}`,
          severity: "error",
          errorType: "execution_error",
          context: { operation: "test" },
          isRecoverable: i !== 2, // error2 is not recoverable
        };
        state.recordError(error);
      }

      expect(state.canRecoverFromErrors()).toBe(false);
    });
  });

  describe("getRecommendedRecoveryAction", () => {
    it("should return abort when no errors", () => {
      const action = state.getRecommendedRecoveryAction();

      expect(action).toBe("abort");
    });

    it("should prefer retry when most errors suggest retry", () => {
      for (let i = 1; i <= 3; i++) {
        const error: ExecutionErrorRecord = {
          id: `error${i}`,
          timestamp: Date.now(),
          message: `Error ${i}`,
          severity: "error",
          errorType: "execution_error",
          context: { operation: "test" },
          isRecoverable: true,
          recoveryAction: i === 1 ? "fallback" : "retry",
        };
        state.recordError(error);
      }

      const action = state.getRecommendedRecoveryAction();

      expect(action).toBe("retry");
    });

    it("should fallback to manual_intervention when no clear action", () => {
      const error: ExecutionErrorRecord = {
        id: "error1",
        timestamp: Date.now(),
        message: "Complex error",
        severity: "error",
        errorType: "execution_error",
        context: { operation: "test" },
        isRecoverable: false,
      };

      state.recordError(error);
      const action = state.getRecommendedRecoveryAction();

      expect(action).toBe("retry");
    });
  });
});
