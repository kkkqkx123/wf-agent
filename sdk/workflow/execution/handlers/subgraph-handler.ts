/**
 * Subgraph Processing Function
 * Responsible for handling the logic related to the entry and exit of subgraphs
 *
 * Responsibilities:
 * - Handle the logic for subgraphs to enter
 * - Handle the logic for subgraphs to exit
 * - Create metadata for the subgraph context
 * - Support context passing between parent and subgraph workflows
 *
 * Design Principles:
 * - Reuse existing path parsing functions
 * - Avoid duplicating the implementation of variable parsing logic
 * - Provide a clear interface for subgraph processing
 * - Use pure functions with no internal state
 */

import type { WorkflowExecutionEntity } from "../../entities/workflow-execution-entity.js";
import type { Node, SubgraphNodeConfig, NamedMessageContext, LLMMessage } from "@wf-agent/types";
import { RuntimeValidationError } from "@wf-agent/types";
import { now } from "@wf-agent/common-utils";
import {
  checkWorkflowInterruption,
  shouldContinue,
  getWorkflowInterruptionDescription,
} from "../../../core/utils/interruption/index.js";
import { createContextualLogger } from "../../../utils/contextual-logger.js";

const logger = createContextualLogger({ component: "subgraph-handler" });

/**
 * Enter the subgraph
 * @param executionEntity WorkflowExecution entity
 * @param workflowId Subgraph workflow ID
 * @param parentWorkflowId Parent workflow ID
 * @param input Subgraph input
 * @param subgraphNode Optional SUBGRAPH node definition (for contextPassing config)
 */
export async function enterSubgraph(
  executionEntity: WorkflowExecutionEntity,
  workflowId: string,
  parentWorkflowId: string,
  input: Record<string, unknown>,
  subgraphNode?: Node,
): Promise<void> {
  // Check for interruption before entering subgraph
  const abortSignal = executionEntity.getAbortSignal();
  const interruption = checkWorkflowInterruption(abortSignal);
  
  if (!shouldContinue(interruption)) {
    logger.info("Subgraph entry interrupted", {
      executionId: executionEntity.id,
      workflowId,
      interruptionType: interruption.type,
    });
    throw new Error(`Subgraph entry interrupted: ${getWorkflowInterruptionDescription(interruption)}`);
  }

  // Handle context passing if configured
  if (subgraphNode && subgraphNode.type === "SUBGRAPH") {
    const config = subgraphNode.config as SubgraphNodeConfig;
    if (config.contextPassing) {
      await handleContextPassingOnEntry(executionEntity, config.contextPassing);
    }
  }

  await executionEntity.enterSubgraph(workflowId, parentWorkflowId, input);
}

/**
 * Exit the subgraph
 * @param executionEntity WorkflowExecution entity
 * @param subgraphNode Optional SUBGRAPH node definition (for contextPassing config)
 */
export async function exitSubgraph(
  executionEntity: WorkflowExecutionEntity,
  subgraphNode?: Node,
): Promise<void> {
  // Check for interruption before exiting subgraph
  const abortSignal = executionEntity.getAbortSignal();
  const interruption = checkWorkflowInterruption(abortSignal);
  
  if (!shouldContinue(interruption)) {
    logger.info("Subgraph exit interrupted", {
      executionId: executionEntity.id,
      interruptionType: interruption.type,
    });
    throw new Error(`Subgraph exit interrupted: ${getWorkflowInterruptionDescription(interruption)}`);
  }

  // Handle context passing on exit if configured
  if (subgraphNode && subgraphNode.type === "SUBGRAPH") {
    const config = subgraphNode.config as SubgraphNodeConfig;
    if (config.contextPassing) {
      await handleContextPassingOnExit(executionEntity, config.contextPassing);
    }
  }

  await executionEntity.exitSubgraph();
}

/**
 * Get subgraph input
 * @param executionEntity WorkflowExecution entity
 * @returns Subgraph input data (using the variable system)
 */
export function getSubgraphInput(executionEntity: WorkflowExecutionEntity): Record<string, unknown> {
  // Using a variable system to retrieve input data
  return executionEntity.getAllVariables();
}

/**
 * Get the subgraph output
 * @param executionEntity WorkflowExecution entity
 * @returns Subgraph output data
 */
export function getSubgraphOutput(executionEntity: WorkflowExecutionEntity): Record<string, unknown> {
  const subgraphContext = executionEntity.getCurrentSubgraphContext();
  if (!subgraphContext) return {};

  // Get the output of the END node of the subgraph.
  const graph = executionEntity.getGraph();
  const endNodes = graph.endNodeIds;

  for (const endNodeId of endNodes) {
    const graphNode = graph.getNode(endNodeId);
    if (graphNode?.workflowId === subgraphContext.workflowId) {
      // Find the END node of the subgraph and obtain its output.
      // Note: NodeExecutionResult doesn't have a 'data' property, so we return an empty object
      // This may need to be updated based on the actual implementation
      return {};
    }
  }

  return {};
}

/**
 * Create subgraph context metadata
 * @param triggerId Trigger ID (optional)
 * @param mainExecutionId Main execution ID (optional)
 * @returns Subgraph context metadata
 */
export function createSubgraphMetadata(
  triggerId?: string,
  mainExecutionId?: string,
): Record<string, unknown> {
  const metadata: Record<string, unknown> = {
    timestamp: now(),
  };

  if (triggerId || mainExecutionId) {
    metadata["triggeredBy"] = {
      triggerId,
      mainExecutionId,
      timestamp: now(),
    };
    metadata["isTriggeredSubgraph"] = true;
  }

  return metadata;
}

/**
 * Handle context passing on subgraph entry
 * Copies specified contexts from parent to subgraph
 */
async function handleContextPassingOnEntry(
  executionEntity: WorkflowExecutionEntity,
  contextPassing: NonNullable<SubgraphNodeConfig["contextPassing"]>,
): Promise<void> {
  const registry = (executionEntity.getExecution() as any).messageContextRegistry;
  
  if (!registry) {
    logger.warn("MessageContextRegistry not found, skipping context passing");
    return;
  }

  const { contextIds, strategy } = contextPassing;
  const mode = strategy?.mode || 'clone';
  const namespace = strategy?.namespace;

  logger.info("Handling context passing on subgraph entry", {
    executionId: executionEntity.id,
    contextIds,
    mode,
    namespace,
  });

  // Copy each specified context
  for (const contextId of contextIds) {
    const sourceContext = registry.get(contextId);
    
    if (!sourceContext) {
      logger.warn(`Source context '${contextId}' not found, skipping`, {
        executionId: executionEntity.id,
      });
      continue;
    }

    // Determine target context ID (with optional namespace prefix)
    const targetContextId = namespace ? `${namespace}.${contextId}` : contextId;

    // Check if target already exists
    if (registry.has(targetContextId)) {
      logger.debug(`Target context '${targetContextId}' already exists, skipping`, {
        executionId: executionEntity.id,
      });
      continue;
    }

    // Clone or reference the context based on strategy
    let messagesToCopy: typeof sourceContext.messages;
    
    if (mode === 'clone') {
      // Deep clone messages
      messagesToCopy = JSON.parse(JSON.stringify(sourceContext.messages));
    } else if (mode === 'reference') {
      // Reference the same array (shared state)
      messagesToCopy = sourceContext.messages;
    } else if (mode === 'snapshot') {
      // Create immutable snapshot
      messagesToCopy = Object.freeze([...sourceContext.messages]);
    } else {
      messagesToCopy = JSON.parse(JSON.stringify(sourceContext.messages));
    }

    // Register the context in subgraph's registry
    registry.register({
      id: targetContextId,
      messages: messagesToCopy,
      createdAt: now(),
      updatedAt: now(),
      metadata: {
        ...sourceContext.metadata,
        description: `Copied from parent context '${contextId}' (${mode})`,
        sourceContextId: contextId,
        copiedAt: now(),
      },
    });

    logger.debug(`Context '${contextId}' copied to '${targetContextId}'`, {
      executionId: executionEntity.id,
      messageCount: messagesToCopy.length,
    });
  }
}

/**
 * Handle context passing on subgraph exit
 * Merges modified contexts back to parent workflow
 */
async function handleContextPassingOnExit(
  executionEntity: WorkflowExecutionEntity,
  contextPassing: NonNullable<SubgraphNodeConfig["contextPassing"]>,
): Promise<void> {
  const registry = (executionEntity.getExecution() as any).messageContextRegistry;
  
  if (!registry) {
    logger.warn("MessageContextRegistry not found, skipping context passing on exit");
    return;
  }

  const { contextIds, strategy } = contextPassing;
  const mergeToInitial = strategy?.mergeToInitial ?? false;
  const namespace = strategy?.namespace;

  logger.info("Handling context passing on subgraph exit", {
    executionId: executionEntity.id,
    contextIds,
    mergeToInitial,
    namespace,
  });

  // For each context that was passed in, merge changes back
  for (const contextId of contextIds) {
    const targetContextId = namespace ? `${namespace}.${contextId}` : contextId;
    const subgraphContext = registry.get(targetContextId);
    
    if (!subgraphContext) {
      logger.debug(`Subgraph context '${targetContextId}' not found, skipping merge`, {
        executionId: executionEntity.id,
      });
      continue;
    }

    // Get parent context (without namespace)
    const parentContext = registry.get(contextId);
    
    if (!parentContext) {
      logger.debug(`Parent context '${contextId}' not found, skipping merge`, {
        executionId: executionEntity.id,
      });
      continue;
    }

    // Merge strategy: append new messages from subgraph to parent
    if (mergeToInitial) {
      // Append all messages from subgraph context to parent
      const mergedMessages = [
        ...parentContext.messages,
        ...subgraphContext.messages.filter((msg: LLMMessage) => 
          !parentContext.messages.includes(msg)
        ),
      ];
      
      registry.update(contextId, mergedMessages);
      
      logger.debug(`Merged ${subgraphContext.messages.length} messages from '${targetContextId}' to '${contextId}'`, {
        executionId: executionEntity.id,
        totalMessages: mergedMessages.length,
      });
    } else {
      // Replace parent context with subgraph context
      registry.update(contextId, subgraphContext.messages);
      
      logger.debug(`Replaced '${contextId}' with '${targetContextId}' (${subgraphContext.messages.length} messages)`, {
        executionId: executionEntity.id,
      });
    }
  }
}
