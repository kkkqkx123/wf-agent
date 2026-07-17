import type { TriggerAction, TriggerExecutionResult } from "@wf-agent/types";
import { applyMessageOperationHandler } from "./apply-message-operation-handler.js";
import { executeScriptHandler } from "./execute-script-handler.js";
import { executeTriggeredSubworkflowHandler } from "./execute-triggered-subworkflow-handler.js";
import { pauseExecutionHandler } from "./pause-execution-handler.js";
import { resumeExecutionHandler } from "./resume-execution-handler.js";
import { sendNotificationHandler } from "./send-notification-handler.js";
import { setVariableHandler } from "./set-variable-handler.js";
import { skipNodeHandler } from "./skip-node-handler.js";
import { stopExecutionHandler } from "./stop-execution-handler.js";

export type TriggerHandlerFn = (
  action: TriggerAction,
  triggerId: string,
  ...dependencies: unknown[]
) => Promise<TriggerExecutionResult>;

// Explicitly typed map avoids `as const` + `as unknown as` casts
const triggerHandlersMap: Record<string, TriggerHandlerFn> = {
  apply_message_operation: applyMessageOperationHandler as TriggerHandlerFn,
  execute_script: executeScriptHandler as TriggerHandlerFn,
  execute_triggered_subworkflow: executeTriggeredSubworkflowHandler as TriggerHandlerFn,
  pause_workflow_execution: pauseExecutionHandler as TriggerHandlerFn,
  resume_workflow_execution: resumeExecutionHandler as TriggerHandlerFn,
  send_notification: sendNotificationHandler as TriggerHandlerFn,
  set_variable: setVariableHandler as TriggerHandlerFn,
  skip_node: skipNodeHandler as TriggerHandlerFn,
  stop_workflow_execution: stopExecutionHandler as TriggerHandlerFn,
};

export const triggerHandlers: Record<string, TriggerHandlerFn> = triggerHandlersMap;

export function getTriggerHandler(actionType: string): TriggerHandlerFn {
  const handler = triggerHandlersMap[actionType];
  if (!handler) {
    throw new Error(`Unknown trigger action type: ${actionType}`);
  }
  return handler;
}

export {
  applyMessageOperationHandler,
  executeScriptHandler,
  executeTriggeredSubworkflowHandler,
  pauseExecutionHandler,
  resumeExecutionHandler,
  sendNotificationHandler,
  setVariableHandler,
  skipNodeHandler,
  stopExecutionHandler,
};
