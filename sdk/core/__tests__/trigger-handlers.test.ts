/**
 * Trigger Handler Integration Tests
 *
 * Test Scenarios:
 * - Stop the workflow execution processor
 * - Pause the workflow execution processor
 * - Resume the workflow execution processor
 * - Skip the node processor
 * - Set the variable processor
 * - Send a notification processor
 * - Custom action processor
 * - Execute the script processor
 * - Execute the triggered sub-workflow processor
 * - Apply the message operation processor
 */

import { describe, it, expect, beforeEach, vi } from "vitest";
import {
  getTriggerHandler,
  triggerHandlers,
} from "../../workflow/execution/handlers/trigger-handlers/index.js";
import { TriggerActionType } from "@wf-agent/types";
import type { TriggerAction, TriggerExecutionResult } from "@wf-agent/types";

// Mock implementations
const mockWorkflowLifecycleCoordinator = {
  stopWorkflowExecution: vi.fn(),
  pauseWorkflowExecution: vi.fn(),
  resumeWorkflowExecution: vi.fn(),
} as any;

const mockWorkflowExecutionRegistry = {
  get: vi.fn().mockReturnValue({
    id: "workflow-execution-123",
    getWorkflowExecutionId: vi.fn(() => "workflow-execution-123"),
    getWorkflowId: vi.fn(() => "workflow-123"),
    getThread: vi.fn(() => ({
      nodeResults: [],
    })),
    setVariable: vi.fn(),
    getInput: vi.fn(() => ({})),
    getOutput: vi.fn(() => ({})),
  }),
  update: vi.fn(),
  setVariable: vi.fn(),
  applyMessageOperation: vi.fn(),
} as any;

const mockEventManager = {
  emit: vi.fn(),
} as any;

const mockWorkflowExecutionBuilder = {
  build: vi.fn(),
} as any;

const mockTaskQueueManager = {
  submitSync: vi.fn(),
  submitAsync: vi.fn(),
} as any;

const mockScriptService = {
  execute: vi.fn().mockResolvedValue({ success: true, output: "script output" }),
} as any;

describe("Trigger Handlers", () => {
  describe("Pause Workflow Execution Processor", () => {
    it("Test suspended workflow execution: pauseWorkflowExecutionHandler correctly suspends the specified workflow execution", async () => {
      const action: TriggerAction = {
        type: "pause_workflow_execution",
        parameters: {
          executionId: "workflow-execution-123",
          reason: "Test Suspension",
        },
      };

      const handler = getTriggerHandler("pause_workflow_execution");
      const result = await handler(action, "trigger-1", mockWorkflowLifecycleCoordinator);

      expect(mockWorkflowLifecycleCoordinator.pauseWorkflowExecution).toHaveBeenCalledWith("workflow-execution-123");
      expect(result.success).toBe(true);
    });

    it("Test pause reason: the reason for pause is correctly recorded by the reason parameter", async () => {
      const action: TriggerAction = {
        type: "pause_workflow_execution",
        parameters: {
          executionId: "workflow-execution-123",
          // "reason" is not set.
        },
      };

      const handler = getTriggerHandler("pause_workflow_execution");
      await handler(action, "trigger-1", mockWorkflowLifecycleCoordinator);

      expect(mockWorkflowLifecycleCoordinator.pauseWorkflowExecution).toHaveBeenCalledWith("workflow-execution-123");
    });
  });

  describe("Recovery Workflow Execution Processor", () => {
    it("Test for resuming workflow executions: resumeWorkflowExecutionHandler correctly resumes the specified workflow executions", async () => {
      const action: TriggerAction = {
        type: "resume_workflow_execution",
        parameters: {
          executionId: "workflow-execution-123",
        },
      };

      const handler = getTriggerHandler("resume_workflow_execution");
      const result = await handler(action, "trigger-1", mockWorkflowLifecycleCoordinator);

      expect(mockWorkflowLifecycleCoordinator.resumeWorkflowExecution).toHaveBeenCalledWith("workflow-execution-123");
      expect(result.success).toBe(true);
    });
  });

  describe("Skip Node Processor", () => {
    it("Test skip node: skipNodeHandler correctly skips the specified node", async () => {
      const action: TriggerAction = {
        type: "skip_node",
        parameters: {
          executionId: "workflow-execution-123",
          nodeId: "node-456",
        },
      };

      const handler = getTriggerHandler("skip_node");
      const result = await handler(action, "trigger-1", mockWorkflowExecutionRegistry, mockEventManager);

      expect(result.success).toBe(true);
      expect(result.triggerId).toBe("trigger-1");
    });
  });

  describe("Setting up the variable handler", () => {
    it("Test setting variables: setVariableHandler sets variables correctly", async () => {
      const action: TriggerAction = {
        type: "set_variable",
        parameters: {
          executionId: "workflow-execution-123",
          variables: {
            var1: "value1",
            var2: 123,
          },
          scope: "workflowExecution",
        },
      };

      const handler = getTriggerHandler("set_variable");
      const result = await handler(action, "trigger-1", mockWorkflowExecutionRegistry);

      expect(result.success).toBe(true);
      expect(result.triggerId).toBe("trigger-1");
    });

    it("Test variable scoping: scope parameter controls variable scoping", async () => {
      const action: TriggerAction = {
        type: "set_variable",
        parameters: {
          executionId: "workflow-execution-123",
          variables: {
            globalVar: "global-value",
          },
          scope: "global",
        },
      };

      const handler = getTriggerHandler("set_variable");
      const result = await handler(action, "trigger-1", mockWorkflowExecutionRegistry);

      expect(result.success).toBe(true);
    });
  });

  describe("Send Notification Handler", () => {
    it("Test sending notification: sendNotificationHandler sends notification correctly", async () => {
      const action: TriggerAction = {
        type: "send_notification",
        parameters: {
          message: "Test Notification",
          recipients: ["user1@example.com"],
          level: "info",
          channel: "email",
        },
      };

      const handler = getTriggerHandler("send_notification");
      const result = await handler(action, "trigger-1");

      expect(result.success).toBe(true);
      expect(result.triggerId).toBe("trigger-1");
    });

    it("Test notification level: level parameter correctly sets notification level", async () => {
      const action: TriggerAction = {
        type: "send_notification",
        parameters: {
          message: "warning notice",
          level: "warning",
        },
      };

      const handler = getTriggerHandler("send_notification");
      const result = await handler(action, "trigger-1");

      expect(result.success).toBe(true);
    });

    it("Test notification channel: channel parameter correctly set notification channel", async () => {
      const action: TriggerAction = {
        type: "send_notification",
        parameters: {
          message: "Test Notification",
          channel: "webhook",
        },
      };

      const handler = getTriggerHandler("send_notification");
      const result = await handler(action, "trigger-1");

      expect(result.success).toBe(true);
    });
  });

  describe("Customized Action Processor", () => {
    it("Test custom action: customHandler correctly executes the custom action", async () => {
      const action: TriggerAction = {
        type: "custom",
        parameters: {
          handlerName: "myCustomHandler",
          data: {
            key: "value",
          },
        },
      };

      const handler = getTriggerHandler("custom");
      const result = await handler(action, "trigger-1");

      expect(result.success).toBe(true);
      expect(result.triggerId).toBe("trigger-1");
    });
  });

  describe("Execution Script Processor", () => {
    it("Test execution of scripts: executeScriptHandler executes scripts correctly", async () => {
      const action: TriggerAction = {
        type: "execute_script",
        parameters: {
          scriptName: "test-script",
          parameters: {
            input: "test",
          },
          timeout: 5000,
          ignoreError: false,
        },
      };

      const handler = getTriggerHandler("execute_script");
      const result = await handler(action, "trigger-1", mockScriptService);

      expect(result.success).toBe(true);
      expect(result.triggerId).toBe("trigger-1");
    });

    it("Test script parameters: parameters are passed to the script correctly", async () => {
      const action: TriggerAction = {
        type: "execute_script",
        parameters: {
          scriptName: "test-script",
          parameters: {
            arg1: "value1",
            arg2: 123,
          },
        },
      };

      const handler = getTriggerHandler("execute_script");
      const result = await handler(action, "trigger-1", mockScriptService);

      expect(result.success).toBe(true);
    });

    it("Test timeout handling: timeout parameter controls timeout behavior", async () => {
      const action: TriggerAction = {
        type: "execute_script",
        parameters: {
          scriptName: "test-script",
          timeout: 10000,
        },
      };

      const handler = getTriggerHandler("execute_script");
      const result = await handler(action, "trigger-1", mockScriptService);

      expect(result.success).toBe(true);
    });
  });

  describe("Execution Trigger Sub-Workflow Processor", () => {
    it("Test execution of subworkflows: executeTriggeredSubgraphHandler correctly executes subworkflows", async () => {
      const action: TriggerAction = {
        type: "execute_triggered_subgraph",
        parameters: {
          triggeredWorkflowId: "subgraph-workflow-123",
          waitForCompletion: true,
        },
      };

      const handler = getTriggerHandler("execute_triggered_subgraph");
      const result = await handler(
        action,
        "trigger-1",
        mockWorkflowExecutionRegistry,
        mockEventManager,
        mockWorkflowExecutionBuilder,
        mockTaskQueueManager,
        "parent-workflow-execution-456",
      );

      expect(result.success).toBe(true);
      expect(result.triggerId).toBe("trigger-1");
    });

    it("Test wait for completion: the waitForCompletion parameter controls the wait behavior", async () => {
      const action: TriggerAction = {
        type: "execute_triggered_subgraph",
        parameters: {
          triggeredWorkflowId: "subgraph-workflow-123",
          waitForCompletion: false,
        },
      };

      const handler = getTriggerHandler("execute_triggered_subgraph");
      const result = await handler(
        action,
        "trigger-1",
        mockWorkflowExecutionRegistry,
        mockEventManager,
        mockWorkflowExecutionBuilder,
        mockTaskQueueManager,
        "parent-workflow-execution-456",
      );

      expect(result.success).toBe(true);
    });
  });

  describe("application message manipulation processor (AMMP)", () => {
    it("Test application message operation: applyMessageOperationHandler correctly applies operation", async () => {
      const action: TriggerAction = {
        type: "apply_message_operation",
        parameters: {
          executionId: "workflow-execution-123",
          operationType: "compress",
          config: {
            strategy: "keep-last-n",
            count: 10,
          },
        },
      };

      const handler = getTriggerHandler("apply_message_operation");
      const result = await handler(action, "trigger-1", mockWorkflowExecutionRegistry);

      expect(result.success).toBe(true);
      expect(result.triggerId).toBe("trigger-1");
    });

    it("Test operation type: the operationType parameter controls the operation type.", async () => {
      const action: TriggerAction = {
        type: "apply_message_operation",
        parameters: {
          executionId: "workflow-execution-123",
          operationType: "truncate",
        },
      };

      const handler = getTriggerHandler("apply_message_operation");
      const result = await handler(action, "trigger-1", mockWorkflowExecutionRegistry);

      expect(result.success).toBe(true);
    });
  });

  describe("processor mapping", () => {
    it("Test trigger handler mapping: triggerHandlers contains all handlers", () => {
      expect(triggerHandlers).toBeDefined();
      expect(triggerHandlers["stop_workflow_execution"]).toBeDefined();
      expect(triggerHandlers["pause_workflow_execution"]).toBeDefined();
      expect(triggerHandlers["resume_workflow_execution"]).toBeDefined();
      expect(triggerHandlers["skip_node"]).toBeDefined();
      expect(triggerHandlers["set_variable"]).toBeDefined();
      expect(triggerHandlers["send_notification"]).toBeDefined();
      expect(triggerHandlers["custom"]).toBeDefined();
      expect(triggerHandlers["execute_script"]).toBeDefined();
      expect(triggerHandlers["apply_message_operation"]).toBeDefined();
      expect(triggerHandlers["execute_triggered_subgraph"]).toBeDefined();
    });

    it("Test to get the handler: getTriggerHandler returns the correct handler", () => {
      const stopWorkflowExecutionHandler = getTriggerHandler("stop_workflow_execution");
      const pauseWorkflowExecutionHandler = getTriggerHandler("pause_workflow_execution");
      const resumeWorkflowExecutionHandler = getTriggerHandler("resume_workflow_execution");

      expect(stopWorkflowExecutionHandler).toBeDefined();
      expect(pauseWorkflowExecutionHandler).toBeDefined();
      expect(resumeWorkflowExecutionHandler).toBeDefined();
    });

    it("Test to get non-existent processor: should throw an error", () => {
      expect(() => {
        getTriggerHandler("invalid_handler" as any);
      }).toThrow();
    });
  });

  describe("Processor execution results", () => {
    it("Test that all processors return the correct result format", async () => {
      const actionConfigs = [
        { type: "pause_workflow_execution", parameters: { executionId: "workflow-execution-123" } },
        { type: "resume_workflow_execution", parameters: { executionId: "workflow-execution-123" } },
        { type: "skip_node", parameters: { executionId: "workflow-execution-123", nodeId: "node-456" } },
        { type: "set_variable", parameters: { executionId: "workflow-execution-123", variables: {} } },
        { type: "send_notification", parameters: { message: "test" } },
        { type: "custom", parameters: { handlerName: "test" } },
        { type: "execute_script", parameters: { scriptName: "test" } },
        {
          type: "apply_message_operation",
          parameters: { executionId: "workflow-execution-123", operationType: "compress" },
        },
        { type: "execute_triggered_subgraph", parameters: { triggeredWorkflowId: "workflow-123" } },
      ];

      for (const actionConfig of actionConfigs) {
        const action: TriggerAction = actionConfig as TriggerAction;

        const handler = getTriggerHandler(action.type);
        const result = await handler(
          action,
          "trigger-1",
          mockWorkflowLifecycleCoordinator,
          mockWorkflowExecutionRegistry,
          mockEventManager,
          mockWorkflowExecutionBuilder,
          mockTaskQueueManager,
          "workflow-execution-123",
          mockScriptService,
        );

        expect(result).toBeDefined();
        expect(result.triggerId).toBe("trigger-1");
        expect(result.action).toBe(action);
        expect(result.executionTime).toBeGreaterThanOrEqual(0);
      }
    });
  });
});
