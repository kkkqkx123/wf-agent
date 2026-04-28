/**
 * Trigger handlers export
 */

import type { TriggerAction, TriggerExecutionResult } from "@wf-agent/types";
import { applyMessageOperationHandler } from "./apply-message-operation-handler.js";
import { customHandler } from "./custom-handler.js";
import { executeScriptHandler } from "./execute-script-handler.js";
import { executeTriggeredSubgraphHandler } from "./execute-triggered-subgraph-handler.js";
import { pauseThreadHandler } from "./pause-thread-handler.js";
import { resumeThreadHandler } from "./resume-thread-handler.js";
import { sendNotificationHandler } from "./send-notification-handler.js";
import { setVariableHandler } from "./set-variable-handler.js";
import { skipNodeHandler } from "./skip-node-handler.js";
import { stopThreadHandler } from "./stop-thread-handler.js";

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
    pause_thread: pauseThreadHandler as unknown as TriggerHandlerFn,
    resume_thread: resumeThreadHandler as unknown as TriggerHandlerFn,
    send_notification: sendNotificationHandler as unknown as TriggerHandlerFn,
    set_variable: setVariableHandler as unknown as TriggerHandlerFn,
    skip_node: skipNodeHandler as unknown as TriggerHandlerFn,
    stop_thread: stopThreadHandler as unknown as TriggerHandlerFn,
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
  pauseThreadHandler,
  resumeThreadHandler,
  sendNotificationHandler,
  setVariableHandler,
  skipNodeHandler,
  stopThreadHandler,
};
