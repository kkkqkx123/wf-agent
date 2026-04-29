/**
 * Trigger Coordinator Integration Testing
 *
 * Test Scenarios:
 * - Registration and Logout
 * - Activation and Deactivation
 * - Query Functions
 * - Event Handling
 * - Checkpoint Support
 */

import { describe, it, expect, beforeEach, vi } from "vitest";
import { TriggerCoordinator } from "../../workflow/execution/coordinators/trigger-coordinator.js";
import { TriggerState } from "../../workflow/state-managers/trigger-state.js";
import { WorkflowRegistry } from "../../workflow/workflow/workflow-registry.js";
import { WorkflowGraphRegistry } from "../../workflow/graph-structure/graph-registry.js";
import { WorkflowExecutionRegistry } from "../../workflow/stores/thread-registry.js";
import type { WorkflowTrigger, Trigger, BaseEvent, NodeCustomEvent } from "@wf-agent/types";
import { EventType } from "@wf-agent/types";
import { ExecutionError, RuntimeValidationError } from "@wf-agent/types";

// Mock implementations
const mockThreadRegistry = {
  get: vi.fn(),
  register: vi.fn(),
  update: vi.fn(),
  delete: vi.fn(),
} as any;

const mockGraphRegistry = {
  get: vi.fn(),
} as any;

const mockWorkflowRegistry = new WorkflowRegistry();

const mockEventManager = {
  emit: vi.fn(),
  on: vi.fn(),
  off: vi.fn(),
} as any;

const mockWorkflowLifecycleCoordinator = {
  stopThread: vi.fn(),
  pauseThread: vi.fn(),
  resumeThread: vi.fn(),
} as any;

const mockWorkflowExecutionBuilder = {
  build: vi.fn(),
} as any;

const mockTaskQueueManager = {
  submitSync: vi.fn(),
  submitAsync: vi.fn(),
} as any;

const mockCheckpointStateManager = {
  createCheckpoint: vi.fn(),
} as any;

describe("Trigger Coordinator", () => {
  let coordinator: TriggerCoordinator;
  let stateManager: TriggerState;

  beforeEach(() => {
    // Clean up the mocks.
    vi.clearAllMocks();

    // Create a state manager
    stateManager = new TriggerState("test-thread");
    stateManager.setWorkflowId("workflow-123");

    // Create a coordinator
    coordinator = new TriggerCoordinator({
      workflowExecutionRegistry: mockThreadRegistry,
      workflowRegistry: mockWorkflowRegistry,
      stateManager: stateManager,
      workflowGraphRegistry: mockGraphRegistry,
      eventManager: mockEventManager,
      threadLifecycleCoordinator: mockWorkflowLifecycleCoordinator,
      threadBuilder: mockWorkflowExecutionBuilder,
      taskQueueManager: mockTaskQueueManager,
      checkpointStateManager: mockCheckpointStateManager,
    });

    // Register a test workflow
    const testWorkflow = {
      id: "workflow-123",
      name: "Test Workflow",
      version: "1.0.0",
      type: "STANDALONE" as const,
      description: "Test workflow",
      nodes: [],
      edges: [],
      metadata: {},
      triggers: [],
      createdAt: Date.now(),
      updatedAt: Date.now(),
    };

    mockWorkflowRegistry.register(testWorkflow);
  });

  describe("Register and Logout", () => {
    it("Testing the registration trigger: The register method correctly initializes the runtime state.", () => {
      const workflowTrigger: WorkflowTrigger = {
        id: "trigger-1",
        name: "Test Trigger",
        condition: {
          eventType: "THREAD_STARTED" as EventType,
        },
        action: {
          type: "pause_thread",
          parameters: { threadId: "test-thread" },
        },
        enabled: true,
        maxTriggers: 5,
      };

      coordinator.register(workflowTrigger, "workflow-123");

      expect(stateManager.hasState("trigger-1")).toBe(true);
      const state = stateManager.getState("trigger-1");
      expect(state?.triggerId).toBe("trigger-1");
      expect(state?.threadId).toBe("test-thread");
      expect(state?.workflowId).toBe("workflow-123");
      expect(state?.status).toBe("enabled");
      expect(state?.triggerCount).toBe(0);
    });

    it("Testing duplicate registration: Duplicate registration should throw an error.", () => {
      const workflowTrigger: WorkflowTrigger = {
        id: "trigger-1",
        name: "Test Trigger",
        condition: {
          eventType: "THREAD_STARTED" as EventType,
        },
        action: {
          type: "pause_thread",
          parameters: { threadId: "test-thread" },
        },
        enabled: true,
      };

      coordinator.register(workflowTrigger, "workflow-123");

      expect(() => {
        coordinator.register(workflowTrigger, "workflow-123");
      }).toThrow(RuntimeValidationError);
    });

    it("Testing the logout trigger: The unregister method correctly removes the status.", () => {
      const workflowTrigger: WorkflowTrigger = {
        id: "trigger-1",
        name: "Test Trigger",
        condition: {
          eventType: "THREAD_STARTED" as EventType,
        },
        action: {
          type: "pause_thread",
          parameters: { threadId: "test-thread" },
        },
        enabled: true,
      };

      coordinator.register(workflowTrigger, "workflow-123");
      expect(stateManager.hasState("trigger-1")).toBe(true);

      coordinator.unregister("trigger-1");

      expect(stateManager.hasState("trigger-1")).toBe(false);
    });

    it("Testing the cancellation of a non-existent trigger: An error should be thrown.", () => {
      expect(() => {
        coordinator.unregister("non-existent");
      }).toThrow(ExecutionError);
    });
  });

  describe("Enable and disable", () => {
    beforeEach(() => {
      const workflowTrigger: WorkflowTrigger = {
        id: "trigger-1",
        name: "Test Trigger",
        condition: {
          eventType: "THREAD_STARTED" as EventType,
        },
        action: {
          type: "pause_thread",
          parameters: { threadId: "test-thread" },
        },
        enabled: true,
      };

      coordinator.register(workflowTrigger, "workflow-123");
    });

    it("Testing trigger activation: The enable method correctly updates the status.", () => {
      // Disable it first.
      stateManager.updateStatus("trigger-1", "disabled");
      expect(stateManager.getState("trigger-1")?.status).toBe("disabled");

      // Re-enable
      coordinator.enable("trigger-1");

      expect(stateManager.getState("trigger-1")?.status).toBe("enabled");
    });

    it("Testing trigger disabling: The disable method correctly updates the status.", () => {
      expect(stateManager.getState("trigger-1")?.status).toBe("enabled");

      coordinator.disable("trigger-1");

      expect(stateManager.getState("trigger-1")?.status).toBe("disabled");
    });

    it("Test the triggered event that has been enabled: Updates should not be repeated.", () => {
      const state = stateManager.getState("trigger-1");
      const originalStatus = state?.status;

      coordinator.enable("trigger-1");

      expect(stateManager.getState("trigger-1")?.status).toBe(originalStatus);
    });

    it("Testing disabling a disabled trigger: Updates should not be repeated.", () => {
      coordinator.disable("trigger-1");
      const state = stateManager.getState("trigger-1");
      const originalStatus = state?.status;

      coordinator.disable("trigger-1");

      expect(stateManager.getState("trigger-1")?.status).toBe(originalStatus);
    });

    it("Testing the activation of a non-existent trigger: An error should be thrown.", () => {
      expect(() => {
        coordinator.enable("non-existent");
      }).toThrow(ExecutionError);
    });

    it("Testing the disabling of a non-existent trigger: An error should be thrown.", () => {
      expect(() => {
        coordinator.disable("non-existent");
      }).toThrow(ExecutionError);
    });
  });

  describe("Query function", () => {
    beforeEach(() => {
      const workflowTrigger: WorkflowTrigger = {
        id: "trigger-1",
        name: "Test Trigger",
        description: "This is a test trigger.",
        condition: {
          eventType: "THREAD_STARTED" as EventType,
          eventName: "custom-event",
        },
        action: {
          type: "pause_thread",
          parameters: { threadId: "test-thread" },
        },
        enabled: true,
        maxTriggers: 5,
        metadata: { key: "value" },
      };

      coordinator.register(workflowTrigger, "workflow-123");

      // Simulate triggering one time
      stateManager.incrementTriggerCount("trigger-1");
    });

    it("Testing trigger retrieval: The get method returns a merged result of the definition and status.", () => {
      const trigger = coordinator.get("trigger-1");

      expect(trigger).toBeDefined();
      expect(trigger?.id).toBe("trigger-1");
      expect(trigger?.name).toBe("Test Trigger");
      expect(trigger?.description).toBe("This is a test trigger.");
      expect(trigger?.condition.eventType).toBe("THREAD_STARTED");
      expect(trigger?.condition.eventName).toBe("custom-event");
      expect(trigger?.action.type).toBe("pause_thread");
      expect(trigger?.enabled).toBe(true);
      expect(trigger?.maxTriggers).toBe(5);
      expect(trigger?.metadata).toEqual({ key: "value" });
      expect(trigger?.status).toBe("enabled");
      expect(trigger?.triggerCount).toBe(1);
      expect(trigger?.threadId).toBe("test-thread");
    });

    it("Test to retrieve all triggers: The getAll method returns all triggers.", () => {
      const triggers = coordinator.getAll();

      expect(triggers).toHaveLength(1);
      expect(triggers[0]?.id).toBe("trigger-1");
    });

    it("Testing for the retrieval of a non-existent trigger: Returns undefined.", () => {
      const trigger = coordinator.get("non-existent");

      expect(trigger).toBeUndefined();
    });

    it("Testing the retrieval of multiple triggers: getAll returns all registered triggers.", () => {
      const workflowTrigger2: WorkflowTrigger = {
        id: "trigger-2",
        name: "Test Trigger 2",
        condition: {
          eventType: "THREAD_COMPLETED" as EventType,
        },
        action: {
          type: "pause_thread",
          parameters: { threadId: "test-thread" },
        },
        enabled: true,
      };

      coordinator.register(workflowTrigger2, "workflow-123");

      const triggers = coordinator.getAll();

      expect(triggers).toHaveLength(2);
      expect(triggers.map(t => t?.id)).toContain("trigger-1");
      expect(triggers.map(t => t?.id)).toContain("trigger-2");
    });
  });

  describe("Event handling", () => {
    beforeEach(() => {
      const workflowTrigger1: WorkflowTrigger = {
        id: "trigger-1",
        name: "Matching Triggers",
        condition: {
          eventType: "THREAD_STARTED" as EventType,
        },
        action: {
          type: "pause_thread",
          parameters: { threadId: "test-thread" },
        },
        enabled: true,
        maxTriggers: 5,
      };

      const workflowTrigger2: WorkflowTrigger = {
        id: "trigger-2",
        name: "Mismatching triggers",
        condition: {
          eventType: "THREAD_COMPLETED" as EventType,
        },
        action: {
          type: "pause_thread",
          parameters: { threadId: "test-thread" },
        },
        enabled: true,
      };

      const workflowTrigger3: WorkflowTrigger = {
        id: "trigger-3",
        name: "Disabled triggers",
        condition: {
          eventType: "THREAD_STARTED" as EventType,
        },
        action: {
          type: "resume_thread",
          parameters: { threadId: "test-thread" },
        },
        enabled: false,
      };

      coordinator.register(workflowTrigger1, "workflow-123");
      coordinator.register(workflowTrigger2, "workflow-123");
      coordinator.register(workflowTrigger3, "workflow-123");
    });

    it("Test matching event: The handleEvent method correctly matches and executes the trigger.", async () => {
      const event: BaseEvent = {
        type: "THREAD_STARTED",
        threadId: "test-thread",
        workflowId: "workflow-123",
        timestamp: Date.now(),
      };

      await coordinator.handleEvent(event);

      // Verify that the matched trigger was executed.
      expect(mockWorkflowLifecycleCoordinator.stopThread).toHaveBeenCalled();
    });

    it("Test mismatch events: Mismatching events should not be triggered.", async () => {
      const event: BaseEvent = {
        type: "NODE_STARTED",
        threadId: "test-thread",
        workflowId: "workflow-123",
        timestamp: Date.now(),
      };

      await coordinator.handleEvent(event);

      // Verify that no triggers were executed.
      expect(mockWorkflowLifecycleCoordinator.stopThread).not.toHaveBeenCalled();
    });

    it("Testing that disabling triggers does not cause them to execute: Triggers that are in a disabled state should not be executed.", async () => {
      const event: BaseEvent = {
        type: "THREAD_STARTED",
        threadId: "test-thread",
        workflowId: "workflow-123",
        timestamp: Date.now(),
      };

      await coordinator.handleEvent(event);

      // Verify that the disabled triggers have not been executed.
      expect(mockWorkflowLifecycleCoordinator.resumeThread).not.toHaveBeenCalled();
    });

    it("Test should not execute when the maximum number of attempts is reached: Triggers that have reached the maxTriggers value should not be executed.", async () => {
      // Simulate reaching the maximum number of triggers.
      for (let i = 0; i < 5; i++) {
        stateManager.incrementTriggerCount("trigger-1");
      }

      const event: BaseEvent = {
        type: "THREAD_STARTED",
        threadId: "test-thread",
        workflowId: "workflow-123",
        timestamp: Date.now(),
      };

      await coordinator.handleEvent(event);

      // Verify that triggers that have reached the maximum number of executions have not been executed.
      expect(mockWorkflowLifecycleCoordinator.stopThread).not.toHaveBeenCalled();
    });

    it("Testing the NODE_CUSTOM_EVENT event: Correct matching of eventName", async () => {
      const customTrigger: WorkflowTrigger = {
        id: "custom-trigger",
        name: "Custom event trigger",
        condition: {
          eventType: "NODE_CUSTOM_EVENT" as EventType,
          eventName: "my-custom-event",
        },
        action: {
          type: "pause_thread",
          parameters: { threadId: "test-thread" },
        },
        enabled: true,
      };

      coordinator.register(customTrigger, "workflow-123");

      const event: NodeCustomEvent = {
        type: "NODE_CUSTOM_EVENT",
        threadId: "test-thread",
        workflowId: "workflow-123",
        nodeId: "node-1",
        nodeType: "LLM",
        eventName: "my-custom-event",
        eventData: {},
        timestamp: Date.now(),
      };

      await coordinator.handleEvent(event);

      // Verify that the matched trigger has been executed.
      expect(mockWorkflowLifecycleCoordinator.stopThread).toHaveBeenCalled();
    });

    it("Test for NODE_CUSTOM_EVENT not matching eventName: Do not execute if eventName does not match.", async () => {
      const customTrigger: WorkflowTrigger = {
        id: "custom-trigger",
        name: "Custom event trigger",
        condition: {
          eventType: "NODE_CUSTOM_EVENT" as EventType,
          eventName: "my-custom-event",
        },
        action: {
          type: "pause_thread",
          parameters: { threadId: "test-thread" },
        },
        enabled: true,
      };

      coordinator.register(customTrigger, "workflow-123");

      const event: NodeCustomEvent = {
        type: "NODE_CUSTOM_EVENT",
        threadId: "test-thread",
        workflowId: "workflow-123",
        nodeId: "node-1",
        nodeType: "LLM",
        eventName: "different-event",
        eventData: {},
        timestamp: Date.now(),
      };

      await coordinator.handleEvent(event);

      // Verify that unmatched triggers were not executed.
      expect(mockWorkflowLifecycleCoordinator.stopThread).not.toHaveBeenCalled();
    });
  });

  describe("Checkpoint support", () => {
    beforeEach(() => {
      const workflowTrigger: WorkflowTrigger = {
        id: "trigger-1",
        name: "Test Trigger",
        condition: {
          eventType: "THREAD_STARTED" as EventType,
        },
        action: {
          type: "pause_thread",
          parameters: { threadId: "test-thread" },
        },
        enabled: true,
        createCheckpoint: true,
        checkpointDescription: "Checkpoint description",
      };

      coordinator.register(workflowTrigger, "workflow-123");
    });

    it("Create a checkpoint before the test is triggered: The checkpoint is created correctly using the createCheckpoint configuration.", async () => {
      const event: BaseEvent = {
        type: "THREAD_STARTED",
        threadId: "test-thread",
        workflowId: "workflow-123",
        timestamp: Date.now(),
      };

      await coordinator.handleEvent(event);

      // The verification checkpoint has been created.
      expect(mockCheckpointStateManager.createCheckpoint).toHaveBeenCalled();
    });

    it("The failure to create a test checkpoint does not affect execution: The failure of a checkpoint should not impact the execution of the trigger.", async () => {
      // Simulation checkpoint creation failed.
      mockCheckpointStateManager.createCheckpoint.mockRejectedValue(new Error("Checkpoint failed"));

      const event: BaseEvent = {
        type: "THREAD_STARTED",
        threadId: "test-thread",
        workflowId: "workflow-123",
        timestamp: Date.now(),
      };

      // No errors should be thrown.
      await expect(coordinator.handleEvent(event)).resolves.not.toThrow();

      // Verify that the trigger is still being executed.
      expect(mockWorkflowLifecycleCoordinator.stopThread).toHaveBeenCalled();
    });
  });

  describe("Clear Function", () => {
    it("Test clearing all trigger states: The clear method clears all statuses.", () => {
      const workflowTrigger1: WorkflowTrigger = {
        id: "trigger-1",
        name: "Trigger 1",
        condition: { eventType: "THREAD_STARTED" as EventType },
        action: { type: "pause_thread", parameters: { threadId: "test-thread" } },
        enabled: true,
      };

      const workflowTrigger2: WorkflowTrigger = {
        id: "trigger-2",
        name: "Trigger 2",
        condition: { eventType: "THREAD_COMPLETED" as EventType },
        action: { type: "pause_thread", parameters: { threadId: "test-thread" } },
        enabled: true,
      };

      coordinator.register(workflowTrigger1, "workflow-123");
      coordinator.register(workflowTrigger2, "workflow-123");

      expect(stateManager.size()).toBe(2);

      coordinator.clear();

      expect(stateManager.size()).toBe(0);
      expect(stateManager.getAllStates().size).toBe(0);
    });
  });

  describe("Boundary cases", () => {
    it("Test handling of events without triggers: should be handled normally", async () => {
      const event: BaseEvent = {
        type: "THREAD_STARTED",
        threadId: "test-thread",
        workflowId: "workflow-123",
        timestamp: Date.now(),
      };

      // No errors should be thrown.
      await expect(coordinator.handleEvent(event)).resolves.not.toThrow();
    });

    it("Test processing multiple matching triggers: all matching triggers should be executed", async () => {
      const workflowTrigger1: WorkflowTrigger = {
        id: "trigger-1",
        name: "Trigger 1",
        condition: { eventType: "THREAD_STARTED" as EventType },
        action: { type: "pause_thread", parameters: { threadId: "test-thread" } },
        enabled: true,
      };

      const workflowTrigger2: WorkflowTrigger = {
        id: "trigger-2",
        name: "Trigger 2",
        condition: { eventType: "THREAD_STARTED" as EventType },
        action: { type: "pause_thread", parameters: { threadId: "test-thread" } },
        enabled: true,
      };

      coordinator.register(workflowTrigger1, "workflow-123");
      coordinator.register(workflowTrigger2, "workflow-123");

      const event: BaseEvent = {
        type: "THREAD_STARTED",
        threadId: "test-thread",
        workflowId: "workflow-123",
        timestamp: Date.now(),
      };

      await coordinator.handleEvent(event);

      // Verify that both triggers have been executed.
      expect(mockWorkflowLifecycleCoordinator.stopThread).toHaveBeenCalled();
      expect(mockWorkflowLifecycleCoordinator.pauseThread).toHaveBeenCalled();
    });
  });
});
