/**
 * TriggerCoordinator Unit Tests
 */

import { describe, it, expect, beforeEach, vi } from "vitest";
import { TriggerCoordinator } from "../trigger-coordinator.js";
import { TriggerHandlerContextFactory } from "../../factories/trigger-handler-context-factory.js";
import type { TriggerHandlerContextFactoryConfig } from "../../factories/trigger-handler-context-factory.js";
import type { TriggerStatus, WorkflowTrigger, TriggerRuntimeState } from "@wf-agent/types";
import type { BaseEvent } from "@wf-agent/types";
import { ExecutionError, RuntimeValidationError } from "@wf-agent/types";
import type { WorkflowExecutionRegistry } from "../../../registry/workflow-execution-registry.js";
import type { WorkflowRegistry } from "../../../registry/workflow-registry.js";

// ============================================================================
// Mock Helpers
// ============================================================================

function createMockStateManager() {
  const states = new Map<string, TriggerRuntimeState>();

  return {
    hasState: vi.fn((id: string) => states.has(id)),
    register: vi.fn((state: TriggerRuntimeState) => {
      states.set(state.triggerId, state);
    }),
    getState: vi.fn((id: string) => states.get(id)),
    getAllStates: vi.fn(() => new Map(states)),
    updateStatus: vi.fn((id: string, status: TriggerStatus) => {
      const state = states.get(id);
      if (state) {
        state.status = status;
      }
    }),
    deleteState: vi.fn((id: string) => states.delete(id)),
    incrementTriggerCount: vi.fn((id: string) => {
      const state = states.get(id);
      if (state) {
        state.triggerCount++;
      }
    }),
    getExecutionId: vi.fn(() => "test-exec-1"),
    getWorkflowId: vi.fn(() => "workflow-1"),
    cleanup: vi.fn(),
  };
}

function createMockGraphRegistry() {
  return {
    get: vi.fn(),
    register: vi.fn(),
    has: vi.fn(),
    unregister: vi.fn(),
    list: vi.fn(),
    clear: vi.fn(),
    size: vi.fn(),
  };
}

function createMockCheckpointState() {
  return {
    create: vi.fn().mockResolvedValue("checkpoint-1"),
    get: vi.fn(),
    list: vi.fn().mockResolvedValue([]),
    delete: vi.fn().mockResolvedValue(true),
    exists: vi.fn().mockResolvedValue(true),
    update: vi.fn(),
    clear: vi.fn(),
    count: vi.fn().mockResolvedValue(0),
    getLatest: vi.fn(),
    getLatestByNode: vi.fn(),
    restore: vi.fn(),
    extractStorageMetadata: vi.fn(),
    buildCreatedEvent: vi.fn(),
    buildDeletedEvent: vi.fn(),
    buildFailedEvent: vi.fn(),
    cleanupWorkflowExecutionCheckpoints: vi.fn().mockResolvedValue(undefined),
  };
}

function createMockEventRegistry() {
  return {
    on: vi.fn(),
    off: vi.fn(),
    emit: vi.fn().mockResolvedValue(undefined),
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
  };
}

function createMockWorkflowRegistry() {
  return {
    register: vi.fn(),
    get: vi.fn(),
    has: vi.fn(),
    unregister: vi.fn(),
    list: vi.fn(),
    clear: vi.fn(),
    size: vi.fn(),
    getAllWorkflowIds: vi.fn(),
  };
}

function createMockGlobalContext() {
  return {
    container: {
      get: vi.fn(),
    },
    eventRegistry: createMockEventRegistry(),
    llmExecutor: {},
    metricsRegistry: {
      getWorkflowCollector: vi
        .fn()
        .mockReturnValue({ recordExecutionStart: vi.fn(), recordExecutionEnd: vi.fn() }),
      getNodeCollector: vi
        .fn()
        .mockReturnValue({ recordNodeExecutionStart: vi.fn(), recordNodeExecution: vi.fn() }),
    },
  };
}

function createMockWorkflowExecutionRegistry() {
  return {
    register: vi.fn(),
    get: vi.fn(),
    delete: vi.fn(),
    getAll: vi.fn().mockReturnValue([]),
    getAllIds: vi.fn().mockReturnValue([]),
    size: vi.fn().mockReturnValue(0),
    clear: vi.fn(),
    has: vi.fn(),
    isWorkflowActive: vi.fn(),
    getByStatus: vi.fn(),
    getActive: vi.fn(),
    cleanupTerminated: vi.fn(),
  };
}

function buildTriggerDefinition(overrides: Partial<WorkflowTrigger> = {}): WorkflowTrigger {
  return {
    id: "trigger-1",
    name: "Test Trigger",
    type: "workflow_trigger",
    condition: {
      eventType: "NODE_COMPLETED",
    },
    action: {
      type: "set_variable",
      parameters: {
        executionId: "current",
        variables: { status: "completed" },
      },
    },
    enabled: true,
    ...overrides,
  } as WorkflowTrigger;
}

function buildMockEvent(overrides: Partial<BaseEvent> = {}): BaseEvent {
  return {
    type: "NODE_COMPLETED",
    id: "event-1",
    executionId: "test-exec-1",
    workflowId: "workflow-1",
    timestamp: Date.now(),
    nodeId: "node-1",
    ...overrides,
  } as unknown as BaseEvent;
}

// ============================================================================
// Tests
// ============================================================================

describe("TriggerCoordinator", () => {
  let coordinator: TriggerCoordinator;
  let mockStateManager: ReturnType<typeof createMockStateManager>;
  let mockGraphRegistry: ReturnType<typeof createMockGraphRegistry>;
  let contextFactoryConfig: TriggerHandlerContextFactoryConfig;

  beforeEach(() => {
    mockStateManager = createMockStateManager();
    mockGraphRegistry = createMockGraphRegistry();

    contextFactoryConfig = {
      workflowExecutionRegistry:
        createMockWorkflowExecutionRegistry() as unknown as WorkflowExecutionRegistry,
      workflowRegistry: createMockWorkflowRegistry() as unknown as WorkflowRegistry,
      stateManager: mockStateManager as any,
      globalContext: createMockGlobalContext() as any,
      graphRegistry: mockGraphRegistry as any,
      eventManager: createMockEventRegistry() as any,
      checkpointStateManager: createMockCheckpointState() as any,
    };

    coordinator = new TriggerCoordinator(contextFactoryConfig);
  });

  describe("constructor", () => {
    it("should create with default error handling strategy", () => {
      expect(coordinator).toBeInstanceOf(TriggerCoordinator);
    });

    it("should create with custom error handling strategy", () => {
      const c = new TriggerCoordinator(contextFactoryConfig, { errorHandlingStrategy: "throw" });
      expect(c).toBeInstanceOf(TriggerCoordinator);
    });

    it("should create with silent error handling strategy", () => {
      const c = new TriggerCoordinator(contextFactoryConfig, { errorHandlingStrategy: "silent" });
      expect(c).toBeInstanceOf(TriggerCoordinator);
    });
  });

  describe("getContextFactory", () => {
    it("should return the context factory", () => {
      const factory = coordinator.getContextFactory();
      expect(factory).toBeInstanceOf(TriggerHandlerContextFactory);
    });
  });

  describe("register", () => {
    it("should register a new trigger state", () => {
      const trigger = buildTriggerDefinition();
      coordinator.register(trigger, "workflow-1");

      expect(mockStateManager.register).toHaveBeenCalledTimes(1);
      const registeredState = mockStateManager.register.mock.calls[0]![0] as TriggerRuntimeState;
      expect(registeredState.triggerId).toBe("trigger-1");
      expect(registeredState.status).toBe("enabled");
      expect(registeredState.workflowId).toBe("workflow-1");
      expect(registeredState.executionId).toBe("test-exec-1");
    });

    it("should register disabled trigger when enabled is false", () => {
      const trigger = buildTriggerDefinition({ enabled: false });
      coordinator.register(trigger, "workflow-1");

      const registeredState = mockStateManager.register.mock.calls[0]![0] as TriggerRuntimeState;
      expect(registeredState.status).toBe("disabled");
    });

    it("should throw RuntimeValidationError when trigger already exists", () => {
      const trigger = buildTriggerDefinition();
      // First registration
      mockStateManager.hasState.mockReturnValueOnce(false);
      coordinator.register(trigger, "workflow-1");

      // Second registration should throw
      mockStateManager.hasState.mockReturnValueOnce(true);
      expect(() => coordinator.register(trigger, "workflow-1")).toThrow(RuntimeValidationError);
    });
  });

  describe("unregister", () => {
    it("should unregister an existing trigger", () => {
      // Register first
      mockStateManager.hasState.mockReturnValue(true);
      mockStateManager.getState.mockReturnValue({
        triggerId: "trigger-1",
        status: "enabled" as TriggerStatus,
        triggerCount: 0,
      } as TriggerRuntimeState);

      coordinator.unregister("trigger-1");

      expect(mockStateManager.deleteState).toHaveBeenCalledWith("trigger-1");
    });

    it("should throw ExecutionError when trigger does not exist", () => {
      mockStateManager.hasState.mockReturnValue(false);

      expect(() => coordinator.unregister("non-existent")).toThrow(ExecutionError);
    });
  });

  describe("enable", () => {
    it("should enable a disabled trigger", () => {
      mockStateManager.hasState.mockReturnValue(true);
      mockStateManager.getState.mockReturnValue({
        triggerId: "trigger-1",
        status: "disabled" as TriggerStatus,
        triggerCount: 0,
      } as TriggerRuntimeState);

      coordinator.enable("trigger-1");

      expect(mockStateManager.updateStatus).toHaveBeenCalledWith("trigger-1", "enabled");
    });

    it("should skip enabling an already enabled trigger", () => {
      mockStateManager.hasState.mockReturnValue(true);
      mockStateManager.getState.mockReturnValue({
        triggerId: "trigger-1",
        status: "enabled" as TriggerStatus,
        triggerCount: 0,
      } as TriggerRuntimeState);

      coordinator.enable("trigger-1");

      expect(mockStateManager.updateStatus).not.toHaveBeenCalled();
    });

    it("should throw ExecutionError when trigger does not exist", () => {
      mockStateManager.hasState.mockReturnValue(false);

      expect(() => coordinator.enable("non-existent")).toThrow(ExecutionError);
    });
  });

  describe("disable", () => {
    it("should disable an enabled trigger", () => {
      mockStateManager.hasState.mockReturnValue(true);
      mockStateManager.getState.mockReturnValue({
        triggerId: "trigger-1",
        status: "enabled" as TriggerStatus,
        triggerCount: 0,
      } as TriggerRuntimeState);

      coordinator.disable("trigger-1");

      expect(mockStateManager.updateStatus).toHaveBeenCalledWith("trigger-1", "disabled");
    });

    it("should skip disabling an already disabled trigger", () => {
      mockStateManager.hasState.mockReturnValue(true);
      mockStateManager.getState.mockReturnValue({
        triggerId: "trigger-1",
        status: "disabled" as TriggerStatus,
        triggerCount: 0,
      } as TriggerRuntimeState);

      coordinator.disable("trigger-1");

      expect(mockStateManager.updateStatus).not.toHaveBeenCalled();
    });

    it("should throw ExecutionError when trigger does not exist", () => {
      mockStateManager.hasState.mockReturnValue(false);

      expect(() => coordinator.disable("non-existent")).toThrow(ExecutionError);
    });
  });

  describe("get", () => {
    it("should return undefined when state does not exist", () => {
      mockStateManager.getState.mockReturnValue(undefined);

      const result = coordinator.get("non-existent");

      expect(result).toBeUndefined();
    });

    it("should return undefined when trigger definition not found", () => {
      mockStateManager.getState.mockReturnValue({
        triggerId: "trigger-1",
        status: "enabled" as TriggerStatus,
        executionId: "exec-1",
        workflowId: "workflow-1",
        triggerCount: 0,
        updatedAt: Date.now(),
      });
      mockGraphRegistry.get.mockReturnValue(undefined);

      const result = coordinator.get("trigger-1");

      expect(result).toBeUndefined();
    });

    it("should return merged trigger when both state and definition exist", () => {
      const now = Date.now();
      mockStateManager.getState.mockReturnValue({
        triggerId: "trigger-1",
        status: "enabled" as TriggerStatus,
        executionId: "exec-1",
        workflowId: "workflow-1",
        triggerCount: 2,
        updatedAt: now,
      });
      mockGraphRegistry.get.mockReturnValue({
        triggers: [
          {
            id: "trigger-1",
            name: "Test Trigger",
            condition: { eventType: "NODE_COMPLETED" },
            action: { type: "set_variable", parameters: {} },
          },
        ],
      });

      const result = coordinator.get("trigger-1");

      expect(result).toBeDefined();
      expect(result!.id).toBe("trigger-1");
      expect(result!.status).toBe("enabled");
      expect(result!.triggerCount).toBe(2);
    });
  });

  describe("getAll", () => {
    it("should return all triggers", () => {
      const now = Date.now();
      mockStateManager.getAllStates.mockReturnValue(
        new Map([
          [
            "trigger-1",
            {
              triggerId: "trigger-1",
              status: "enabled" as TriggerStatus,
              executionId: "exec-1",
              workflowId: "workflow-1",
              triggerCount: 0,
              updatedAt: now,
            } as TriggerRuntimeState,
          ],
          [
            "trigger-2",
            {
              triggerId: "trigger-2",
              status: "disabled" as TriggerStatus,
              executionId: "exec-1",
              workflowId: "workflow-1",
              triggerCount: 1,
              updatedAt: now,
            } as TriggerRuntimeState,
          ],
        ]),
      );
      mockGraphRegistry.get.mockReturnValue({
        triggers: [
          {
            id: "trigger-1",
            name: "T1",
            condition: { eventType: "NODE_COMPLETED" },
            action: { type: "set_variable", parameters: {} },
          },
          {
            id: "trigger-2",
            name: "T2",
            condition: { eventType: "WORKFLOW_EXECUTION_COMPLETED" },
            action: { type: "stop_workflow_execution", parameters: {} },
          },
        ],
      });

      const results = coordinator.getAll();

      expect(results).toHaveLength(2);
      expect(results[0]!.id).toBe("trigger-1");
      expect(results[1]!.id).toBe("trigger-2");
    });

    it("should return empty array when no triggers", () => {
      mockStateManager.getAllStates.mockReturnValue(new Map());

      const results = coordinator.getAll();

      expect(results).toEqual([]);
    });
  });

  describe("handleEvent", () => {
    it("should not execute triggers when event type does not match", async () => {
      const event = buildMockEvent({ type: "WORKFLOW_EXECUTION_COMPLETED" });
      // No triggers registered
      mockStateManager.getAllStates.mockReturnValue(new Map());

      await coordinator.handleEvent(event);

      // Should not throw even with no triggers
      expect(mockStateManager.incrementTriggerCount).not.toHaveBeenCalled();
    });

    it("should handle errors according to silent strategy", async () => {
      const c = new TriggerCoordinator(contextFactoryConfig, { errorHandlingStrategy: "silent" });
      const event = buildMockEvent();

      await c.handleEvent(event);

      // Should not throw
      expect(true).toBe(true);
    });

    it("should handle errors according to log strategy", async () => {
      const c = new TriggerCoordinator(contextFactoryConfig, { errorHandlingStrategy: "log" });
      const event = buildMockEvent();

      await c.handleEvent(event);

      // Should not throw
      expect(true).toBe(true);
    });
  });

  describe("clear", () => {
    it("should clear all trigger states", () => {
      coordinator.clear();
      expect(mockStateManager.cleanup).toHaveBeenCalled();
    });
  });
});
