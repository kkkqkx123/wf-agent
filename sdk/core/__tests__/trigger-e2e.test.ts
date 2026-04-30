/**
 * Trigger End-to-End Integration Testing
 *
 * Test Scenarios:
 * - Complete trigger process
 * - Consistency of status
 * - Error handling
 * - Performance testing
 */

import { describe, it, expect, beforeEach, vi } from "vitest";
import { TriggerCoordinator } from "../../workflow/execution/coordinators/trigger-coordinator.js";
import { TriggerState } from "../../workflow/state-managers/trigger-state.js";
import { TriggerTemplateRegistry } from "../registry/trigger-template-registry.js";
import { WorkflowRegistry } from "../../workflow/stores/workflow-registry.js";
import {
  registerContextCompression,
  CONTEXT_COMPRESSION_TRIGGER_NAME,
  CONTEXT_COMPRESSION_WORKFLOW_ID,
} from "../../resources/predefined/index.js";
import type { WorkflowTrigger, BaseEvent, NodeCustomEvent } from "@wf-agent/types";
import { EventType } from "@wf-agent/types";

// Mock implementations
const mockWorkflowExecutionRegistry = {
  get: vi.fn(),
  register: vi.fn(),
  update: vi.fn(),
  delete: vi.fn(),
  setVariable: vi.fn(),
  applyMessageOperation: vi.fn(),
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
  stopWorkflowExecution: vi.fn(),
  pauseWorkflowExecution: vi.fn(),
  resumeWorkflowExecution: vi.fn(),
} as any;

const mockWorkflowExecutionBuilder = {
  build: vi.fn(),
} as any;

const mockTaskQueueManager = {
  submitSync: vi.fn(),
  submitAsync: vi.fn(),
  cancelTask: vi.fn(),
  getQueueStats: vi.fn(),
  drain: vi.fn(),
} as any;

const mockCheckpointStateManager = {
  createCheckpoint: vi.fn(),
} as any;

describe("Trigger End-to-End - End-to-End Integration Testing", () => {
  let coordinator: TriggerCoordinator;
  let stateManager: TriggerState;
  let triggerTemplateRegistry: TriggerTemplateRegistry;

  beforeEach(() => {
    vi.clearAllMocks();

    // Create a state manager
    stateManager = new TriggerState("test-execution");
    stateManager.setWorkflowId("workflow-123");

    // Create a trigger template registry
    triggerTemplateRegistry = new TriggerTemplateRegistry();

    // Create a coordinator
    coordinator = new TriggerCoordinator({
      workflowExecutionRegistry: mockWorkflowExecutionRegistry,
      workflowRegistry: mockWorkflowRegistry,
      stateManager: stateManager,
      graphRegistry: mockGraphRegistry,
      eventManager: mockEventManager,
      workflowLifecycleCoordinator: mockWorkflowLifecycleCoordinator,
      executionBuilder: mockWorkflowExecutionBuilder,
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

  describe("Full trigger process", () => {
    it("Test the entire process from event triggering to action execution.", async () => {
      // 1. Register a trigger
      const workflowTrigger: WorkflowTrigger = {
        id: "trigger-1",
        name: "Test Trigger",
        condition: {
          eventType: "WORKFLOW_EXECUTION_STARTED" as EventType,
        },
        action: {
          type: "pause_workflow_execution",
          parameters: { executionId: "test-execution" },
        },
        enabled: true,
        maxTriggers: 5,
      };

      coordinator.register(workflowTrigger, "workflow-123");

      // 2. Triggering Events
      const event: BaseEvent = {
        type: "WORKFLOW_EXECUTION_STARTED",
        executionId: "test-execution",
        workflowId: "workflow-123",
        timestamp: Date.now(),
      };

      await coordinator.handleEvent(event);

      // 3. Verify that the trigger has been executed.
      expect(mockWorkflowLifecycleCoordinator.stopWorkflowExecution).toHaveBeenCalledWith("test-execution", false);

      // 4. Verify status updates
      const trigger = coordinator.get("trigger-1");
      expect(trigger?.triggerCount).toBe(1);
      expect(trigger?.status).toBe("enabled");
    });

    it("Testing multiple triggers being triggered simultaneously", async () => {
      // Register multiple triggers that listen for the same event
      const triggers: WorkflowTrigger[] = [
        {
          id: "trigger-1",
          name: "Trigger 1",
          condition: { eventType: "WORKFLOW_EXECUTION_STARTED" as EventType },
          action: { type: "pause_workflow_execution", parameters: { executionId: "test-execution" } },
          enabled: true,
        },
        {
          id: "trigger-2",
          name: "Trigger 2",
          condition: { eventType: "WORKFLOW_EXECUTION_STARTED" as EventType },
          action: { type: "pause_workflow_execution", parameters: { executionId: "test-execution" } },
          enabled: true,
        },
        {
          id: "trigger-3",
          name: "Trigger 3",
          condition: { eventType: "WORKFLOW_EXECUTION_STARTED" as EventType },
          action: { type: "resume_workflow_execution", parameters: { executionId: "test-execution" } },
          enabled: true,
        },
      ];

      triggers.forEach(trigger => coordinator.register(trigger, "workflow-123"));

      // Trigger event
      const event: BaseEvent = {
        type: "WORKFLOW_EXECUTION_STARTED",
        executionId: "test-execution",
        workflowId: "workflow-123",
        timestamp: Date.now(),
      };

      await coordinator.handleEvent(event);

      // Verify that all triggers have been executed.
      expect(mockWorkflowLifecycleCoordinator.stopWorkflowExecution).toHaveBeenCalled();
      expect(mockWorkflowLifecycleCoordinator.pauseWorkflowExecution).toHaveBeenCalled();
      expect(mockWorkflowLifecycleCoordinator.resumeWorkflowExecution).toHaveBeenCalled();

      // Verify that the count of all triggers has increased.
      expect(coordinator.get("trigger-1")?.triggerCount).toBe(1);
      expect(coordinator.get("trigger-2")?.triggerCount).toBe(1);
      expect(coordinator.get("trigger-3")?.triggerCount).toBe(1);
    });

    it("Testing trigger chain triggering", async () => {
      // Register Trigger A: Stop the workflow execution
      const triggerA: WorkflowTrigger = {
        id: "trigger-a",
        name: "Trigger A",
        condition: { eventType: "WORKFLOW_EXECUTION_STARTED" as EventType },
        action: { type: "pause_workflow_execution", parameters: { executionId: "test-execution" } },
        enabled: true,
      };

      coordinator.register(triggerA, "workflow-123");

      // Register Trigger B: Pause the workflow execution (assuming it is triggered after stopping).
      const triggerB: WorkflowTrigger = {
        id: "trigger-b",
        name: "Trigger B",
        condition: { eventType: "WORKFLOW_EXECUTION_PAUSED" as EventType },
        action: { type: "pause_workflow_execution", parameters: { executionId: "test-execution" } },
        enabled: true,
      };

      coordinator.register(triggerB, "workflow-123");

      // Simulate triggering the first event.
      const event1: BaseEvent = {
        type: "WORKFLOW_EXECUTION_STARTED",
        executionId: "test-execution",
        workflowId: "workflow-123",
        timestamp: Date.now(),
      };

      await coordinator.handleEvent(event1);

      // Verify that trigger A has been executed.
      expect(mockWorkflowLifecycleCoordinator.stopWorkflowExecution).toHaveBeenCalled();

      // Simulate triggering the second event.
      const event2: BaseEvent = {
        type: "WORKFLOW_EXECUTION_PAUSED",
        executionId: "test-execution",
        workflowId: "workflow-123",
        timestamp: Date.now(),
      };

      await coordinator.handleEvent(event2);

      // Verify that trigger B has been executed.
      expect(mockWorkflowLifecycleCoordinator.pauseWorkflowExecution).toHaveBeenCalled();
    });
  });

  describe("State consistency", () => {
    it("The status is updated correctly after the test trigger is activated.", async () => {
      const workflowTrigger: WorkflowTrigger = {
        id: "trigger-1",
        name: "Test Trigger",
        condition: { eventType: "WORKFLOW_EXECUTION_STARTED" as EventType },
        action: { type: "pause_workflow_execution", parameters: { executionId: "test-execution" } },
        enabled: true,
        maxTriggers: 3,
      };

      coordinator.register(workflowTrigger, "workflow-123");

      // First trigger
      const event: BaseEvent = {
        type: "WORKFLOW_EXECUTION_STARTED",
        executionId: "test-execution",
        workflowId: "workflow-123",
        timestamp: Date.now(),
      };

      await coordinator.handleEvent(event);

      let trigger = coordinator.get("trigger-1");
      expect(trigger?.triggerCount).toBe(1);
      expect(trigger?.status).toBe("enabled");

      // Second trigger
      await coordinator.handleEvent(event);

      trigger = coordinator.get("trigger-1");
      expect(trigger?.triggerCount).toBe(2);

      // Third trigger
      await coordinator.handleEvent(event);

      trigger = coordinator.get("trigger-1");
      expect(trigger?.triggerCount).toBe(3);
      expect(trigger?.status).toBe("disabled"); // Automatically disable after reaching the maximum number of attempts.
    });

    it("Testing the consistency of states when concurrency is triggered.", async () => {
      const workflowTrigger: WorkflowTrigger = {
        id: "trigger-1",
        name: "Test Trigger",
        condition: { eventType: "WORKFLOW_EXECUTION_STARTED" as EventType },
        action: { type: "pause_workflow_execution", parameters: { executionId: "test-execution" } },
        enabled: true,
        maxTriggers: 10,
      };

      coordinator.register(workflowTrigger, "workflow-123");

      const event: BaseEvent = {
        type: "WORKFLOW_EXECUTION_STARTED",
        executionId: "test-execution",
        workflowId: "workflow-123",
        timestamp: Date.now(),
      };

      // Concurrent triggering occurs multiple times.
      const promises = Array(5)
        .fill(null)
        .map(() => coordinator.handleEvent(event));

      await Promise.all(promises);

      const trigger = coordinator.get("trigger-1");
      expect(trigger?.triggerCount).toBe(5);
    });

    it("Test the consistency of the snapshot with the state after recovery.", () => {
      const workflowTrigger: WorkflowTrigger = {
        id: "trigger-1",
        name: "Test Trigger",
        condition: { eventType: "WORKFLOW_EXECUTION_STARTED" as EventType },
        action: { type: "pause_workflow_execution", parameters: { executionId: "test-execution" } },
        enabled: true,
        maxTriggers: 5,
      };

      coordinator.register(workflowTrigger, "workflow-123");

      // Create a snapshot
      const snapshot = stateManager.createSnapshot();

      // Modify status
      stateManager.updateStatus("trigger-1", "disabled");
      stateManager.incrementTriggerCount("trigger-1");

      // Recover the snapshot
      stateManager.restoreFromSnapshot(snapshot);

      // Verification status has been restored.
      const state = stateManager.getState("trigger-1");
      expect(state?.status).toBe("enabled");
      expect(state?.triggerCount).toBe(0);
    });
  });

  describe("Error Handling", () => {
    it("Error handling when the test trigger execution fails", async () => {
      const workflowTrigger: WorkflowTrigger = {
        id: "trigger-1",
        name: "Test Trigger",
        condition: { eventType: "WORKFLOW_EXECUTION_STARTED" as EventType },
        action: { type: "pause_workflow_execution", parameters: { executionId: "test-execution" } },
        enabled: true,
      };

      coordinator.register(workflowTrigger, "workflow-123");

      // Simulator processor failed.
      mockWorkflowLifecycleCoordinator.stopWorkflowExecution.mockRejectedValue(new Error("Stop failed"));

      const event: BaseEvent = {
        type: "WORKFLOW_EXECUTION_STARTED",
        executionId: "test-execution",
        workflowId: "workflow-123",
        timestamp: Date.now(),
      };

      // No errors should be thrown; errors should be handled silently.
      await expect(coordinator.handleEvent(event)).resolves.not.toThrow();
    });

    it("The failure of a trigger in the testing section does not affect the other triggers.", async () => {
      const triggers: WorkflowTrigger[] = [
        {
          id: "trigger-1",
          name: "Failed trigger",
          condition: { eventType: "WORKFLOW_EXECUTION_STARTED" as EventType },
          action: { type: "pause_workflow_execution", parameters: { executionId: "test-execution" } },
          enabled: true,
        },
        {
          id: "trigger-2",
          name: "Successful trigger",
          condition: { eventType: "WORKFLOW_EXECUTION_STARTED" as EventType },
          action: { type: "pause_workflow_execution", parameters: { executionId: "test-execution" } },
          enabled: true,
        },
      ];

      triggers.forEach(trigger => coordinator.register(trigger, "workflow-123"));

      // Simulation of the first trigger failed.
      mockWorkflowLifecycleCoordinator.stopWorkflowExecution.mockRejectedValue(new Error("Failed"));

      const event: BaseEvent = {
        type: "WORKFLOW_EXECUTION_STARTED",
        executionId: "test-execution",
        workflowId: "workflow-123",
        timestamp: Date.now(),
      };

      await coordinator.handleEvent(event);

      // Verify that the second trigger is still being executed.
      expect(mockWorkflowLifecycleCoordinator.pauseWorkflowExecution).toHaveBeenCalled();
    });

    it("Error message when test dependencies are missing", async () => {
      // Create a coordinator without any necessary dependencies.
      const incompleteCoordinator = new TriggerCoordinator({
        workflowExecutionRegistry: mockWorkflowExecutionRegistry,
        workflowRegistry: mockWorkflowRegistry,
        stateManager: stateManager,
        // Missing workflowLifecycleCoordinator
      });

      const workflowTrigger: WorkflowTrigger = {
        id: "trigger-1",
        name: "Test Trigger",
        condition: { eventType: "WORKFLOW_EXECUTION_STARTED" as EventType },
        action: { type: "pause_workflow_execution", parameters: { executionId: "test-workflow-execution" } },
        enabled: true,
      };

      incompleteCoordinator.register(workflowTrigger, "workflow-123");

      const event: BaseEvent = {
        type: "WORKFLOW_EXECUTION_STARTED",
        executionId: "test-workflow-execution",
        workflowId: "workflow-123",
        timestamp: Date.now(),
      };

      // A dependency injection error should be thrown.
      await expect(incompleteCoordinator.handleEvent(event)).rejects.toThrow();
    });
  });

  describe("Performance Testing", () => {
    it("Testing the matching performance of a large number of triggers", async () => {
      // Registering a large number of triggers
      const triggerCount = 100;
      const triggers: WorkflowTrigger[] = [];

      for (let i = 0; i < triggerCount; i++) {
        triggers.push({
          id: `trigger-${i}`,
          name: `trigger ${i}`,
          condition: { eventType: "WORKFLOW_EXECUTION_STARTED" as EventType },
          action: { type: "pause_workflow_execution", parameters: { executionId: "test-execution" } },
          enabled: true,
        });
      }

      triggers.forEach(trigger => coordinator.register(trigger, "workflow-123"));

      const event: BaseEvent = {
        type: "WORKFLOW_EXECUTION_STARTED",
        executionId: "test-execution",
        workflowId: "workflow-123",
        timestamp: Date.now(),
      };

      // Measure execution time
      const startTime = Date.now();
      await coordinator.handleEvent(event);
      const executionTime = Date.now() - startTime;

      // Verify that it can be completed within a reasonable amount of time (<1 second).
      expect(executionTime).toBeLessThan(1000);
    });

    it("Testing the performance of handling high-frequency events", async () => {
      const workflowTrigger: WorkflowTrigger = {
        id: "trigger-1",
        name: "Test Trigger",
        condition: { eventType: "WORKFLOW_EXECUTION_STARTED" as EventType },
        action: { type: "pause_workflow_execution", parameters: { executionId: "test-execution" } },
        enabled: true,
      };

      coordinator.register(workflowTrigger, "workflow-123");

      const eventCount = 50;
      const events: BaseEvent[] = [];

      for (let i = 0; i < eventCount; i++) {
        events.push({
          type: "WORKFLOW_EXECUTION_STARTED",
          executionId: "test-execution",
          workflowId: "workflow-123",
          timestamp: Date.now(),
        });
      }

      // Measure execution time
      const startTime = Date.now();
      await Promise.all(events.map(event => coordinator.handleEvent(event)));
      const executionTime = Date.now() - startTime;

      // Verify that it can be completed within a reasonable amount of time (< 2 seconds).
      expect(executionTime).toBeLessThan(2000);

      // Verify that all triggers have been counted.
      const trigger = coordinator.get("trigger-1");
      expect(trigger?.triggerCount).toBe(eventCount);
    });

    it("Testing the performance of concurrent triggers", async () => {
      const workflowTrigger: WorkflowTrigger = {
        id: "trigger-1",
        name: "Test Trigger",
        condition: { eventType: "WORKFLOW_EXECUTION_STARTED" as EventType },
        action: { type: "pause_workflow_execution", parameters: { executionId: "test-execution" } },
        enabled: true,
        maxTriggers: 100,
      };

      coordinator.register(workflowTrigger, "workflow-123");

      const event: BaseEvent = {
        type: "WORKFLOW_EXECUTION_STARTED",
        executionId: "test-execution",
        workflowId: "workflow-123",
        timestamp: Date.now(),
      };

      // Concurrent triggering
      const startTime = Date.now();
      await Promise.all(
        Array(20)
          .fill(null)
          .map(() => coordinator.handleEvent(event)),
      );
      const executionTime = Date.now() - startTime;

      // Verify that it can be completed within a reasonable amount of time (<1 second).
      expect(executionTime).toBeLessThan(1000);
    });
  });

  describe("Predefined Trigger Integration Testing", () => {
    it("Testing the registration and use of predefined triggers", async () => {
      // Register predefined triggers and workflows
      const result = registerContextCompression(triggerTemplateRegistry, mockWorkflowRegistry);

      expect(result.triggerRegistered).toBe(true);
      expect(result.workflowRegistered).toBe(true);

      // Verify that the trigger template has been registered.
      expect(triggerTemplateRegistry.has(CONTEXT_COMPRESSION_TRIGGER_NAME)).toBe(true);
      expect(mockWorkflowRegistry.has(CONTEXT_COMPRESSION_WORKFLOW_ID)).toBe(true);

      // Get trigger template
      const triggerTemplate = triggerTemplateRegistry.get(CONTEXT_COMPRESSION_TRIGGER_NAME);
      expect(triggerTemplate).toBeDefined();
      expect(triggerTemplate?.condition.eventType).toBe("CONTEXT_COMPRESSION_REQUESTED");
      expect(triggerTemplate?.action.type).toBe("execute_triggered_subgraph");
    });
  });

  describe("NODE_CUSTOM_EVENT Event Testing", () => {
    it("Test the correct matching and execution of the NODE_CUSTOM_EVENT event.", async () => {
      const workflowTrigger: WorkflowTrigger = {
        id: "custom-trigger-1",
        name: "Custom event trigger",
        condition: {
          eventType: "NODE_CUSTOM_EVENT" as EventType,
          eventName: "my-custom-event",
        },
        action: {
          type: "pause_workflow_execution",
          parameters: { executionId: "test-execution" },
        },
        enabled: true,
      };

      coordinator.register(workflowTrigger, "workflow-123");

      const event: NodeCustomEvent = {
        type: "NODE_CUSTOM_EVENT",
        executionId: "test-execution",
        workflowId: "workflow-123",
        nodeId: "node-1",
        nodeType: "LLM",
        eventName: "my-custom-event",
        eventData: {},
        timestamp: Date.now(),
      };

      await coordinator.handleEvent(event);

      // Verify that the trigger has been executed.
      expect(mockWorkflowLifecycleCoordinator.stopWorkflowExecution).toHaveBeenCalled();
      expect(coordinator.get("custom-trigger-1")?.triggerCount).toBe(1);
    });
  });

  describe("Clean and reset", () => {
    it("Test to clear all trigger statuses.", () => {
      const triggers: WorkflowTrigger[] = [
        {
          id: "trigger-1",
          name: "Trigger 1",
          condition: { eventType: "WORKFLOW_EXECUTION_STARTED" as EventType },
          action: { type: "pause_workflow_execution", parameters: { executionId: "test-execution" } },
          enabled: true,
        },
        {
          id: "trigger-2",
          name: "Trigger 2",
          condition: { eventType: "WORKFLOW_EXECUTION_COMPLETED" as EventType },
          action: { type: "pause_workflow_execution", parameters: { executionId: "test-execution" } },
          enabled: true,
        },
      ];

      triggers.forEach(trigger => coordinator.register(trigger, "workflow-123"));

      expect(stateManager.size()).toBe(2);

      coordinator.clear();

      expect(stateManager.size()).toBe(0);
    });
  });
});
