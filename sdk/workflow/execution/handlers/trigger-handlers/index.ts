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

const triggerHandlersMap = {
  apply_message_operation: applyMessageOperationHandler,
  execute_script: executeScriptHandler,
  execute_triggered_subworkflow: executeTriggeredSubworkflowHandler,
  pause_workflow_execution: pauseExecutionHandler,
  resume_workflow_execution: resumeExecutionHandler,
  send_notification: sendNotificationHandler,
  set_variable: setVariableHandler,
  skip_node: skipNodeHandler,
  stop_workflow_execution: stopExecutionHandler,
} as const;

export const triggerHandlers: Record<string, TriggerHandlerFn> =
  triggerHandlersMap as unknown as Record<string, TriggerHandlerFn>;

export function getTriggerHandler(actionType: string): TriggerHandlerFn {
  const handler = triggerHandlersMap[actionType as keyof typeof triggerHandlersMap];
  if (!handler) {
    throw new Error(`Unknown trigger action type: ${actionType}`);
  }
  return handler as unknown as TriggerHandlerFn;
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
