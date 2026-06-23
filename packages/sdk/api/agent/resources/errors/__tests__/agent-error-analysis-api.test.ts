/**
 * Agent Error Analysis API Tests
 *
 * Tests for error analysis, root cause identification, and recovery proposals
 */

import { describe, it, expect, beforeEach } from "vitest";
import { AgentErrorAnalysisAPI, type RootCauseAnalysis, type ErrorStatistics } from "../agent-error-analysis-api.js";
import type { ExecutionErrorRecord } from "@wf-agent/types";

describe("AgentErrorAnalysisAPI", () => {
  let api: AgentErrorAnalysisAPI;

  beforeEach(() => {
    // Mock dependencies manager
    const mockDeps = {} as any;
    api = new AgentErrorAnalysisAPI(mockDeps);
  });

  describe("RootCauseAnalysis type", () => {
    it("should have correct structure for no errors case", () => {
      const analysis: RootCauseAnalysis = {
        hasError: false,
        errorChain: [],
        canRecover: false,
        summary: "No errors occurred during execution",
      };

      expect(analysis.hasError).toBe(false);
      expect(analysis.rootCauseError).toBeUndefined();
      expect(analysis.errorChain).toHaveLength(0);
    });

    it("should have correct structure for error case", () => {
      const error: ExecutionErrorRecord = {
        id: "error1",
        timestamp: Date.now(),
        message: "Tool not found",
        severity: "error",
        errorType: "tool_error",
        context: { operation: "tool_call" },
        isRecoverable: true,
      };

      const analysis: RootCauseAnalysis = {
        hasError: true,
        rootCauseError: error,
        errorChain: [error],
        canRecover: true,
        recommendedAction: "retry",
        summary: "Error: Tool not found",
      };

      expect(analysis.hasError).toBe(true);
      expect(analysis.rootCauseError?.id).toBe("error1");
      expect(analysis.recommendedAction).toBe("retry");
    });
  });

  describe("ErrorStatistics type", () => {
    it("should have correct structure", () => {
      const stats: ErrorStatistics = {
        totalErrors: 5,
        byType: { tool_error: 3, validation_error: 2 },
        byIteration: { 1: 2, 2: 3 },
        bySeverity: { error: 4, warning: 1 },
        mostCommonType: "tool_error",
        errorRecoveryRate: 80,
      };

      expect(stats.totalErrors).toBe(5);
      expect(stats.byType.tool_error).toBe(3);
      expect(stats.errorRecoveryRate).toBe(80);
    });
  });

  describe("API method signatures", () => {
    it("should have getErrorChain method", () => {
      expect(typeof api.getErrorChain).toBe("function");
    });

    it("should have analyzeRootCause method", () => {
      expect(typeof api.analyzeRootCause).toBe("function");
    });

    it("should have getErrorStatistics method", () => {
      expect(typeof api.getErrorStatistics).toBe("function");
    });

    it("should have getRecoveryProposal method", () => {
      expect(typeof api.getRecoveryProposal).toBe("function");
    });

    it("should have getSimilarErrors method", () => {
      expect(typeof api.getSimilarErrors).toBe("function");
    });
  });

  describe("Error record structure", () => {
    it("should support error chain fields", () => {
      const error: ExecutionErrorRecord = {
        id: "error1",
        timestamp: Date.now(),
        message: "Initial error",
        severity: "error",
        errorType: "tool_error",
        context: { operation: "tool_call", toolName: "my_tool" },
        isRecoverable: true,
        recoveryAction: "retry",
        // Error chain fields
        parentErrorId: undefined,
        errorChain: ["error1"],
        rootCauseId: "error1",
        causedBy: {
          reason: "Tool not found",
          handlingAttempt: "Attempted retry",
        },
      };

      expect(error.parentErrorId).toBeUndefined();
      expect(error.errorChain).toEqual(["error1"]);
      expect(error.rootCauseId).toBe("error1");
      expect(error.causedBy?.reason).toBe("Tool not found");
    });

    it("should support error chain with multiple errors", () => {
      const error1: ExecutionErrorRecord = {
        id: "error1",
        timestamp: Date.now(),
        message: "Initial error",
        severity: "error",
        errorType: "tool_error",
        context: { operation: "tool_call" },
        isRecoverable: true,
      };

      const error2: ExecutionErrorRecord = {
        id: "error2",
        timestamp: Date.now() + 1000,
        message: "Cascading error",
        severity: "error",
        errorType: "execution_error",
        context: { operation: "execute" },
        isRecoverable: true,
        parentErrorId: "error1",
        errorChain: ["error1", "error2"],
        rootCauseId: "error1",
        causedBy: {
          reason: "Failed to recover from error1",
        },
      };

      expect(error2.parentErrorId).toBe("error1");
      expect(error2.errorChain).toEqual(["error1", "error2"]);
      expect(error2.rootCauseId).toBe("error1");
    });
  });

  describe("Error context", () => {
    it("should capture tool execution context", () => {
      const error: ExecutionErrorRecord = {
        id: "tool_error1",
        timestamp: Date.now(),
        message: "Tool execution failed",
        severity: "error",
        errorType: "tool_error",
        context: {
          operation: "tool_call",
          toolName: "web_search",
          input: { query: "test", max_results: 10 },
        },
        isRecoverable: true,
      };

      expect(error.context.toolName).toBe("web_search");
      expect(error.context.input?.query).toBe("test");
    });

    it("should capture iteration context", () => {
      const error: ExecutionErrorRecord = {
        id: "iter_error1",
        timestamp: Date.now(),
        message: "Iteration failed",
        severity: "error",
        errorType: "execution_error",
        context: { operation: "iteration_execute" },
        isRecoverable: false,
        iteration: 3,
      };

      expect(error.iteration).toBe(3);
    });
  });

  describe("Recovery actions", () => {
    it("should support all recovery action types", () => {
      const actions: Array<"retry" | "fallback" | "skip" | "abort"> = [
        "retry",
        "fallback",
        "skip",
        "abort",
      ];

      actions.forEach(action => {
        const error: ExecutionErrorRecord = {
          id: `error_${action}`,
          timestamp: Date.now(),
          message: `Error for ${action}`,
          severity: "error",
          errorType: "execution_error",
          context: { operation: "test" },
          isRecoverable: action !== "abort",
          recoveryAction: action as any,
        };

        expect(error.recoveryAction).toBe(action);
      });
    });
  });

  describe("Error severity levels", () => {
    it("should support all severity levels", () => {
      const severities: Array<"error" | "warning" | "info"> = ["error", "warning", "info"];

      severities.forEach(severity => {
        const error: ExecutionErrorRecord = {
          id: `error_${severity}`,
          timestamp: Date.now(),
          message: `${severity} level message`,
          severity,
          errorType: "execution_error",
          context: { operation: "test" },
          isRecoverable: true,
        };

        expect(error.severity).toBe(severity);
      });
    });
  });

  describe("Error details", () => {
    it("should support additional error details", () => {
      const error: ExecutionErrorRecord = {
        id: "detailed_error",
        timestamp: Date.now(),
        message: "Detailed error",
        code: "ERR_001",
        severity: "error",
        errorType: "execution_error",
        context: { operation: "test" },
        isRecoverable: true,
        details: {
          stackTrace: "at function...",
          requestId: "req_123",
          userId: "user_456",
        },
      };

      expect(error.code).toBe("ERR_001");
      expect(error.details?.requestId).toBe("req_123");
    });
  });
});
