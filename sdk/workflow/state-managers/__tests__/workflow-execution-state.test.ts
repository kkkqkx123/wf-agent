/**
 * WorkflowExecutionState - Unit Tests
 * Tests for workflow execution state management
 */

import { describe, it, expect, beforeEach } from "vitest";
import { WorkflowExecutionState } from "../workflow-execution-state.js";
import { RuntimeValidationError } from "@wf-agent/types";
import type { OperationState } from "../workflow-execution-state.js";

describe("WorkflowExecutionState", () => {
  let state: WorkflowExecutionState;

  beforeEach(() => {
    state = new WorkflowExecutionState();
  });

  describe("constructor", () => {
    it("should initialize with CREATED status", () => {
      expect(state.status).toBe("CREATED");
      expect(state.startTime).toBeNull();
      expect(state.endTime).toBeNull();
      expect(state.error).toBeNull();
      expect(state.interrupted).toBe(false);
      expect(state.shouldPause()).toBe(false);
      expect(state.shouldStop()).toBe(false);
      expect(state.size()).toBe(1);
      expect(state.isEmpty()).toBe(false);
    });
  });

  describe("status transitions", () => {
    describe("start", () => {
      it("should transition to RUNNING", () => {
        // Act
        state.start();

        // Assert
        expect(state.status).toBe("RUNNING");
        expect(state.startTime).not.toBeNull();
        expect(state.isRunning()).toBe(true);
      });
    });

    describe("pause", () => {
      it("should transition to PAUSED from RUNNING", () => {
        // Arrange
        state.start();

        // Act
        state.pause();

        // Assert
        expect(state.status).toBe("PAUSED");
        expect(state.isPaused()).toBe(true);
        expect(state.shouldPause()).toBe(false);
      });

      it("should throw error when pausing non-RUNNING state", () => {
        // Act & Assert
        expect(() => state.pause()).toThrow(RuntimeValidationError);
        expect(() => state.pause()).toThrow("Can only pause RUNNING execution");
      });
    });

    describe("resume", () => {
      it("should transition to RUNNING from PAUSED", () => {
        // Arrange
        state.start();
        state.pause();

        // Act
        state.resume();

        // Assert
        expect(state.status).toBe("RUNNING");
        expect(state.isRunning()).toBe(true);
        expect(state.shouldPause()).toBe(false);
      });

      it("should throw error when resuming non-PAUSED state", () => {
        // Act & Assert
        expect(() => state.resume()).toThrow(RuntimeValidationError);
        expect(() => state.resume()).toThrow("Can only resume PAUSED execution");
      });
    });

    describe("complete", () => {
      it("should transition to COMPLETED", () => {
        // Act
        state.complete();

        // Assert
        expect(state.status).toBe("COMPLETED");
        expect(state.endTime).not.toBeNull();
        expect(state.isCompleted()).toBe(true);
      });
    });

    describe("fail", () => {
      it("should transition to FAILED with error", () => {
        // Arrange
        const error = new Error("Test error");

        // Act
        state.fail(error);

        // Assert
        expect(state.status).toBe("FAILED");
        expect(state.error).toBe(error);
        expect(state.endTime).not.toBeNull();
        expect(state.isFailed()).toBe(true);
      });
    });

    describe("cancel", () => {
      it("should transition to CANCELLED", () => {
        // Act
        state.cancel();

        // Assert
        expect(state.status).toBe("CANCELLED");
        expect(state.endTime).not.toBeNull();
        expect(state.isCancelled()).toBe(true);
      });
    });

    describe("timeout", () => {
      it("should transition to TIMEOUT", () => {
        // Act
        state.timeout();

        // Assert
        expect(state.status).toBe("TIMEOUT");
        expect(state.endTime).not.toBeNull();
        expect(state.isTimeout()).toBe(true);
      });
    });
  });

  describe("interrupt flags", () => {
    describe("interrupt", () => {
      it("should set pause flag", () => {
        // Act
        state.interrupt("PAUSE");

        // Assert
        expect(state.shouldPause()).toBe(true);
        expect(state.shouldStop()).toBe(false);
      });

      it("should set stop flag", () => {
        // Act
        state.interrupt("STOP");

        // Assert
        expect(state.shouldPause()).toBe(false);
        expect(state.shouldStop()).toBe(true);
      });
    });

    describe("resetInterrupt", () => {
      it("should reset both flags", () => {
        // Arrange
        state.interrupt("PAUSE");
        state.interrupt("STOP");

        // Act
        state.resetInterrupt();

        // Assert
        expect(state.shouldPause()).toBe(false);
        expect(state.shouldStop()).toBe(false);
      });
    });

    describe("setShouldPause and setShouldStop", () => {
      it("should set pause flag", () => {
        state.setShouldPause(true);
        expect(state.shouldPause()).toBe(true);

        state.setShouldPause(false);
        expect(state.shouldPause()).toBe(false);
      });

      it("should set stop flag", () => {
        state.setShouldStop(true);
        expect(state.shouldStop()).toBe(true);

        state.setShouldStop(false);
        expect(state.shouldStop()).toBe(false);
      });
    });
  });

  describe("interrupted property", () => {
    it("should set interrupted flag via interrupt()", () => {
      state.interrupt("PAUSE");
      expect(state.interrupted).toBe(true);

      state.resetInterrupt();
      expect(state.interrupted).toBe(false);
    });
  });

  describe("operation state", () => {
    it("should set current operation", () => {
      // Arrange
      const operation: OperationState = {
        type: "LLM_STREAMING",
        operationId: "req-1",
        nodeId: "node-1",
        startedAt: Date.now(),
      };

      // Act
      state.setCurrentOperation(operation);

      // Assert
      expect(state.getCurrentOperation()).toEqual(operation);
    });

    it("should clear operation", () => {
      // Arrange
      const operation: OperationState = {
        type: "LLM_STREAMING",
        operationId: "req-1",
        nodeId: "node-1",
        startedAt: Date.now(),
      };
      state.setCurrentOperation(operation);

      // Act
      state.clearOperation();

      // Assert
      expect(state.getCurrentOperation()).toBeNull();
    });

    it("should update operation progress", () => {
      // Arrange
      const operation: OperationState = {
        type: "LLM_STREAMING",
        operationId: "req-1",
        nodeId: "node-1",
        startedAt: Date.now(),
      };
      state.setCurrentOperation(operation);

      // Act
      state.updateOperationProgress({ tokensGenerated: 100, percentage: 50 });

      // Assert
      const currentOp = state.getCurrentOperation();
      expect(currentOp?.progress?.tokensGenerated).toBe(100);
      expect(currentOp?.progress?.percentage).toBe(50);
    });

    it("should not update progress when no operation", () => {
      // Act
      state.updateOperationProgress({ tokensGenerated: 100 });

      // Assert
      expect(state.getCurrentOperation()).toBeNull();
    });

    it("should update partial result", () => {
      // Arrange
      const operation: OperationState = {
        type: "LLM_STREAMING",
        operationId: "req-1",
        nodeId: "node-1",
        startedAt: Date.now(),
      };
      state.setCurrentOperation(operation);

      // Act
      state.updatePartialResult({ text: "partial content" });

      // Assert
      const currentOp = state.getCurrentOperation();
      expect(currentOp?.partialResult).toEqual({ text: "partial content" });
    });

    it("should not update partial result when no operation", () => {
      // Act
      state.updatePartialResult({ text: "test" });

      // Assert
      expect(state.getCurrentOperation()).toBeNull();
    });

    it("should get operation state snapshot", () => {
      // Arrange
      const operation: OperationState = {
        type: "TOOL_EXECUTION",
        operationId: "tool-1",
        nodeId: "node-1",
        startedAt: Date.now(),
        progress: { itemsProcessed: 5, totalItems: 10 },
      };
      state.setCurrentOperation(operation);

      // Act
      const snapshot = state.getOperationStateSnapshot();

      // Assert
      expect(snapshot).toEqual(operation);
    });

    it("should restore operation state", () => {
      // Arrange
      const operation: OperationState = {
        type: "SCRIPT_EXECUTION",
        operationId: "script-1",
        nodeId: "node-1",
        startedAt: Date.now(),
      };

      // Act
      state.restoreOperationState(operation);

      // Assert
      expect(state.getCurrentOperation()).toEqual(operation);
    });
  });

  describe("createSnapshot and restoreFromSnapshot", () => {
    it("should create snapshot", () => {
      // Arrange
      state.start();
      state.interrupt("PAUSE");
      const operation: OperationState = {
        type: "LLM_STREAMING",
        operationId: "req-1",
        nodeId: "node-1",
        startedAt: Date.now(),
      };
      state.setCurrentOperation(operation);

      // Act
      const snapshot = state.createSnapshot();

      // Assert
      expect(snapshot.status).toBe("RUNNING");
      expect(snapshot.shouldPause).toBe(true);
      expect(snapshot.shouldStop).toBe(false);
      expect(snapshot.startTime).not.toBeNull();
      expect(snapshot.endTime).toBeNull();
      expect(snapshot.interrupted).toBe(true);
      expect(snapshot.currentOperation).toEqual(operation);
    });

    it("should restore from snapshot", () => {
      // Arrange
      state.start();
      state.fail(new Error("error"));

      const snapshot = {
        status: "RUNNING" as const,
        shouldPause: false,
        shouldStop: false,
        startTime: 1000,
        endTime: null,
        error: null,
        interrupted: false,
        currentOperation: null,
      };

      // Act
      state.restoreFromSnapshot(snapshot);

      // Assert
      expect(state.status).toBe("RUNNING");
      expect(state.startTime).toBe(1000);
      expect(state.error).toBeNull();
      expect(state.isRunning()).toBe(true);
    });
  });

  describe("cleanup", () => {
    it("should cleanup error", () => {
      // Arrange
      state.fail(new Error("test error"));

      // Act
      state.cleanup();

      // Assert
      expect(state.error).toBeNull();
    });
  });

  describe("reset", () => {
    it("should reset to initial CREATED state", () => {
      // Arrange
      state.start();
      state.interrupt("STOP");
      state.fail(new Error("error"));
      const operation: OperationState = {
        type: "LLM_STREAMING",
        operationId: "req-1",
        nodeId: "node-1",
        startedAt: Date.now(),
      };
      state.setCurrentOperation(operation);

      // Act
      state.reset();

      // Assert
      expect(state.status).toBe("CREATED");
      expect(state.startTime).toBeNull();
      expect(state.endTime).toBeNull();
      expect(state.error).toBeNull();
      expect(state.interrupted).toBe(false);
      expect(state.shouldPause()).toBe(false);
      expect(state.shouldStop()).toBe(false);
      expect(state.getCurrentOperation()).toBeNull();
    });
  });

  describe("clone", () => {
    it("should clone state", () => {
      // Arrange
      state.start();
      state.interrupt("PAUSE");
      const operation: OperationState = {
        type: "LLM_STREAMING",
        operationId: "req-1",
        nodeId: "node-1",
        startedAt: Date.now(),
      };
      state.setCurrentOperation(operation);

      // Act
      const cloned = state.clone();

      // Assert
      expect(cloned.status).toBe("RUNNING");
      expect(cloned.shouldPause()).toBe(true);
      expect(cloned.interrupted).toBe(true);
      expect(cloned.getCurrentOperation()).toEqual(operation);

      // Verify independence
      cloned.interrupt("STOP");
      expect(state.shouldStop()).toBe(false);
      expect(cloned.shouldStop()).toBe(true);
    });
  });

  describe("status check methods", () => {
    it("should check running status", () => {
      expect(state.isRunning()).toBe(false);
      state.start();
      expect(state.isRunning()).toBe(true);
    });

    it("should check paused status", () => {
      expect(state.isPaused()).toBe(false);
      state.start();
      state.pause();
      expect(state.isPaused()).toBe(true);
    });

    it("should check completed status", () => {
      expect(state.isCompleted()).toBe(false);
      state.complete();
      expect(state.isCompleted()).toBe(true);
    });

    it("should check failed status", () => {
      expect(state.isFailed()).toBe(false);
      state.fail(new Error("error"));
      expect(state.isFailed()).toBe(true);
    });

    it("should check cancelled status", () => {
      expect(state.isCancelled()).toBe(false);
      state.cancel();
      expect(state.isCancelled()).toBe(true);
    });

    it("should check timeout status", () => {
      expect(state.isTimeout()).toBe(false);
      state.timeout();
      expect(state.isTimeout()).toBe(true);
    });
  });
});
