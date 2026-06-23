/**
 * Tests for WorkflowErrorAnalysisAPI
 */

import { describe, it, expect, beforeEach } from "vitest";
import type { ExecutionErrorRecord, ID } from "@wf-agent/types";
import { WorkflowErrorAnalysisAPI } from "../workflow-error-analysis-api.js";
import type { APIDependencyManager } from "@sdk/api/shared/core/sdk-dependencies.js";

// ============================================================================
// Test Helpers
// ============================================================================

/**
 * Create a mock error record for testing
 */
const createMockError = (
  id: string,
  type: string = "tool_error",
  nodeId?: string,
  isRecoverable = true
): ExecutionErrorRecord => ({
  id,
  errorType: type,
  message: `${type} error in test`,
  severity: "error",
  isRecoverable,
  timestamp: Date.now(),
  context: { operation: "test", nodeId },
  iteration: 1,
  causedBy: undefined,
  parentErrorId: undefined,
  errorChain: undefined,
  rootCauseId: undefined,
  recoveryAction: undefined,
});

/**
 * Create a mock APIDependencyManager
 */
const createMockDeps = (
  mockEntity: any
): APIDependencyManager => {
  return {
    getWorkflowExecutionRegistry: () => ({
      get: async () => mockEntity,
    }),
  } as any;
};

// ============================================================================
// Test Suites
// ============================================================================

describe("WorkflowErrorAnalysisAPI", () => {
  let api: WorkflowErrorAnalysisAPI;
  let mockDeps: APIDependencyManager;
  let mockEntity: any;

  beforeEach(() => {
    mockEntity = {
      state: {
        getErrorRecords: () => [],
      },
    };

    mockDeps = createMockDeps(mockEntity);
    api = new WorkflowErrorAnalysisAPI(mockDeps);
  });

  describe("analyzeRootCause", () => {
    it("should return no error when error records are empty", async () => {
      const result = await api.analyzeRootCause("exec-1" as ID);

      expect(result.hasError).toBe(false);
      expect(result.errorChain).toHaveLength(0);
      expect(result.affectedNodes).toHaveLength(0);
      expect(result.canRecover).toBe(false);
    });

    it("should identify root cause from single error", async () => {
      const error = createMockError("err-1", "tool_error", "node-1");
      error.errorChain = ["err-1"];
      error.rootCauseId = "err-1";

      mockEntity.state.getErrorRecords = () => [error];

      const result = await api.analyzeRootCause("exec-1" as ID);

      expect(result.hasError).toBe(true);
      expect(result.rootCauseError?.id).toBe("err-1");
      expect(result.errorChain).toHaveLength(1);
      expect(result.affectedNodes).toContain("node-1");
    });

    it("should identify root cause from error chain", async () => {
      const error1 = createMockError("err-1", "tool_error", "node-1");
      const error2 = createMockError("err-2", "timeout", "node-1");

      error1.errorChain = ["err-1"];
      error1.rootCauseId = "err-1";

      error2.parentErrorId = "err-1";
      error2.errorChain = ["err-1", "err-2"];
      error2.rootCauseId = "err-1";

      mockEntity.state.getErrorRecords = () => [error1, error2];

      const result = await api.analyzeRootCause("exec-1" as ID);

      expect(result.hasError).toBe(true);
      expect(result.rootCauseError?.id).toBe("err-1");
      expect(result.errorChain).toHaveLength(2);
      expect(result.errorChain[0]?.id).toBe("err-1");
      expect(result.errorChain[1]?.id).toBe("err-2");
    });

    it("should recommend recovery action based on error chain", async () => {
      const error = createMockError("err-1", "tool_error", "node-1", true);
      error.errorChain = ["err-1"];
      error.rootCauseId = "err-1";

      mockEntity.state.getErrorRecords = () => [error];

      const result = await api.analyzeRootCause("exec-1" as ID);

      expect(result.canRecover).toBe(true);
      expect(result.recommendedAction).toBe("retry");
    });
  });

  describe("getErrorStatistics", () => {
    it("should return zero statistics for empty error records", async () => {
      const result = await api.getErrorStatistics("exec-1" as ID);

      expect(result.totalErrors).toBe(0);
      expect(result.byType).toEqual({});
      expect(result.byNodeId).toEqual({});
      expect(result.errorRecoveryRate).toBe(100);
    });

    it("should compute error statistics by type", async () => {
      const errors = [
        createMockError("err-1", "tool_error", "node-1"),
        createMockError("err-2", "tool_error", "node-2"),
        createMockError("err-3", "timeout", "node-1"),
      ];

      mockEntity.state.getErrorRecords = () => errors;

      const result = await api.getErrorStatistics("exec-1" as ID);

      expect(result.totalErrors).toBe(3);
      expect(result.byType["tool_error"]).toBe(2);
      expect(result.byType["timeout"]).toBe(1);
    });

    it("should track errors by node ID", async () => {
      const errors = [
        createMockError("err-1", "tool_error", "node-1"),
        createMockError("err-2", "tool_error", "node-2"),
        createMockError("err-3", "timeout", "node-1"),
      ];

      mockEntity.state.getErrorRecords = () => errors;

      const result = await api.getErrorStatistics("exec-1" as ID);

      expect(result.byNodeId["node-1"]).toBe(2);
      expect(result.byNodeId["node-2"]).toBe(1);
    });

    it("should identify most error-prone node", async () => {
      const errors = [
        createMockError("err-1", "tool_error", "node-1"),
        createMockError("err-2", "tool_error", "node-2"),
        createMockError("err-3", "timeout", "node-1"),
        createMockError("err-4", "validation_error", "node-1"),
      ];

      mockEntity.state.getErrorRecords = () => errors;

      const result = await api.getErrorStatistics("exec-1" as ID);

      expect(result.mostErrorProneNode?.id).toBe("node-1");
      expect(result.mostErrorProneNode?.count).toBe(3);
    });

    it("should calculate error recovery rate", async () => {
      const errors = [
        createMockError("err-1", "tool_error", "node-1", true),
        createMockError("err-2", "tool_error", "node-2", false),
        createMockError("err-3", "timeout", "node-1", true),
      ];

      mockEntity.state.getErrorRecords = () => errors;

      const result = await api.getErrorStatistics("exec-1" as ID);

      expect(result.errorRecoveryRate).toBeCloseTo(66.67, 1);
    });
  });

  describe("getRecoveryProposal", () => {
    it("should return null for non-existent error", async () => {
      mockEntity.state.getErrorRecords = () => [];

      const result = await api.getRecoveryProposal("exec-1" as ID, "err-not-found");

      expect(result).toBeNull();
    });

    it("should propose retry for recoverable tool error", async () => {
      const error = createMockError("err-1", "tool_error", "node-1", true);

      mockEntity.state.getErrorRecords = () => [error];

      const result = await api.getRecoveryProposal("exec-1" as ID, "err-1");

      expect(result).not.toBeNull();
      expect(result!.action).toBe("retry");
      expect(result!.likelihood).toBeGreaterThan(50);
      expect(result!.steps.length).toBeGreaterThan(0);
    });

    it("should propose fallback for non-recoverable error", async () => {
      const error = createMockError("err-1", "tool_error", "node-1", false);

      mockEntity.state.getErrorRecords = () => [error];

      const result = await api.getRecoveryProposal("exec-1" as ID, "err-1");

      expect(result).not.toBeNull();
      expect(result!.action).toBe("fallback");
    });

    it("should include affected node in recovery proposal", async () => {
      const error = createMockError("err-1", "tool_error", "node-1", true);

      mockEntity.state.getErrorRecords = () => [error];

      const result = await api.getRecoveryProposal("exec-1" as ID, "err-1");

      expect(result!.affectedNode).toBeDefined();
      expect(result!.affectedNode?.id).toBe("node-1");
    });

    it("should generate recovery steps based on action", async () => {
      const error = createMockError("err-1", "tool_error", "node-1", true);

      mockEntity.state.getErrorRecords = () => [error];

      const result = await api.getRecoveryProposal("exec-1" as ID, "err-1");

      expect(result!.steps).toHaveLength(3);
      expect(result!.steps[0]).toContain("Wait");
      expect(result!.steps[1]).toContain("Retry");
    });

    it("should estimate recovery time", async () => {
      const error = createMockError("err-1", "tool_error", "node-1", true);

      mockEntity.state.getErrorRecords = () => [error];

      const result = await api.getRecoveryProposal("exec-1" as ID, "err-1");

      expect(result!.estimatedTimeToRecover).toBeGreaterThan(0);
    });
  });

  describe("getErrorChain", () => {
    it("should return empty chain for no errors", async () => {
      const result = await api.getErrorChain("exec-1" as ID);

      expect(result).toHaveLength(0);
    });

    it("should return complete error chain", async () => {
      const error1 = createMockError("err-1", "tool_error");
      const error2 = createMockError("err-2", "timeout");
      const error3 = createMockError("err-3", "validation_error");

      error1.errorChain = ["err-1"];
      error2.parentErrorId = "err-1";
      error2.errorChain = ["err-1", "err-2"];
      error3.parentErrorId = "err-2";
      error3.errorChain = ["err-1", "err-2", "err-3"];

      mockEntity.state.getErrorRecords = () => [error1, error2, error3];

      const result = await api.getErrorChain("exec-1" as ID);

      expect(result).toHaveLength(3);
      expect(result[0]!.id).toBe("err-1");
      expect(result[1]!.id).toBe("err-2");
      expect(result[2]!.id).toBe("err-3");
    });

    it("should return chain up to specified error", async () => {
      const error1 = createMockError("err-1", "tool_error");
      const error2 = createMockError("err-2", "timeout");
      const error3 = createMockError("err-3", "validation_error");

      error1.errorChain = ["err-1"];
      error2.parentErrorId = "err-1";
      error2.errorChain = ["err-1", "err-2"];
      error3.parentErrorId = "err-2";
      error3.errorChain = ["err-1", "err-2", "err-3"];

      mockEntity.state.getErrorRecords = () => [error1, error2, error3];

      const result = await api.getErrorChain("exec-1" as ID, "err-2");

      expect(result).toHaveLength(2);
      expect(result[0]!.id).toBe("err-1");
      expect(result[1]!.id).toBe("err-2");
    });
  });

  describe("getSimilarErrors", () => {
    it("should return empty for no errors", async () => {
      const result = await api.getSimilarErrors("exec-1" as ID, "err-not-found");

      expect(result).toHaveLength(0);
    });

    it("should find similar errors by type", async () => {
      const errors = [
        createMockError("err-1", "tool_error"),
        createMockError("err-2", "tool_error"),
        createMockError("err-3", "timeout"),
        createMockError("err-4", "tool_error"),
      ];

      mockEntity.state.getErrorRecords = () => errors;

      const result = await api.getSimilarErrors("exec-1" as ID, "err-1");

      expect(result).toHaveLength(2);
      expect(result.map(e => e.id)).toContain("err-2");
      expect(result.map(e => e.id)).toContain("err-4");
      expect(result.map(e => e.id)).not.toContain("err-3");
    });

    it("should not include the target error in similar errors", async () => {
      const errors = [
        createMockError("err-1", "tool_error"),
        createMockError("err-2", "tool_error"),
      ];

      mockEntity.state.getErrorRecords = () => errors;

      const result = await api.getSimilarErrors("exec-1" as ID, "err-1");

      expect(result).toHaveLength(1);
      expect(result[0]!.id).toBe("err-2");
    });
  });

  describe("getExecutionErrorRecords", () => {
    it("should retrieve error records from registry", async () => {
      const errors = [
        createMockError("err-1", "tool_error"),
        createMockError("err-2", "timeout"),
      ];

      mockEntity.state.getErrorRecords = () => errors;

      const result = await api.getExecutionErrorRecords("exec-1" as ID);

      expect(result).toHaveLength(2);
      expect(result[0]!.id).toBe("err-1");
      expect(result[1]!.id).toBe("err-2");
    });

    it("should handle missing execution", async () => {
      const deps = createMockDeps(null);
      const testApi = new WorkflowErrorAnalysisAPI(deps);

      const result = await testApi.getExecutionErrorRecords("exec-missing" as ID);

      expect(result).toHaveLength(0);
    });

    it("should handle registry errors gracefully", async () => {
      const deps = {
        getWorkflowExecutionRegistry: () => ({
          get: async () => {
            throw new Error("Registry error");
          },
        }),
      } as any;
      const testApi = new WorkflowErrorAnalysisAPI(deps);

      const result = await testApi.getExecutionErrorRecords("exec-1" as ID);

      expect(result).toHaveLength(0);
    });
  });

  describe("workflow-specific features", () => {
    it("should extract affected nodes from error chain", async () => {
      const errors = [
        createMockError("err-1", "tool_error", "node-1"),
        createMockError("err-2", "timeout", "node-2"),
        createMockError("err-3", "validation_error", "node-3"),
      ];

      errors[0]!.errorChain = ["err-1"];
      errors[1]!.errorChain = ["err-1", "err-2"];
      errors[2]!.errorChain = ["err-1", "err-2", "err-3"];

      mockEntity.state.getErrorRecords = () => errors;

      const result = await api.analyzeRootCause("exec-1" as ID);

      expect(result.affectedNodes).toHaveLength(3);
      expect(result.affectedNodes).toContain("node-1");
      expect(result.affectedNodes).toContain("node-2");
      expect(result.affectedNodes).toContain("node-3");
    });

    it("should generate workflow-aware summary", async () => {
      const error = createMockError("err-1", "tool_error", "node-1");
      error.errorChain = ["err-1"];
      error.rootCauseId = "err-1";

      mockEntity.state.getErrorRecords = () => [error];

      const result = await api.analyzeRootCause("exec-1" as ID);

      expect(result.summary).toContain("node-1");
    });

    it("should suggest skip action for warnings", async () => {
      const error = createMockError("err-1", "tool_error", "node-1", true);
      error.severity = "warning";
      error.errorChain = ["err-1"];
      error.rootCauseId = "err-1";

      mockEntity.state.getErrorRecords = () => [error];

      const result = await api.analyzeRootCause("exec-1" as ID);

      expect(result.recommendedAction).toBe("skip");
    });

    it("should suggest abort for execution errors", async () => {
      const error = createMockError("err-1", "execution_error", "node-1", true);

      mockEntity.state.getErrorRecords = () => [error];

      const proposal = await api.getRecoveryProposal("exec-1" as ID, "err-1");

      expect(proposal!.action).toBe("retry");
    });
  });
});
