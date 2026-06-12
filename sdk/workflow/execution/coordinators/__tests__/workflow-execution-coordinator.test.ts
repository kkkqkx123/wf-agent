/**
 * WorkflowExecutionCoordinator Unit Tests
 */

import { describe, it, expect, beforeEach, vi } from "vitest";
import { WorkflowExecutionCoordinator } from "../workflow-execution-coordinator.js";
import type { WorkflowExecutionEntity } from "../../entities/workflow-execution-entity.js";
import type { InterruptionState } from "../../../core/utils/interruption/interruption-state.js";
import type { NodeExecutionCoordinator } from "../node-execution-coordinator.js";
import type { WorkflowNavigator } from "../../builder/workflow-navigator.js";
import type { WorkflowNode, RuntimeNode, NodeExecutionResult, Condition, EvaluationContext } from "@wf-agent/types";

// ============================================================================
// Mock Helpers
// ============================================================================

function createMockInterruptionState(): InterruptionState {
  return {
    getAbortSignal: vi.fn().mockReturnValue(new AbortController().signal),
    requestPause: vi.fn(),
    requestStop: vi.fn(),
    resume: vi.fn(),
    isInterrupted: vi.fn().mockReturnValue(false),
  } as unknown as InterruptionState;
}

function createMockNodeExecutionCoordinator(): NodeExecutionCoordinator {
  return {
    executeNode: vi.fn(),
    handleInterruption: vi.fn().mockResolvedValue(undefined),
  } as unknown as NodeExecutionCoordinator;
}

function createMockNavigator(): WorkflowNavigator {
  return {
    getGraph: vi.fn().mockReturnValue({
      getNode: vi.fn(),
      getOutgoingEdges: vi.fn(),
    }),
    getNextNode: vi.fn(),
    routeNextNode: vi.fn(),
  } as unknown as WorkflowNavigator;
}

function createMockEntity(overrides: Partial<WorkflowExecutionEntity> = {}): WorkflowExecutionEntity {
  return {
    id: "test-exec-1",
    getWorkflowId: vi.fn().mockReturnValue("workflow-1"),
    getStatus: vi.fn().mockReturnValue("RUNNING"),
    setStatus: vi.fn(),
    getCurrentNodeId: vi.fn().mockReturnValue("node-1"),
    setCurrentNodeId: vi.fn(),
    getOutput: vi.fn().mockReturnValue({}),
    setOutput: vi.fn(),
    getInput: vi.fn().mockReturnValue({}),
    getStartTime: vi.fn().mockReturnValue(1000),
    getEndTime: vi.fn().mockReturnValue(2000),
    getErrors: vi.fn().mockReturnValue([]),
    getNodeResults: vi.fn().mockReturnValue([]),
    addNodeResult: vi.fn(),
    getAllVariables: vi.fn().mockReturnValue({}),
    getAbortSignal: vi.fn().mockReturnValue(new AbortController().signal),
    resetInterrupt: vi.fn(),
    cleanup: vi.fn(),
    getWorkflowVersion: vi.fn().mockReturnValue("1.0.0"),
    getHierarchyDepth: vi.fn().mockReturnValue(0),
    getChildExecutionIds: vi.fn().mockReturnValue([]),
  } as unknown as WorkflowExecutionEntity;
}

// ============================================================================
// Tests
// ============================================================================

describe("WorkflowExecutionCoordinator", () => {
  let coordinator: WorkflowExecutionCoordinator;
  let mockEntity: WorkflowExecutionEntity;
  let mockInterruptionState: InterruptionState;
  let mockNodeCoordinator: NodeExecutionCoordinator;
  let mockNavigator: WorkflowNavigator;

  beforeEach(() => {
    mockEntity = createMockEntity();
    mockInterruptionState = createMockInterruptionState();
    mockNodeCoordinator = createMockNodeExecutionCoordinator();
    mockNavigator = createMockNavigator();

    coordinator = new WorkflowExecutionCoordinator(
      mockEntity,
      mockInterruptionState,
      mockNodeCoordinator,
      mockNavigator,
    );
  });

  describe("constructor", () => {
    it("should create instance", () => {
      expect(coordinator).toBeInstanceOf(WorkflowExecutionCoordinator);
    });

    it("should expose the workflow execution entity", () => {
      expect(coordinator.getWorkflowExecutionEntity()).toBe(mockEntity);
    });
  });

  describe("pause / resume / stop", () => {
    it("should delegate pause to interruption manager", () => {
      coordinator.pause();
      expect(mockInterruptionState.requestPause).toHaveBeenCalled();
    });

    it("should delegate resume to interruption manager", () => {
      coordinator.resume();
      expect(mockInterruptionState.resume).toHaveBeenCalled();
    });

    it("should delegate stop to interruption manager", () => {
      coordinator.stop();
      expect(mockInterruptionState.requestStop).toHaveBeenCalled();
    });
  });

  describe("getWorkflowExecutionEntity", () => {
    it("should return the entity", () => {
      expect(coordinator.getWorkflowExecutionEntity()).toBe(mockEntity);
    });
  });
});