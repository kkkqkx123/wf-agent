/**
 * Node handlers export
 */

import type { RuntimeNode } from "@wf-agent/types";
import type { WorkflowExecutionEntity } from "../../../entities/workflow-execution-entity.js";
import type { GlobalContext } from "../../../../core/global-context.js";
import { addToolHandler, type AddToolHandlerContext } from "./add-tool-handler.js";
import { agentLoopHandler, type AgentLoopHandlerContext } from "./agent-loop-handler.js";
import { contextProcessorHandler, type ContextProcessorHandlerContext } from "./context-processor-handler.js";
import { continueFromTriggerHandler } from "./continue-from-trigger-handler.js";
import { endHandler } from "./end-handler.js";
import { forkHandler } from "./fork-handler.js";
import { joinHandler } from "./join-handler.js";
import { llmHandler, type LLMHandlerContext } from "./llm-handler.js";
import { loopEndHandler } from "./loop-end-handler.js";
import { loopStartHandler } from "./loop-start-handler.js";
import { routeHandler } from "./route-handler.js";
import { scriptHandler } from "./script-handler.js";
import { startFromTriggerHandler } from "./start-from-trigger-handler.js";
import { startHandler } from "./start-handler.js";
import { userInteractionHandler, type UserInteractionHandlerContext } from "./user-interaction-handler.js";
import { variableHandler } from "./variable-handler.js";

// NodeHandlerFn signature: all handlers receive globalContext as first parameter
export type NodeHandlerFn = (
  globalContext: GlobalContext,
  workflowExecutionEntity: WorkflowExecutionEntity,
  node: RuntimeNode,
  context?: unknown,
) => Promise<unknown>;

export function getNodeHandler(nodeType: string): NodeHandlerFn {
  const handlers: Record<string, NodeHandlerFn> = {
    // Handlers that need globalContext as first param
    AGENT_LOOP: (globalContext, workflowExecutionEntity, node, context) =>
      agentLoopHandler(globalContext, workflowExecutionEntity.getExecution(), node, context as AgentLoopHandlerContext),
    SCRIPT: (globalContext, workflowExecutionEntity, node, context) =>
      scriptHandler(globalContext, workflowExecutionEntity, node, context),
    
    // Handlers that don't use globalContext (ignore it)
    ADD_TOOL: (_gc, workflowExecutionEntity, node, context) =>
      addToolHandler(workflowExecutionEntity.getExecution(), node, context as AddToolHandlerContext),
    CONTEXT_PROCESSOR: (_gc, workflowExecutionEntity, node, context) =>
      contextProcessorHandler(workflowExecutionEntity.getExecution(), node, context as ContextProcessorHandlerContext),
    CONTINUE_FROM_TRIGGER: (_gc, workflowExecutionEntity, node, _ctx) =>
      continueFromTriggerHandler(workflowExecutionEntity, node),
    END: (_gc, workflowExecutionEntity, node, _ctx) => endHandler(workflowExecutionEntity, node),
    FORK: (_gc, workflowExecutionEntity, node, _ctx) => forkHandler(workflowExecutionEntity, node),
    JOIN: (_gc, workflowExecutionEntity, node, _ctx) => joinHandler(workflowExecutionEntity, node),
    LLM: (_gc, workflowExecutionEntity, node, context) =>
      llmHandler(workflowExecutionEntity.getExecution(), node, context as LLMHandlerContext),
    LOOP_END: (_gc, workflowExecutionEntity, node, _ctx) => loopEndHandler(workflowExecutionEntity, node),
    LOOP_START: (_gc, workflowExecutionEntity, node, _ctx) => loopStartHandler(workflowExecutionEntity, node),
    ROUTE: (_gc, workflowExecutionEntity, node, _ctx) => routeHandler(workflowExecutionEntity, node),
    START_FROM_TRIGGER: (_gc, workflowExecutionEntity, node, context) =>
      startFromTriggerHandler(workflowExecutionEntity, node, context as import("./start-from-trigger-handler.js").StartFromTriggerHandlerContext),
    START: (_gc, workflowExecutionEntity, node, _ctx) => startHandler(workflowExecutionEntity, node),
    USER_INTERACTION: (_gc, workflowExecutionEntity, node, context) =>
      userInteractionHandler(workflowExecutionEntity.getExecution(), node, context as UserInteractionHandlerContext),
    VARIABLE: (_gc, workflowExecutionEntity, node, _ctx) => variableHandler(workflowExecutionEntity, node),
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
