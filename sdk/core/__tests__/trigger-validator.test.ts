/**
 * Trigger Validator Integration Tests
 *
 * Test Scenarios:
 * - Trigger condition validation
 * - Trigger action validation
 * - Workflow trigger validation
 * - Trigger reference validation
 */

import { describe, it, expect } from "vitest";
import {
  validateTriggerCondition,
  validateTriggerAction,
  validateWorkflowTrigger,
  validateTriggerReference,
  validateTriggers,
  validateExecuteTriggeredSubgraphActionConfig,
  validateExecuteScriptActionConfig,
} from "../validation/trigger-validator.js";
import type {
  WorkflowTrigger,
  TriggerCondition,
  TriggerAction,
  TriggerReference,
} from "@wf-agent/types";
import { EventType, TriggerActionType } from "@wf-agent/types";

describe("Trigger Validator - Trigger Validator", () => {
  describe("Trigger condition validation", () => {
    it("Test for valid trigger conditions: a valid eventType should pass validation", () => {
      const condition: TriggerCondition = {
        eventType: "WORKFLOW_EXECUTION_STARTED" as EventType,
      };

      const result = validateTriggerCondition(condition);

      expect(result.isOk()).toBe(true);
      if (result.isOk()) {
        expect(result.value.eventType).toBe("WORKFLOW_EXECUTION_STARTED");
      }
    });

    it("Testing for invalid event types: invalid eventType should fail validation", () => {
      const condition: any = {
        eventType: "INVALID_EVENT_TYPE",
      };

      const result = validateTriggerCondition(condition);

      expect(result.isErr()).toBe(true);
    });

    it("Test NODE_CUSTOM_EVENT must be eventName: eventName is required if eventType is NODE_CUSTOM_EVENT.", () => {
      const condition: any = {
        eventType: "NODE_CUSTOM_EVENT",
        // "The eventName has not been set."
      };

      const result = validateTriggerCondition(condition);

      expect(result.isErr()).toBe(true);
    });

    it("Test NODE_CUSTOM_EVENT with eventName: eventType of NODE_CUSTOM_EVENT with eventName set should pass validation", () => {
      const condition: TriggerCondition = {
        eventType: "NODE_CUSTOM_EVENT" as EventType,
        eventName: "custom-event-name",
      };

      const result = validateTriggerCondition(condition);

      expect(result.isOk()).toBe(true);
    });

    it("Test optional eventName: eventName is optional for other eventTypes.", () => {
      const condition1: TriggerCondition = {
        eventType: "WORKFLOW_EXECUTION_STARTED" as EventType,
      };

      const condition2: TriggerCondition = {
        eventType: "WORKFLOW_EXECUTION_STARTED" as EventType,
        eventName: "optional-event-name",
      };

      const result1 = validateTriggerCondition(condition1);
      const result2 = validateTriggerCondition(condition2);

      expect(result1.isOk()).toBe(true);
      expect(result2.isOk()).toBe(true);
    });

    it("Test trigger condition metadata: metadata should be an optional object", () => {
      const condition: TriggerCondition = {
        eventType: "WORKFLOW_EXECUTION_STARTED" as EventType,
        metadata: {
          key1: "value1",
          key2: 123,
        },
      };

      const result = validateTriggerCondition(condition);

      expect(result.isOk()).toBe(true);
    });
  });

  describe("Trigger Action Verification", () => {
    it("Test for valid triggered actions: valid actions should pass validation", () => {
      const action: TriggerAction = {
        type: "pause_workflow_execution",
        parameters: {
          executionId: "workflow-execution-123",
          force: false,
        },
      };

      const result = validateTriggerAction(action);

      expect(result.isOk()).toBe(true);
    });

    it("Testing for invalid action types: invalid action.type should fail validation", () => {
      const action: any = {
        type: "invalid_action_type",
        parameters: {},
      };

      const result = validateTriggerAction(action);

      expect(result.isErr()).toBe(true);
    });

    it("Test action parameter validation: the pause_workflow_execution action parameter should be correctly validated", () => {
      const action1: TriggerAction = {
        type: "pause_workflow_execution",
        parameters: {
          executionId: "workflow-execution-123",
          force: false,
        },
      };

      const action2: any = {
        type: "pause_workflow_execution",
        parameters: {
          force: false,
          // `executionId` is missing.
        },
      };

      const result1 = validateTriggerAction(action1);
      const result2 = validateTriggerAction(action2);

      expect(result1.isOk()).toBe(true);
      expect(result2.isErr()).toBe(true);
    });

    it("Test action parameter validation: the pause_workflow_execution action parameter should be correctly validated", () => {
      const action1: TriggerAction = {
        type: "pause_workflow_execution",
        parameters: {
          executionId: "workflow-execution-123",
          reason: "Test Suspension",
        },
      };

      const action2: TriggerAction = {
        type: "pause_workflow_execution",
        parameters: {
          executionId: "workflow-execution-123",
          // "reason" is optional.
        },
      };

      const result1 = validateTriggerAction(action1);
      const result2 = validateTriggerAction(action2);

      expect(result1.isOk()).toBe(true);
      expect(result2.isOk()).toBe(true);
    });

    it("Test action parameter validation: the set_variable action parameter should be correctly validated", () => {
      const action1: TriggerAction = {
        type: "set_variable",
        parameters: {
          executionId: "workflow-execution-123",
          variables: {
            var1: "value1",
            var2: 123,
          },
        },
      };

      const action2: any = {
        type: "set_variable",
        parameters: {
          executionId: "workflow-execution-123",
          variables: {},
          // Variables cannot be empty.
        },
      };

      const result1 = validateTriggerAction(action1);
      const result2 = validateTriggerAction(action2);

      expect(result1.isOk()).toBe(true);
      expect(result2.isErr()).toBe(true);
    });

    it("Test action parameter validation: the execute_triggered_subgraph action parameter should validate correctly", () => {
      const action1: TriggerAction = {
        type: "execute_triggered_subgraph",
        parameters: {
          triggeredWorkflowId: "workflow-123",
          waitForCompletion: true,
        },
      };

      const action2: any = {
        type: "execute_triggered_subgraph",
        parameters: {
          waitForCompletion: true,
          // `triggeredWorkflowId` is missing.
        },
      };

      const result1 = validateTriggerAction(action1);
      const result2 = validateTriggerAction(action2);

      expect(result1.isOk()).toBe(true);
      expect(result2.isErr()).toBe(true);
    });

    it("Test action parameter validation: execute_script action parameter should be validated correctly", () => {
      const action1: TriggerAction = {
        type: "execute_script",
        parameters: {
          scriptName: "test-script",
          parameters: { key: "value" },
          timeout: 5000,
        },
      };

      const action2: any = {
        type: "execute_script",
        parameters: {
          parameters: { key: "value" },
          // `scriptName` is missing.
        },
      };

      const result1 = validateTriggerAction(action1);
      const result2 = validateTriggerAction(action2);

      expect(result1.isOk()).toBe(true);
      expect(result2.isErr()).toBe(true);
    });

    it("Test action metadata: metadata should be an optional object", () => {
      const action: TriggerAction = {
        type: "pause_workflow_execution",
        parameters: {
          executionId: "workflow-execution-123",
        },
        metadata: {
          description: "Test Motion",
        },
      };

      const result = validateTriggerAction(action);

      expect(result.isOk()).toBe(true);
    });
  });

  describe("Workflow Trigger Validation", () => {
    it("Test complete trigger configuration: all required fields should pass validation", () => {
      const trigger: WorkflowTrigger = {
        id: "trigger-123",
        name: "Test Trigger",
        description: "This is a test trigger",
        condition: {
          eventType: "WORKFLOW_EXECUTION_STARTED" as EventType,
        },
        action: {
          type: "pause_workflow_execution",
          parameters: {
            executionId: "workflow-execution-123",
          },
        },
        enabled: true,
        maxTriggers: 5,
      };

      const result = validateWorkflowTrigger(trigger);

      expect(result.isOk()).toBe(true);
    });

    it("Test for missing required fields: missing id should fail validation", () => {
      const trigger: any = {
        name: "Test Trigger",
        condition: {
          eventType: "WORKFLOW_EXECUTION_STARTED" as EventType,
        },
        action: {
          type: "pause_workflow_execution",
          parameters: {
            executionId: "workflow-execution-123",
          },
        },
        // 'id is missing'
      };

      const result = validateWorkflowTrigger(trigger);

      expect(result.isErr()).toBe(true);
    });

    it("Test for missing required fields: missing name should fail validation", () => {
      const trigger: any = {
        id: "trigger-123",
        condition: {
          eventType: "WORKFLOW_EXECUTION_STARTED" as EventType,
        },
        action: {
          type: "pause_workflow_execution",
          parameters: {
            executionId: "workflow-execution-123",
          },
        },
        // `name` is missing.
      };

      const result = validateWorkflowTrigger(trigger);

      expect(result.isErr()).toBe(true);
    });

    it("Test for missing required fields: missing condition should fail validation", () => {
      const trigger: any = {
        id: "trigger-123",
        name: "Test Trigger",
        action: {
          type: "pause_workflow_execution",
          parameters: {
            executionId: "workflow-execution-123",
          },
        },
        // "condition is missing"
      };

      const result = validateWorkflowTrigger(trigger);

      expect(result.isErr()).toBe(true);
    });

    it("Test for missing required fields: missing action should fail validation", () => {
      const trigger: any = {
        id: "trigger-123",
        name: "Test Trigger",
        condition: {
          eventType: "WORKFLOW_EXECUTION_STARTED" as EventType,
        },
        // "action missing"
      };

      const result = validateWorkflowTrigger(trigger);

      expect(result.isErr()).toBe(true);
    });

    it("Test optional fields: enabled, maxTriggers, metadata and other optional fields", () => {
      const trigger: WorkflowTrigger = {
        id: "trigger-123",
        name: "Test Trigger",
        condition: {
          eventType: "WORKFLOW_EXECUTION_STARTED" as EventType,
        },
        action: {
          type: "pause_workflow_execution",
          parameters: {
            executionId: "workflow-execution-123",
          },
        },
        // enabled、maxTriggers、description、metadata 未设置
      };

      const result = validateWorkflowTrigger(trigger);

      expect(result.isOk()).toBe(true);
    });

    it("Test maxTriggers is negative: should fail validation", () => {
      const trigger: any = {
        id: "trigger-123",
        name: "Test Trigger",
        condition: {
          eventType: "WORKFLOW_EXECUTION_STARTED" as EventType,
        },
        action: {
          type: "pause_workflow_execution",
          parameters: {
            executionId: "workflow-execution-123",
          },
        },
        maxTriggers: -1,
      };

      const result = validateWorkflowTrigger(trigger);

      expect(result.isErr()).toBe(true);
    });

    it("Test maxTriggers to 0: should pass validation (indicates no limit)", () => {
      const trigger: WorkflowTrigger = {
        id: "trigger-123",
        name: "Test Trigger",
        condition: {
          eventType: "WORKFLOW_EXECUTION_STARTED" as EventType,
        },
        action: {
          type: "pause_workflow_execution",
          parameters: {
            executionId: "workflow-execution-123",
          },
        },
        maxTriggers: 0,
      };

      const result = validateWorkflowTrigger(trigger);

      expect(result.isOk()).toBe(true);
    });

    it("Test nested validation: conditions and actions embedded in triggers should also be validated", () => {
      const trigger: any = {
        id: "trigger-123",
        name: "Test Trigger",
        condition: {
          eventType: "INVALID_EVENT_TYPE",
        },
        action: {
          type: "pause_workflow_execution",
          parameters: {
            executionId: "workflow-execution-123",
          },
        },
      };

      const result = validateWorkflowTrigger(trigger);

      expect(result.isErr()).toBe(true);
    });
  });

  describe("Trigger array validation", () => {
    it("Test for ID uniqueness: IDs should be unique in the trigger array", () => {
      const triggers: WorkflowTrigger[] = [
        {
          id: "trigger-1",
          name: "Trigger 1",
          condition: { eventType: "WORKFLOW_EXECUTION_STARTED" as EventType },
          action: { type: "pause_workflow_execution", parameters: { executionId: "workflow-execution-123" } },
        },
        {
          id: "trigger-2",
          name: "Trigger 2",
          condition: { eventType: "WORKFLOW_EXECUTION_COMPLETED" as EventType },
          action: { type: "pause_workflow_execution", parameters: { executionId: "workflow-execution-456" } },
        },
      ];

      const result = validateTriggers(triggers);

      expect(result.isOk()).toBe(true);
    });

    it("Test ID duplicate: ID duplicates in the trigger array should fail validation", () => {
      const triggers: WorkflowTrigger[] = [
        {
          id: "trigger-1",
          name: "Trigger 1",
          condition: { eventType: "WORKFLOW_EXECUTION_STARTED" as EventType },
          action: { type: "pause_workflow_execution", parameters: { executionId: "workflow-execution-123" } },
        },
        {
          id: "trigger-1",
          name: "Trigger 2",
          condition: { eventType: "WORKFLOW_EXECUTION_COMPLETED" as EventType },
          action: { type: "pause_workflow_execution", parameters: { executionId: "workflow-execution-456" } },
        },
      ];

      const result = validateTriggers(triggers);

      expect(result.isErr()).toBe(true);
    });

    it("Testing empty trigger arrays: empty arrays should pass validation", () => {
      const triggers: WorkflowTrigger[] = [];

      const result = validateTriggers(triggers);

      expect(result.isOk()).toBe(true);
    });

    it("Testing non-array inputs: non-array should fail validation", () => {
      const triggers: any = null;

      const result = validateTriggers(triggers);

      expect(result.isErr()).toBe(true);
    });

    it("Test hybrid trigger types: WorkflowTrigger and TriggerReference are supported.", () => {
      const workflowTrigger: WorkflowTrigger = {
        id: "trigger-1",
        name: "Trigger 1",
        condition: { eventType: "THREAD_STARTED" as EventType },
        action: { type: "pause_workflow_execution", parameters: { executionId: "workflow-execution-123" } },
      };

      const triggerReference: TriggerReference = {
        templateName: "context-compression",
        triggerId: "trigger-2",
      };

      const result = validateTriggers([workflowTrigger, triggerReference]);

      expect(result.isOk()).toBe(true);
    });

    it("Test for multiple errors: all validation errors should be returned", () => {
      const triggers: any[] = [
        {
          id: "trigger-1",
          name: "Trigger 1",
          condition: { eventType: "INVALID_EVENT_TYPE" },
          action: { type: "pause_workflow_execution", parameters: { executionId: "workflow-execution-123" } },
        },
        {
          id: "trigger-1",
          name: "Trigger 2",
          condition: { eventType: "WORKFLOW_EXECUTION_COMPLETED" as EventType },
          action: { type: "pause_workflow_execution", parameters: { executionId: "workflow-execution-456" } },
        },
      ];

      const result = validateTriggers(triggers);

      expect(result.isErr()).toBe(true);
      if (result.isErr()) {
        expect(result.error.length).toBeGreaterThan(1);
      }
    });
  });

  describe("Trigger Reference Validation", () => {
    it("Test for valid trigger references: valid templateName and triggerId should pass validation", () => {
      const reference: TriggerReference = {
        templateName: "context-compression",
        triggerId: "trigger-123",
      };

      const result = validateTriggerReference(reference);

      expect(result.isOk()).toBe(true);
    });

    it("Test missing templateName: missing templateName should fail validation", () => {
      const reference: any = {
        triggerId: "trigger-123",
        // `templateName` is missing.
      };

      const result = validateTriggerReference(reference);

      expect(result.isErr()).toBe(true);
    });

    it("Test missing triggerId: missing triggerId should fail validation", () => {
      const reference: any = {
        templateName: "context-compression",
        // `triggerId` is missing.
      };

      const result = validateTriggerReference(reference);

      expect(result.isErr()).toBe(true);
    });

    it("Test configuration override: configOverride should validate correctly", () => {
      const reference: TriggerReference = {
        templateName: "context-compression",
        triggerId: "trigger-123",
        triggerName: "Custom Trigger Names",
        configOverride: {
          condition: {
            eventType: "WORKFLOW_EXECUTION_STARTED" as EventType,
          },
          action: {
            type: "pause_workflow_execution",
          },
          enabled: true,
          maxTriggers: 10,
        },
      };

      const result = validateTriggerReference(reference);

      expect(result.isOk()).toBe(true);
    });

    it("Test triggerName optional: triggerName is an optional field", () => {
      const reference1: TriggerReference = {
        templateName: "context-compression",
        triggerId: "trigger-123",
      };

      const reference2: TriggerReference = {
        templateName: "context-compression",
        triggerId: "trigger-123",
        triggerName: "Custom Trigger Names",
      };

      const result1 = validateTriggerReference(reference1);
      const result2 = validateTriggerReference(reference2);

      expect(result1.isOk()).toBe(true);
      expect(result2.isOk()).toBe(true);
    });

    it("Test configOverride optional: configOverride is optional field", () => {
      const reference: TriggerReference = {
        templateName: "context-compression",
        triggerId: "trigger-123",
        // `configOverride` is not set.
      };

      const result = validateTriggerReference(reference);

      expect(result.isOk()).toBe(true);
    });
  });

  describe("Action-specific configuration validation", () => {
    it("Test to validate the configuration of the triggering sub-workflow action: valid configurations should pass validation", () => {
      const config = {
        triggeredWorkflowId: "workflow-123",
        waitForCompletion: true,
        mergeOptions: {
          includeVariables: ["var1", "var2"],
          includeConversationHistory: {
            lastN: 10,
          },
        },
      };

      const result = validateExecuteTriggeredSubgraphActionConfig(config);

      expect(result.isOk()).toBe(true);
    });

    it("Test Validation Triggered Sub-Workflow Action Configuration: missing triggeredWorkflowId should fail validation", () => {
      const config: any = {
        waitForCompletion: true,
        // `triggeredWorkflowId` is missing.
      };

      const result = validateExecuteTriggeredSubgraphActionConfig(config);

      expect(result.isErr()).toBe(true);
    });

    it("Tests validate the execution of scripted action configurations: valid configurations should pass validation", () => {
      const config = {
        scriptName: "test-script",
        parameters: { key: "value" },
        timeout: 5000,
        ignoreError: false,
        validateExistence: true,
      };

      const result = validateExecuteScriptActionConfig(config);

      expect(result.isOk()).toBe(true);
    });

    it("Test validation execution script action configuration: missing scriptName should fail validation", () => {
      const config: any = {
        parameters: { key: "value" },
        timeout: 5000,
        // `scriptName` is missing.
      };

      const result = validateExecuteScriptActionConfig(config);

      expect(result.isErr()).toBe(true);
    });

    it("Test validation execution script action configuration: timeout must be a positive integer", () => {
      const config1: any = {
        scriptName: "test-script",
        timeout: 0,
      };

      const config2: any = {
        scriptName: "test-script",
        timeout: -100,
      };

      const result1 = validateExecuteScriptActionConfig(config1);
      const result2 = validateExecuteScriptActionConfig(config2);

      expect(result1.isErr()).toBe(true);
      expect(result2.isErr()).toBe(true);
    });

    it("Test validation execution script action configuration: all fields optional", () => {
      const config = {
        scriptName: "test-script",
        // Other parameters are not set.
      };

      const result = validateExecuteScriptActionConfig(config);

      expect(result.isOk()).toBe(true);
    });
  });
});
