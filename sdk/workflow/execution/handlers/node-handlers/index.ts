/**
 * Node handlers export
 */

import type { RuntimeNode } from "@wf-agent/types";
import type { WorkflowExecutionEntity } from "../../../entities/workflow-execution-entity.js";
import type { GlobalContext } from "../../../../core/global-context.js";
import type { StartFromTriggerHandlerContext } from "./start-from-trigger-handler.js";
import { agentLoopHandler, type AgentLoopHandlerContext } from "./agent-loop-handler.js";
import { contextProcessorHandler, type ContextProcessorHandlerContext } from "./context-processor-handler.js";
import { continueFromTriggerHandler } from "./continue-from-trigger-handler.js";
import { endHandler } from "./end-handler.js";
import { forkHandler } from "./fork-handler.js";
import type { ForkHandlerContext } from "../../types/fork.types.js";
import { joinHandler } from "./join-handler.js";
import { llmHandler, type LLMHandlerContext } from "./llm-handler.js";
import { loopEndHandler } from "./loop-end-handler.js";
import { loopStartHandler } from "./loop-start-handler.js";
import { routeHandler } from "./route-handler.js";
import { scriptHandler } from "./script-handler.js";
import { interactiveScriptHandler, type InteractiveScriptHandlerContext } from "./interactive-script-handler.js";
import { startFromTriggerHandler } from "./start-from-trigger-handler.js";
import { startHandler } from "./start-handler.js";
import { subgraphHandler, type SubgraphHandlerContext } from "./subgraph-handler.js";
import { embedStartHandler } from "./embed-start-handler.js";
import { embedEndHandler } from "./embed-end-handler.js";
import { userInteractionHandler, type UserInteractionHandlerContext } from "./user-interaction-handler.js";
import { variableHandler } from "./variable-handler.js";
import { toolVisibilityHandler, type ToolVisibilityHandlerContext } from "./tool-visibility-handler.js";
import { syncHandler } from "./sync-handler.js";

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
      agentLoopHandler(globalContext, workflowExecutionEntity as any, node, context as AgentLoopHandlerContext),
    SCRIPT: (globalContext, workflowExecutionEntity, node, _context) =>
      scriptHandler(globalContext, workflowExecutionEntity, node),
    INTERACTIVE_SCRIPT: (globalContext, workflowExecutionEntity, node, context) =>
      interactiveScriptHandler(globalContext, workflowExecutionEntity, node, context as InteractiveScriptHandlerContext),
    
    // Handlers that don't use globalContext (ignore it)
    CONTEXT_PROCESSOR: (_gc, workflowExecutionEntity, node, context) =>
      contextProcessorHandler(workflowExecutionEntity.getExecution(), node, context as ContextProcessorHandlerContext),
    CONTINUE_FROM_TRIGGER: (_gc, workflowExecutionEntity, node, _ctx) =>
      continueFromTriggerHandler(workflowExecutionEntity, node),
    END: (_gc, workflowExecutionEntity, node, _ctx) => endHandler(workflowExecutionEntity, node),
    FORK: (globalContext, workflowExecutionEntity, node, context) =>
      forkHandler(globalContext, workflowExecutionEntity, node, context as ForkHandlerContext | undefined),
    JOIN: (globalContext, workflowExecutionEntity, node, _ctx) => joinHandler(globalContext, workflowExecutionEntity, node),
    LLM: (_gc, workflowExecutionEntity, node, context) =>
      llmHandler(workflowExecutionEntity.getExecution(), node, context as LLMHandlerContext),
    LOOP_END: (_gc, workflowExecutionEntity, node, _ctx) => loopEndHandler(workflowExecutionEntity, node),
    LOOP_START: (_gc, workflowExecutionEntity, node, _ctx) => loopStartHandler(workflowExecutionEntity, node),
    ROUTE: (_gc, workflowExecutionEntity, node, _ctx) => routeHandler(workflowExecutionEntity, node),
    START_FROM_TRIGGER: (_gc, workflowExecutionEntity, node, context) =>
      startFromTriggerHandler(workflowExecutionEntity, node, context as StartFromTriggerHandlerContext),
    START: (_gc, workflowExecutionEntity, node, _ctx) => startHandler(workflowExecutionEntity, node),
    SUBGRAPH: (globalContext, workflowExecutionEntity, node, context) =>
      subgraphHandler(globalContext, workflowExecutionEntity, node, context as SubgraphHandlerContext),
    EMBED_START: (_gc, workflowExecutionEntity, node, _ctx) =>
      embedStartHandler(workflowExecutionEntity, node),
    EMBED_END: (_gc, workflowExecutionEntity, node, _ctx) =>
      embedEndHandler(workflowExecutionEntity, node),
    USER_INTERACTION: (_gc, workflowExecutionEntity, node, context) =>
      userInteractionHandler(workflowExecutionEntity.getExecution(), node, context as UserInteractionHandlerContext, workflowExecutionEntity),
    VARIABLE: (_gc, workflowExecutionEntity, node, _ctx) => variableHandler(workflowExecutionEntity, node),
    TOOL_VISIBILITY: (_gc, _we, node, context) =>
      toolVisibilityHandler(node, context as ToolVisibilityHandlerContext),
    SYNC: (globalContext, workflowExecutionEntity, node, _ctx) => 
      syncHandler(globalContext, workflowExecutionEntity, node),
  };

  const handler = handlers[nodeType];
  if (!handler) {
    throw new Error(`Unknown node type: ${nodeType}`);
  }
  return handler;
}

export {
  agentLoopHandler,
  contextProcessorHandler,
  continueFromTriggerHandler,
  endHandler,
  forkHandler,
  joinHandler,
  syncHandler,
  llmHandler,
  loopEndHandler,
  loopStartHandler,
  routeHandler,
  scriptHandler,
  interactiveScriptHandler,
  startFromTriggerHandler,
  startHandler,
  subgraphHandler,
  userInteractionHandler,
  variableHandler,
  toolVisibilityHandler,
};

export type { ToolVisibilityHandlerContext };
