/**
 * ExecutionState - Unit Tests
 * Tests for subgraph execution stack management
 */

import { describe, it, expect, beforeEach } from "vitest";
import { ExecutionState } from "../execution-state.js";
import { RuntimeValidationError } from "@wf-agent/types";

describe("ExecutionState", () => {
  let executionState: ExecutionState;

  beforeEach(() => {
    executionState = new ExecutionState();
  });

  describe("constructor", () => {
    it("should initialize with empty subgraph stack", () => {
      expect(executionState.size()).toBe(0);
      expect(executionState.isEmpty()).toBe(true);
      expect(executionState.isInSubgraph()).toBe(false);
      expect(executionState.getCurrentSubgraphContext()).toBeNull();
    });
  });

  describe("enterSubgraph", () => {
    it("should enter a subgraph successfully", () => {
      // Act
      executionState.enterSubgraph("sub-workflow-1", "parent-workflow-1", { input: "data" });

      // Assert
      expect(executionState.isInSubgraph()).toBe(true);
      expect(executionState.size()).toBe(1);
      expect(executionState.getCurrentDepth()).toBe(1);

      const context = executionState.getCurrentSubgraphContext();
      expect(context).not.toBeNull();
      expect(context?.workflowId).toBe("sub-workflow-1");
      expect(context?.parentWorkflowId).toBe("parent-workflow-1");
      expect(context?.input).toEqual({ input: "data" });
      expect(context?.depth).toBe(0);
      expect(context?.startTime).toBeGreaterThan(0);
    });

    it("should throw error when workflowId is missing", () => {
      // Act & Assert
      expect(() => {
        executionState.enterSubgraph("", "parent-workflow-1", {});
      }).toThrow(RuntimeValidationError);

      expect(() => {
        executionState.enterSubgraph("", "parent-workflow-1", {});
      }).toThrow("workflowId is required");
    });

    it("should throw error when parentWorkflowId is missing", () => {
      // Act & Assert
      expect(() => {
        executionState.enterSubgraph("sub-workflow-1", "", {});
      }).toThrow(RuntimeValidationError);

      expect(() => {
        executionState.enterSubgraph("sub-workflow-1", "", {});
      }).toThrow("parentWorkflowId is required");
    });

    it("should support nested subgraphs", () => {
      // Act
      executionState.enterSubgraph("sub-1", "parent-1", { level: 1 });
      executionState.enterSubgraph("sub-2", "sub-1", { level: 2 });
      executionState.enterSubgraph("sub-3", "sub-2", { level: 3 });

      // Assert
      expect(executionState.size()).toBe(3);
      expect(executionState.getCurrentDepth()).toBe(3);

      const context = executionState.getCurrentSubgraphContext();
      expect(context?.workflowId).toBe("sub-3");
      expect(context?.depth).toBe(2);
    });
  });

  describe("exitSubgraph", () => {
    it("should exit subgraph successfully", () => {
      // Arrange
      executionState.enterSubgraph("sub-1", "parent-1", {});
      expect(executionState.size()).toBe(1);

      // Act
      executionState.exitSubgraph();

      // Assert
      expect(executionState.size()).toBe(0);
      expect(executionState.isInSubgraph()).toBe(false);
    });

    it("should throw error when exiting with no active subgraph", () => {
      // Act & Assert
      expect(() => {
        executionState.exitSubgraph();
      }).toThrow(RuntimeValidationError);

      expect(() => {
        executionState.exitSubgraph();
      }).toThrow("Cannot exit subgraph: no active subgraph context");
    });

    it("should exit nested subgraphs in LIFO order", () => {
      // Arrange
      executionState.enterSubgraph("sub-1", "parent-1", {});
      executionState.enterSubgraph("sub-2", "sub-1", {});
      executionState.enterSubgraph("sub-3", "sub-2", {});

      // Act & Assert
      executionState.exitSubgraph();
      expect(executionState.getCurrentSubgraphContext()?.workflowId).toBe("sub-2");

      executionState.exitSubgraph();
      expect(executionState.getCurrentSubgraphContext()?.workflowId).toBe("sub-1");

      executionState.exitSubgraph();
      expect(executionState.isInSubgraph()).toBe(false);
    });
  });

  describe("getCurrentSubgraphContext", () => {
    it("should return null when not in subgraph", () => {
      expect(executionState.getCurrentSubgraphContext()).toBeNull();
    });

    it("should return current subgraph context", () => {
      // Arrange
      executionState.enterSubgraph("sub-1", "parent-1", { data: "test" });

      // Act
      const context = executionState.getCurrentSubgraphContext();

      // Assert
      expect(context).not.toBeNull();
      expect(context?.workflowId).toBe("sub-1");
      expect(context?.input).toEqual({ data: "test" });
    });
  });

  describe("getSubgraphStack", () => {
    it("should return empty array when no subgraphs", () => {
      expect(executionState.getSubgraphStack()).toEqual([]);
    });

    it("should return copy of subgraph stack", () => {
      // Arrange
      executionState.enterSubgraph("sub-1", "parent-1", {});
      executionState.enterSubgraph("sub-2", "sub-1", {});

      // Act
      const stack = executionState.getSubgraphStack();

      // Assert
      expect(stack.length).toBe(2);
      expect(stack[0]?.workflowId).toBe("sub-1");
      expect(stack[1]?.workflowId).toBe("sub-2");

      // Verify it's a copy
      stack.pop();
      expect(executionState.size()).toBe(2);
    });
  });

  describe("getCurrentWorkflowId", () => {
    it("should return base workflow ID when not in subgraph", () => {
      const workflowId = executionState.getCurrentWorkflowId("base-workflow");
      expect(workflowId).toBe("base-workflow");
    });

    it("should return current subgraph workflow ID when in subgraph", () => {
      // Arrange
      executionState.enterSubgraph("sub-1", "parent-1", {});

      // Act
      const workflowId = executionState.getCurrentWorkflowId("base-workflow");

      // Assert
      expect(workflowId).toBe("sub-1");
    });
  });

  describe("createSnapshot and restoreFromSnapshot", () => {
    it("should create snapshot of current state", () => {
      // Arrange
      executionState.enterSubgraph("sub-1", "parent-1", { data: "test" });
      executionState.enterSubgraph("sub-2", "sub-1", { data: "test2" });

      // Act
      const snapshot = executionState.createSnapshot();

      // Assert
      expect(snapshot.length).toBe(2);
      expect(snapshot[0]?.workflowId).toBe("sub-1");
      expect(snapshot[1]?.workflowId).toBe("sub-2");
    });

    it("should restore state from snapshot", () => {
      // Arrange
      executionState.enterSubgraph("sub-1", "parent-1", {});
      const snapshot = executionState.createSnapshot();

      executionState.enterSubgraph("sub-2", "sub-1", {});
      expect(executionState.size()).toBe(2);

      // Act
      executionState.restoreFromSnapshot(snapshot);

      // Assert
      expect(executionState.size()).toBe(1);
      expect(executionState.getCurrentSubgraphContext()?.workflowId).toBe("sub-1");
    });

    it("should create independent copy when snapshotting", () => {
      // Arrange
      executionState.enterSubgraph("sub-1", "parent-1", { data: "original" });
      const snapshot = executionState.createSnapshot();

      // Act - modify snapshot
      snapshot[0]!.input = { data: "modified" };

      // Assert - original state should not be affected
      expect(executionState.getCurrentSubgraphContext()?.input).toEqual({ data: "original" });
    });
  });

  describe("cleanup", () => {
    it("should clear all subgraph contexts", () => {
      // Arrange
      executionState.enterSubgraph("sub-1", "parent-1", {});
      executionState.enterSubgraph("sub-2", "sub-1", {});

      // Act
      executionState.cleanup();

      // Assert
      expect(executionState.size()).toBe(0);
      expect(executionState.isEmpty()).toBe(true);
    });
  });

  describe("reset", () => {
    it("should reset to initial state", () => {
      // Arrange
      executionState.enterSubgraph("sub-1", "parent-1", {});

      // Act
      executionState.reset();

      // Assert
      expect(executionState.size()).toBe(0);
      expect(executionState.isEmpty()).toBe(true);
    });
  });

  describe("clone", () => {
    it("should create a deep copy of execution state", () => {
      // Arrange
      executionState.enterSubgraph("sub-1", "parent-1", { data: "test" });

      // Act
      const cloned = executionState.clone();

      // Assert
      expect(cloned.size()).toBe(1);
      expect(cloned.getCurrentSubgraphContext()?.workflowId).toBe("sub-1");

      // Verify independence
      executionState.enterSubgraph("sub-2", "sub-1", {});
      expect(executionState.size()).toBe(2);
      expect(cloned.size()).toBe(1);
    });
  });
});
