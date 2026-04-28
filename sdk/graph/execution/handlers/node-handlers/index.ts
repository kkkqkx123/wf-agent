/**
 * Node handlers export
 */

import type { Node } from "@wf-agent/types";
import type { ThreadEntity } from "../../../entities/thread-entity.js";
import { addToolHandler, type AddToolHandlerContext } from "./add-tool-handler.js";
import { agentLoopHandler } from "./agent-loop-handler.js";
import { contextProcessorHandler } from "./context-processor-handler.js";
import { continueFromTriggerHandler } from "./continue-from-trigger-handler.js";
import { endHandler } from "./end-handler.js";
import { forkHandler } from "./fork-handler.js";
import { joinHandler } from "./join-handler.js";
import { llmHandler } from "./llm-handler.js";
import { loopEndHandler } from "./loop-end-handler.js";
import { loopStartHandler } from "./loop-start-handler.js";
import { routeHandler } from "./route-handler.js";
import { scriptHandler } from "./script-handler.js";
import { startFromTriggerHandler } from "./start-from-trigger-handler.js";
import { startHandler } from "./start-handler.js";
import { userInteractionHandler } from "./user-interaction-handler.js";
import { variableHandler } from "./variable-handler.js";

export type NodeHandlerFn = (
  threadEntity: ThreadEntity,
  node: Node,
  context?: unknown,
) => Promise<unknown>;

export function getNodeHandler(nodeType: string): NodeHandlerFn {
  const handlers: Record<string, NodeHandlerFn> = {
    ADD_TOOL: (threadEntity, node, context) =>
      addToolHandler(threadEntity.getThread(), node, context as AddToolHandlerContext),
    AGENT_LOOP: (threadEntity, node, context) =>
      agentLoopHandler(threadEntity.getThread(), node, context as any),
    CONTEXT_PROCESSOR: (threadEntity, node, context) =>
      contextProcessorHandler(threadEntity.getThread(), node, context as any),
    CONTINUE_FROM_TRIGGER: continueFromTriggerHandler as NodeHandlerFn,
    END: endHandler as NodeHandlerFn,
    FORK: forkHandler as NodeHandlerFn,
    JOIN: joinHandler as NodeHandlerFn,
    LLM: (threadEntity, node, context) =>
      llmHandler(threadEntity.getThread(), node, context as any),
    LOOP_END: loopEndHandler as NodeHandlerFn,
    LOOP_START: loopStartHandler as NodeHandlerFn,
    ROUTE: routeHandler as NodeHandlerFn,
    SCRIPT: scriptHandler as NodeHandlerFn,
    START_FROM_TRIGGER: startFromTriggerHandler as NodeHandlerFn,
    START: startHandler as NodeHandlerFn,
    USER_INTERACTION: (threadEntity, node, context) =>
      userInteractionHandler(threadEntity.getThread(), node, context as any),
    VARIABLE: variableHandler as NodeHandlerFn,
  };

  const handler = handlers[nodeType];
  if (!handler) {
    throw new Error(`Unknown node type: ${nodeType}`);
  }
  return handler;
}

export {
  addToolHandler,
  agentLoopHandler,
  contextProcessorHandler,
  continueFromTriggerHandler,
  endHandler,
  forkHandler,
  joinHandler,
  llmHandler,
  loopEndHandler,
  loopStartHandler,
  routeHandler,
  scriptHandler,
  startFromTriggerHandler,
  startHandler,
  userInteractionHandler,
  variableHandler,
};

export type { AddToolHandlerContext };
