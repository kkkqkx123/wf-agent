/**
 * Subgraph Processing Function (Phase 1: Scheme C - Message Context Only)
 * 
 * NOTE: This module now ONLY handles message context passing for subgraphs.
 * Variable import/export is handled directly in the SUBGRAPH node handler
 * using WorkflowExecutionBuilder.createChildExecution() and VariableManager.importVariables/exportVariables.
 *
 * Responsibilities:
 * - Handle message context passing when entering/exiting subgraphs
 * - Validate message context mappings between parent and child workflows
 * - Copy message contexts to avoid conflicts
 *
 * Design Principles:
 * - Pure functions with no internal state
 * - Reuse existing path parsing functions
 * - Clear separation: variables via VariableManager, messages via this module
 */

import type { WorkflowExecutionEntity } from "../../entities/workflow-execution-entity.js";
import type { StaticNode, WorkflowNode, WorkflowStartConfig, NamedMessageContext, MessageContextRegistry, WorkflowExecution, SubgraphNodeConfig } from "@wf-agent/types";
import { now } from "@wf-agent/common-utils";
import {
  getWorkflowInterruptionDescription,
  executeWithInterruptionHandling,
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
  subgraphNode: StaticNode & { config: SubgraphNodeConfig },
): Promise<void> {
  const abortSignal = executionEntity.getAbortSignal();

  // Use unified interruption handling wrapper
  const result = await executeWithInterruptionHandling(
    async () => {
      // Phase 1: Variable import is handled by SUBGRAPH node handler via createSubgraph()
      // This function only handles message context passing
      logger.debug("Entering subgraph (variables handled by SUBGRAPH node handler)", {
        executionId: executionEntity.id,
        workflowId,
      });

      try {
        // Handle message context passing (required)
        await handleEnterSubgraphMessageContexts(executionEntity, subgraphNode, workflowId);

        executionEntity.enterSubgraph(workflowId, parentWorkflowId, input);
      } catch (error) {
        logger.debug("Subgraph entry failed", {
          executionId: executionEntity.id,
        });
        throw error;
      }
    },
    abortSignal,
  );

  // Handle interruption gracefully
  if (!result.success) {
    const interruption = result.interruption;
    logger.info("Subgraph entry interrupted", {
      executionId: executionEntity.id,
      workflowId,
      interruptionType: interruption.type,
    });
    
    throw new Error(`Subgraph entry interrupted: ${getWorkflowInterruptionDescription(interruption)}`);
  }
}

/**
 * Exit the subgraph
 * @param executionEntity WorkflowExecution entity
 * @param subgraphNode The SUBGRAPH node from parent workflow (required for message context passing)
 */
export async function exitSubgraph(
  executionEntity: WorkflowExecutionEntity,
  subgraphNode: StaticNode & { config: SubgraphNodeConfig },
): Promise<void> {
  const abortSignal = executionEntity.getAbortSignal();

  // Use unified interruption handling wrapper
  const result = await executeWithInterruptionHandling(
    async () => {
      // Phase 1: Variable export is handled by SUBGRAPH node handler via exportVariables()
      // This function only handles message context passing
      logger.debug("Exiting subgraph (variables handled by SUBGRAPH node handler)", {
        executionId: executionEntity.id,
      });

      // Handle message context passing (required)
      await handleExitSubgraphMessageContexts(executionEntity, subgraphNode);

      await executionEntity.exitSubgraph();
    },
    abortSignal,
  );

  // Handle interruption gracefully
  if (!result.success) {
    const interruption = result.interruption;
    logger.info("Subgraph exit interrupted", {
      executionId: executionEntity.id,
      interruptionType: interruption.type,
    });
    
    throw new Error(`Subgraph exit interrupted: ${getWorkflowInterruptionDescription(interruption)}`);
  }
}

/**
 * Handle message context passing when entering a subgraph
 * Copies parent workflow contexts to subgraph internal names
 * 
 * @param executionEntity WorkflowExecution entity
 * @param subgraphNode The SUBGRAPH node with SubgraphNodeConfig
 * @param subgraphWorkflowId Subgraph workflow ID
 */
async function handleEnterSubgraphMessageContexts(
  executionEntity: WorkflowExecutionEntity,
  subgraphNode: StaticNode & { config: SubgraphNodeConfig },
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
 * @param subgraphNode The SUBGRAPH node with SubgraphNodeConfig
 */
async function handleExitSubgraphMessageContexts(
  executionEntity: WorkflowExecutionEntity,
  subgraphNode: StaticNode & { config: SubgraphNodeConfig },
): Promise<void> {
  // Access the MessageContextRegistry attached to workflowExecution
  const workflowExecution = executionEntity.getWorkflowExecutionData() as WorkflowExecution & { messageContextRegistry?: MessageContextRegistry };
  const registry = workflowExecution.messageContextRegistry;
  
  if (!registry) {
    throw new Error(
      `MessageContextRegistry not available. This is required for message context returning.`
    );
  }

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
