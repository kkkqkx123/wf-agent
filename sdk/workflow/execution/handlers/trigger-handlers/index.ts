/**
 * Trigger handlers export
 */

import type { TriggerAction, TriggerExecutionResult } from "@wf-agent/types";
import { applyMessageOperationHandler } from "./apply-message-operation-handler.js";
import { customHandler } from "./custom-handler.js";
import { executeScriptHandler } from "./execute-script-handler.js";
import { executeTriggeredSubgraphHandler } from "./execute-triggered-subgraph-handler.js";
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

export function getTriggerHandler(actionType: string): TriggerHandlerFn {
  const handlers: Record<string, TriggerHandlerFn> = {
    apply_message_operation: applyMessageOperationHandler as unknown as TriggerHandlerFn,
    custom: customHandler as unknown as TriggerHandlerFn,
    execute_script: executeScriptHandler as unknown as TriggerHandlerFn,
    execute_triggered_subgraph: executeTriggeredSubgraphHandler as unknown as TriggerHandlerFn,
    pause_workflow_execution: pauseExecutionHandler as unknown as TriggerHandlerFn,
    resume_workflow_execution: resumeExecutionHandler as unknown as TriggerHandlerFn,
    send_notification: sendNotificationHandler as unknown as TriggerHandlerFn,
    set_variable: setVariableHandler as unknown as TriggerHandlerFn,
    skip_node: skipNodeHandler as unknown as TriggerHandlerFn,
    stop_workflow_execution: stopExecutionHandler as unknown as TriggerHandlerFn,
  };

  const handler = handlers[actionType];
  if (!handler) {
    throw new Error(`Unknown trigger action type: ${actionType}`);
  }
  return handler;
}

export {
  applyMessageOperationHandler,
  customHandler,
  executeScriptHandler,
  executeTriggeredSubgraphHandler,
  pauseExecutionHandler,
  resumeExecutionHandler,
  sendNotificationHandler,
  setVariableHandler,
  skipNodeHandler,
  stopExecutionHandler,
};
