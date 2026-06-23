import { describe, it, expect } from "vitest";
import { getTriggerHandler, triggerHandlers } from "../index.js";
import type { TriggerHandlerFn } from "../index.js";
import { applyMessageOperationHandler } from "../apply-message-operation-handler.js";
import { executeScriptHandler } from "../execute-script-handler.js";
import { executeTriggeredSubworkflowHandler } from "../execute-triggered-subworkflow-handler.js";
import { pauseExecutionHandler } from "../pause-execution-handler.js";
import { resumeExecutionHandler } from "../resume-execution-handler.js";
import { sendNotificationHandler } from "../send-notification-handler.js";
import { setVariableHandler } from "../set-variable-handler.js";
import { skipNodeHandler } from "../skip-node-handler.js";
import { stopExecutionHandler } from "../stop-execution-handler.js";

describe("trigger-handlers index", () => {
  it("should export all handlers", () => {
    expect(applyMessageOperationHandler).toBeDefined();
    expect(executeScriptHandler).toBeDefined();
    expect(executeTriggeredSubworkflowHandler).toBeDefined();
    expect(pauseExecutionHandler).toBeDefined();
    expect(resumeExecutionHandler).toBeDefined();
    expect(sendNotificationHandler).toBeDefined();
    expect(setVariableHandler).toBeDefined();
    expect(skipNodeHandler).toBeDefined();
    expect(stopExecutionHandler).toBeDefined();
  });

  it("should have triggerHandlers map with all action types", () => {
    expect(Object.keys(triggerHandlers)).toHaveLength(9);
    expect(triggerHandlers["apply_message_operation"]).toBe(applyMessageOperationHandler);
    expect(triggerHandlers["execute_script"]).toBe(executeScriptHandler);
    expect(triggerHandlers["execute_triggered_subworkflow"]).toBe(
      executeTriggeredSubworkflowHandler,
    );
    expect(triggerHandlers["pause_workflow_execution"]).toBe(pauseExecutionHandler);
    expect(triggerHandlers["resume_workflow_execution"]).toBe(resumeExecutionHandler);
    expect(triggerHandlers["send_notification"]).toBe(sendNotificationHandler);
    expect(triggerHandlers["set_variable"]).toBe(setVariableHandler);
    expect(triggerHandlers["skip_node"]).toBe(skipNodeHandler);
    expect(triggerHandlers["stop_workflow_execution"]).toBe(stopExecutionHandler);
  });

  describe("getTriggerHandler", () => {
    it("should return correct handler for valid action type", () => {
      const handler1 = getTriggerHandler("pause_workflow_execution");
      expect(handler1).toBe(pauseExecutionHandler);

      const handler2 = getTriggerHandler("send_notification");
      expect(handler2).toBe(sendNotificationHandler);
    });

    it("should throw error for unknown action type", () => {
      expect(() => getTriggerHandler("unknown_action")).toThrow("Unknown trigger action type");
    });

    it("should handle all registered action types", () => {
      const actionTypes = [
        "apply_message_operation",
        "execute_script",
        "execute_triggered_subworkflow",
        "pause_workflow_execution",
        "resume_workflow_execution",
        "send_notification",
        "set_variable",
        "skip_node",
        "stop_workflow_execution",
      ];

      for (const actionType of actionTypes) {
        expect(() => getTriggerHandler(actionType)).not.toThrow();
        const handler = getTriggerHandler(actionType);
        expect(typeof handler).toBe("function");
      }
    });
  });

  it("should have correct function signature for all handlers", () => {
    const sendNotificationFn = triggerHandlers["send_notification"] as TriggerHandlerFn;
    const pauseFn = triggerHandlers["pause_workflow_execution"] as TriggerHandlerFn;

    // Type check - functions should accept these parameters
    expect(typeof sendNotificationFn).toBe("function");
    expect(typeof pauseFn).toBe("function");

    // Handler should accept action, triggerId, and dependencies
    const handler = getTriggerHandler("send_notification");
    expect(handler).toBeInstanceOf(Function);
  });
});
