/**
 * Subgraph Processing Function
 * Responsible for handling the logic related to the entry and exit of subgraphs
 *
 * Responsibilities:
 * - Handle the logic for subgraphs to enter
 * - Handle the logic for subgraphs to exit
 * - Create metadata for the subgraph context
 * - Manage message context passing between parent and subgraph workflows
 * - Manage variable scope isolation using VariableManager's scope stack
 *
 * Design Principles:
 * - Reuse existing path parsing functions
 * - Avoid duplicating the implementation of variable parsing logic
 * - Provide a clear interface for subgraph processing
 * - Use pure functions with no internal state
 * - Leverage VariableManager's scope stack for proper variable isolation
 *
 * Context Management:
 * - Subgraphs can explicitly declare message context inputs/outputs via START node configuration
 * - Message contexts are passed through messagePassing configuration in SUBGRAPH nodes
 * - Contexts are copied (shallow copy) to avoid conflicts between parent and child workflows
 * - Variables are isolated using scope stack - child scopes cannot access parent variables unless explicitly mapped
 */

import type { WorkflowExecutionEntity } from "../../entities/workflow-execution-entity.js";
import type { StaticNode, WorkflowNode, WorkflowStartConfig, SubgraphNodeConfig, NamedMessageContext, MessageContextRegistry, WorkflowExecution } from "@wf-agent/types";
import { now } from "@wf-agent/common-utils";
import {
  checkWorkflowInterruption,
  shouldContinue,
  getWorkflowInterruptionDescription,
} from "../../../core/utils/interruption/index.js";
import { createContextualLogger } from "../../../utils/contextual-logger.js";
import { validateAndMapMessageContexts } from "../../validation/utils/message-context-validator.js";

const logger = createContextualLogger({ component: "subgraph-handler" });



/**
 * Enter the subgraph
 * @param executionEntity WorkflowExecution entity
 * @param workflowId Subgraph workflow ID
 * @param parentWorkflowId Parent workflow ID
 * @param input Subgraph input
 * @param subgraphNode The SUBGRAPH node from parent workflow (required for message context passing)
 */
export async function enterSubgraph(
  executionEntity: WorkflowExecutionEntity,
  workflowId: string,
  parentWorkflowId: string,
  input: Record<string, unknown>,
  subgraphNode: StaticNode,
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

  // Enter a new variable scope for isolation
  // This ensures that variables created in the subgraph don't leak to the parent
  executionEntity.variableStateManager.enterSubgraphScope();
  logger.debug("Entered new variable scope for subgraph", {
    executionId: executionEntity.id,
    workflowId,
  });

  // Handle message context passing (required)
  await handleEnterSubgraphMessageContexts(executionEntity, subgraphNode, workflowId);

  await executionEntity.enterSubgraph(workflowId, parentWorkflowId, input);
}

/**
 * Exit the subgraph
 * @param executionEntity WorkflowExecution entity
 * @param subgraphNode The SUBGRAPH node from parent workflow (required for message context passing)
 */
export async function exitSubgraph(
  executionEntity: WorkflowExecutionEntity,
  subgraphNode: StaticNode,
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

  // NOTE: Variable output mapping is handled by the scope stack mechanism
  // When we exit the scope, all subgraph-local variables are automatically discarded
  // If you need to return specific values, use the workflow's output mechanism

  // Handle message context passing (required)
  await handleExitSubgraphMessageContexts(executionEntity, subgraphNode);

  // Exit the variable scope
  // This discards all variables created within the subgraph scope
  executionEntity.variableStateManager.exitSubgraphScope();
  logger.debug("Exited variable scope for subgraph", {
    executionId: executionEntity.id,
  });

  await executionEntity.exitSubgraph();
}

/**
 * Handle message context passing when entering a subgraph
 * Copies parent workflow contexts to subgraph internal names
 * 
 * @param executionEntity WorkflowExecution entity
 * @param subgraphNode The SUBGRAPH node
 * @param subgraphWorkflowId Subgraph workflow ID
 */
async function handleEnterSubgraphMessageContexts(
  executionEntity: WorkflowExecutionEntity,
  subgraphNode: StaticNode,
  subgraphWorkflowId: string,
): Promise<void> {
  // Access the MessageContextRegistry attached to workflowExecution
  const workflowExecution = executionEntity.getWorkflowExecutionData() as WorkflowExecution & { messageContextRegistry?: MessageContextRegistry };
  const registry = workflowExecution.messageContextRegistry;
  
  if (!registry) {
    throw new Error(
      `MessageContextRegistry not available for subgraph '${subgraphWorkflowId}'. This is required for message context passing.`
    );
  }

  const subgraphConfig = subgraphNode.config as SubgraphNodeConfig;
  
  // Get the subgraph's START node to access its messageInputs declaration
  const subgraphGraph = executionEntity.getGraph();
  const startNodeId = subgraphGraph.startNodeId;
  if (!startNodeId) {
    throw new Error(
      `Subgraph '${subgraphWorkflowId}' has no START node. Cannot validate message context configuration.`
    );
  }
  
  const startNode = subgraphGraph.getNode(startNodeId);
  
  if (!startNode) {
    throw new Error(
      `Subgraph '${subgraphWorkflowId}' START node not found. Cannot validate message context configuration.`
    );
  }

  // Validate and get mapping (this will throw if validation fails)
  const startNodeAsStatic = ('originalNode' in startNode ? (startNode as WorkflowNode).originalNode : startNode) as StaticNode;
  const mappingResult = validateAndMapMessageContexts(subgraphNode, startNodeAsStatic);
  
  if (mappingResult.isErr()) {
    const errorMessages = mappingResult.error.map(e => e.message).join('; ');
    throw new Error(
      `Message context validation failed for subgraph '${subgraphWorkflowId}': ${errorMessages}`
    );
  }

  const mapping = mappingResult.value;
  const startConfig = startNode.config as WorkflowStartConfig;

  // Copy input contexts from parent to subgraph
  for (const [parentContextId, internalName] of mapping.inputMapping) {
    const parentContext = registry.get(parentContextId);
    
    if (!parentContext) {
      // Check if this is a required input
      const inputDef = startConfig.messageInputs?.find(
        (i: { internalName: string }) => i.internalName === internalName
      );
      
      if (inputDef?.required !== false) {
        // Default to required if not explicitly marked as optional
        throw new Error(
          `Required context '${parentContextId}' not found in parent workflow. ` +
          `Cannot pass to subgraph '${subgraphWorkflowId}' as '${internalName}'.`
        );
      }
      
      // Optional input with default messages
      if (inputDef && 'defaultMessages' in inputDef && inputDef.defaultMessages) {
        const defaultContext: NamedMessageContext = {
          id: internalName,
          messages: inputDef.defaultMessages,
          createdAt: now(),
          updatedAt: now(),
          metadata: {
            description: `Default messages for optional input '${internalName}'`,
          },
        };
        registry.register(defaultContext);
        logger.debug(`Created optional context '${internalName}' with default messages`, {
          executionId: executionEntity.id,
        });
      }
      continue;
    }
    
    // Create a shallow copy of messages to avoid conflicts
    const copiedContext: NamedMessageContext = {
      id: internalName,
      messages: [...parentContext.messages],
      createdAt: now(),
      updatedAt: now(),
      metadata: {
        ...parentContext.metadata,
        sourceContext: parentContextId,
        passedFromParent: true,
      } as Record<string, unknown>,
    };
    
    registry.register(copiedContext);
    
    logger.debug(`Passed context '${parentContextId}' as '${internalName}' to subgraph`, {
      executionId: executionEntity.id,
      messageCount: parentContext.messages.length,
    });
  }
}

/**
 * Handle message context passing when exiting a subgraph
 * Copies subgraph output contexts back to parent workflow
 * 
 * @param executionEntity WorkflowExecution entity
 * @param subgraphNode The SUBGRAPH node
 */
async function handleExitSubgraphMessageContexts(
  executionEntity: WorkflowExecutionEntity,
  subgraphNode: StaticNode,
): Promise<void> {
  // Access the MessageContextRegistry attached to workflowExecution
  const workflowExecution = executionEntity.getWorkflowExecutionData() as WorkflowExecution & { messageContextRegistry?: MessageContextRegistry };
  const registry = workflowExecution.messageContextRegistry;
  
  if (!registry) {
    throw new Error(
      `MessageContextRegistry not available. This is required for message context returning.`
    );
  }

  const subgraphConfig = subgraphNode.config as SubgraphNodeConfig;
  
  // Get the subgraph's START node to access its messageOutputs declaration
  const subgraphContext = executionEntity.getCurrentSubgraphContext();
  if (!subgraphContext) {
    throw new Error(
      `No active subgraph context. Cannot return message contexts.`
    );
  }

  const subgraphGraph = executionEntity.getGraph();
  const startNodeId = subgraphGraph.startNodeId;
  if (!startNodeId) {
    throw new Error(
      `Subgraph '${subgraphContext.workflowId}' has no START node. Cannot validate message context configuration.`
    );
  }
  
  const startNode = subgraphGraph.getNode(startNodeId);
  
  if (!startNode) {
    throw new Error(
      `Subgraph '${subgraphContext.workflowId}' START node not found. Cannot validate message context configuration.`
    );
  }

  // Validate and get mapping (this will throw if validation fails)
  const startNodeAsStatic = ('originalNode' in startNode ? (startNode as WorkflowNode).originalNode : startNode) as StaticNode;
  const mappingResult = validateAndMapMessageContexts(subgraphNode, startNodeAsStatic);
  
  if (mappingResult.isErr()) {
    const errorMessages = mappingResult.error.map(e => e.message).join('; ');
    throw new Error(
      `Message context validation failed on exit for subgraph '${subgraphContext.workflowId}': ${errorMessages}`
    );
  }

  const mapping = mappingResult.value;

  // Copy output contexts from subgraph back to parent
  for (const [internalName, parentContextId] of mapping.outputMapping) {
    const childContext = registry.get(internalName);
    
    if (!childContext) {
      throw new Error(
        `Expected output context '${internalName}' not found in subgraph. ` +
        `Cannot return to parent workflow as '${parentContextId}'.`
      );
    }
    
    // Update or create parent workflow's context
    if (registry.has(parentContextId)) {
      registry.update(parentContextId, [...childContext.messages]);
      
      logger.debug(`Returned context '${internalName}' as '${parentContextId}'`, {
        executionId: executionEntity.id,
        messageCount: childContext.messages.length,
      });
    } else {
      registry.register({
        id: parentContextId,
        messages: [...childContext.messages],
        createdAt: now(),
        updatedAt: now(),
        metadata: {
          description: `Output from subgraph '${subgraphContext.workflowId}'`,
          sourceSubgraph: subgraphContext.workflowId,
        } as Record<string, unknown>,
      });
      
      logger.debug(`Created new context '${parentContextId}' from subgraph output '${internalName}'`, {
        executionId: executionEntity.id,
        messageCount: childContext.messages.length,
      });
    }
  }
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
