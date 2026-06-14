/**
 * VariableCoordinator Unit Tests
 */

import { describe, it, expect, beforeEach, vi } from "vitest";
import { VariableCoordinator } from "../variable-coordinator.js";
import { VariableManager } from "../../../state-managers/variable-manager.js";
import type { WorkflowExecutionEntity } from "../../../entities/workflow-execution-entity.js";
import type { EventRegistry } from "../../../../core/registry/event-registry.js";
import type { ExecutionEventEmitter } from "../../../../core/registry/event-emitter.js";
import type { VariableDefinition } from "@wf-agent/types";
import { RuntimeValidationError } from "@wf-agent/types";

// ============================================================================
// Mock Helpers
// ============================================================================

function createMockEventRegistry(): EventRegistry {
  return {
    on: vi.fn(),
    off: vi.fn(),
    emit: vi.fn(),
    once: vi.fn(),
    getEmitter: vi.fn().mockReturnValue({ on: vi.fn(), emit: vi.fn() }),
    removeAllListeners: vi.fn(),
    listenerCount: vi.fn().mockReturnValue(0),
    listeners: vi.fn().mockReturnValue([]),
    rawListeners: vi.fn().mockReturnValue([]),
    waitFor: vi.fn().mockResolvedValue(undefined),
    cleanupExecutionListeners: vi.fn(),
    getExecutionListenerStats: vi.fn().mockReturnValue({}),
    getMetricsCollector: vi.fn(),
    registerWorkflowListeners: vi.fn(),
  } as unknown as EventRegistry;
}

function createMockExecutionEntity(
  overrides: Partial<WorkflowExecutionEntity> = {},
): WorkflowExecutionEntity {
  return {
    id: "test-exec-1",
    getWorkflowId: vi.fn().mockReturnValue("workflow-1"),
    getStatus: vi.fn().mockReturnValue("RUNNING"),
    getCurrentNodeId: vi.fn().mockReturnValue("node-1"),
    setCurrentNodeId: vi.fn(),
    getOutput: vi.fn().mockReturnValue({}),
    setOutput: vi.fn(),
    getInput: vi.fn().mockReturnValue({}),
    getStartTime: vi.fn().mockReturnValue(Date.now()),
    getEndTime: vi.fn().mockReturnValue(undefined),
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
    ...overrides,
  } as unknown as WorkflowExecutionEntity;
}

function createVariableManager(variableDefinitions: VariableDefinition[] = []): VariableManager {
  const manager = new VariableManager();
  if (variableDefinitions.length > 0) {
    manager.initializeFromDefinitions(variableDefinitions);
  }
  return manager;
}

// ============================================================================
// Tests
// ============================================================================

describe("VariableCoordinator", () => {
  let coordinator: VariableCoordinator;
  let mockEventRegistry: EventRegistry;
  let mockExecutionEntity: WorkflowExecutionEntity;

  beforeEach(() => {
    mockEventRegistry = createMockEventRegistry();
    mockExecutionEntity = createMockExecutionEntity();
    coordinator = new VariableCoordinator(mockEventRegistry);
  });

  describe("constructor", () => {
    it("should create without event manager", () => {
      const c = new VariableCoordinator();
      expect(c).toBeInstanceOf(VariableCoordinator);
    });

    it("should create with event manager", () => {
      const c = new VariableCoordinator(mockEventRegistry);
      expect(c).toBeInstanceOf(VariableCoordinator);
    });
  });

  describe("initializeFromDefinitions", () => {
    it("should initialize variables from definitions", () => {
      const manager = createVariableManager();
      const definitions: VariableDefinition[] = [
        { name: "var1", type: "string", value: "hello", readonly: false },
        { name: "var2", type: "number", value: 42, readonly: false },
      ];

      coordinator.initializeFromDefinitions(manager, definitions);

      expect(manager.getVariable("var1")).toBe("hello");
      expect(manager.getVariable("var2")).toBe(42);
    });

    it("should handle empty definitions", () => {
      const manager = createVariableManager();
      coordinator.initializeFromDefinitions(manager, []);
      expect(manager.getAllVariables()).toEqual({});
    });
  });

  describe("getVariable", () => {
    it("should return variable value when it exists", () => {
      const manager = createVariableManager([
        { name: "myVar", type: "string", value: "test-value", readonly: false },
      ]);

      const result = coordinator.getVariable(manager, mockExecutionEntity, "myVar");
      expect(result).toBe("test-value");
    });

    it("should return undefined when variable does not exist", () => {
      const manager = createVariableManager();
      const result = coordinator.getVariable(manager, mockExecutionEntity, "nonExistent");
      expect(result).toBeUndefined();
    });
  });

  describe("updateVariable", () => {
    it("should update an existing variable", async () => {
      const manager = createVariableManager([
        { name: "myVar", type: "string", value: "old", readonly: false },
      ]);

      await coordinator.updateVariable(manager, mockExecutionEntity, "myVar", "new-value");

      expect(manager.getVariable("myVar")).toBe("new-value");
    });

    it("should throw RuntimeValidationError for undefined variable", async () => {
      const manager = createVariableManager();

      await expect(
        coordinator.updateVariable(manager, mockExecutionEntity, "undefinedVar", "value"),
      ).rejects.toThrow(RuntimeValidationError);
    });

    it("should throw RuntimeValidationError for readonly variable", async () => {
      const manager = createVariableManager([
        { name: "readonlyVar", type: "string", value: "fixed", readonly: true },
      ]);

      await expect(
        coordinator.updateVariable(manager, mockExecutionEntity, "readonlyVar", "new-value"),
      ).rejects.toThrow(RuntimeValidationError);
    });

    it("should throw RuntimeValidationError on type mismatch", async () => {
      const manager = createVariableManager([
        { name: "numVar", type: "number", value: 42, readonly: false },
      ]);

      await expect(
        coordinator.updateVariable(manager, mockExecutionEntity, "numVar", "not-a-number"),
      ).rejects.toThrow(RuntimeValidationError);
    });

    it("should emit VARIABLE_CHANGED event on successful update", async () => {
      const emitter = { on: vi.fn(), emit: vi.fn() };
      const eventRegistry = createMockEventRegistry();
      vi.mocked(eventRegistry.getEmitter).mockReturnValue(
        emitter as unknown as ExecutionEventEmitter,
      );

      const c = new VariableCoordinator(eventRegistry);
      const manager = createVariableManager([
        { name: "myVar", type: "string", value: "old", readonly: false },
      ]);

      await c.updateVariable(manager, mockExecutionEntity, "myVar", "new-value");

      expect(eventRegistry.emit).toHaveBeenCalled();
    });

    it("should handle event emission failure gracefully", async () => {
      const eventRegistry = createMockEventRegistry();
      vi.mocked(eventRegistry.emit).mockRejectedValue(new Error("Event error"));

      const c = new VariableCoordinator(eventRegistry);
      const manager = createVariableManager([
        { name: "myVar", type: "string", value: "old", readonly: false },
      ]);

      // Should not throw despite event emission failure
      await expect(
        c.updateVariable(manager, mockExecutionEntity, "myVar", "new-value"),
      ).resolves.not.toThrow();
    });

    it("should not emit event when no event manager is configured", async () => {
      const c = new VariableCoordinator(); // No event manager
      const manager = createVariableManager([
        { name: "myVar", type: "string", value: "old", readonly: false },
      ]);

      await c.updateVariable(manager, mockExecutionEntity, "myVar", "new-value");

      expect(manager.getVariable("myVar")).toBe("new-value");
    });
  });

  describe("validateType", () => {
    it("should validate number type", async () => {
      const manager = createVariableManager([
        { name: "n", type: "number", value: 0, readonly: false },
      ]);

      await expect(
        coordinator.updateVariable(manager, mockExecutionEntity, "n", 123),
      ).resolves.not.toThrow();
    });

    it("should reject NaN for number type", async () => {
      const manager = createVariableManager([
        { name: "n", type: "number", value: 0, readonly: false },
      ]);

      await expect(
        coordinator.updateVariable(manager, mockExecutionEntity, "n", NaN),
      ).rejects.toThrow(RuntimeValidationError);
    });

    it("should validate boolean type", async () => {
      const manager = createVariableManager([
        { name: "b", type: "boolean", value: false, readonly: false },
      ]);

      await coordinator.updateVariable(manager, mockExecutionEntity, "b", true);
      expect(manager.getVariable("b")).toBe(true);
    });

    it("should validate array type", async () => {
      const manager = createVariableManager([
        { name: "arr", type: "array", value: [], readonly: false },
      ]);

      await coordinator.updateVariable(manager, mockExecutionEntity, "arr", [1, 2, 3]);
      expect(manager.getVariable("arr")).toEqual([1, 2, 3]);
    });

    it("should reject non-array for array type", async () => {
      const manager = createVariableManager([
        { name: "arr", type: "array", value: [], readonly: false },
      ]);

      await expect(
        coordinator.updateVariable(manager, mockExecutionEntity, "arr", "not-array"),
      ).rejects.toThrow(RuntimeValidationError);
    });

    it("should validate object type", async () => {
      const manager = createVariableManager([
        { name: "obj", type: "object", value: {}, readonly: false },
      ]);

      await coordinator.updateVariable(manager, mockExecutionEntity, "obj", { key: "val" });
      expect(manager.getVariable("obj")).toEqual({ key: "val" });
    });

    it("should reject array for object type", async () => {
      const manager = createVariableManager([
        { name: "obj", type: "object", value: {}, readonly: false },
      ]);

      await expect(
        coordinator.updateVariable(manager, mockExecutionEntity, "obj", [1, 2]),
      ).rejects.toThrow(RuntimeValidationError);
    });

    it("should reject null for object type", async () => {
      const manager = createVariableManager([
        { name: "obj", type: "object", value: {}, readonly: false },
      ]);

      await expect(
        coordinator.updateVariable(manager, mockExecutionEntity, "obj", null),
      ).rejects.toThrow(RuntimeValidationError);
    });

    it("should reject unknown type", async () => {
      const manager = createVariableManager([
        { name: "unk", type: "number", value: 0, readonly: false },
      ]);

      await expect(
        coordinator.updateVariable(manager, mockExecutionEntity, "unk", "anything"),
      ).rejects.toThrow(RuntimeValidationError);
    });
  });

  describe("hasVariable", () => {
    it("should return true when variable exists with a value", () => {
      const manager = createVariableManager([
        { name: "exists", type: "string", value: "yes", readonly: false },
      ]);

      expect(coordinator.hasVariable(manager, mockExecutionEntity, "exists")).toBe(true);
    });

    it("should return false when variable does not exist", () => {
      const manager = createVariableManager();
      expect(coordinator.hasVariable(manager, mockExecutionEntity, "nonExistent")).toBe(false);
    });
  });

  describe("getAllVariables", () => {
    it("should return all variables", () => {
      const manager = createVariableManager([
        { name: "a", type: "string", value: "1", readonly: false },
      ]);

      const all = coordinator.getAllVariables(manager);
      expect(all).toEqual({ a: "1" });
    });

    it("should return empty object when no variables", () => {
      const manager = createVariableManager();
      expect(coordinator.getAllVariables(manager)).toEqual({});
    });
  });

  describe("copyVariables", () => {
    it("should copy variables from source to target", () => {
      const source = createVariableManager([
        { name: "srcVar", type: "string", value: "source-value", readonly: false },
      ]);
      const target = createVariableManager();

      coordinator.copyVariables(source, target);

      expect(target.getVariable("srcVar")).toBe("source-value");
    });
  });

  describe("clearVariables", () => {
    it("should clear all variables", () => {
      const manager = createVariableManager([
        { name: "a", type: "string", value: "test", readonly: false },
      ]);

      coordinator.clearVariables(manager);

      expect(manager.getAllVariables()).toEqual({});
    });
  });
});
