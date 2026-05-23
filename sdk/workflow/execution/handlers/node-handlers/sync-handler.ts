/**
 * Sync Node Handler - Explicit synchronization between fork branches
 * 
 * Purpose:
 * - Provides explicit data transfer between parallel execution branches
 * - Eliminates implicit state sharing through global variables
 * - Variables are deep cloned during transfer to maintain complete isolation
 * 
 * Usage:
 * - Place SYNC nodes in fork branches where cross-branch data is needed
 * - Configure sourcePathId to specify which branch to sync from
 * - Define variableMappings for explicit variable transfer
 * - Optionally wait for source branch completion before syncing
 */

import type { RuntimeNode, SyncNodeConfig, MessageContextRegistry, WorkflowExecution } from "@wf-agent/types";
import type { WorkflowExecutionEntity } from "../../../entities/workflow-execution-entity.js";
import type { GlobalContext } from "../../../../core/global-context.js";
import type { WorkflowExecutionRegistry } from "../../../stores/workflow-execution-registry.js";
import type { EventRegistry } from "../../../../core/registry/event-registry.js";
import { RuntimeValidationError } from "@wf-agent/types";
import { createContextualLogger } from "../../../../utils/contextual-logger.js";
import * as Identifiers from "../../../../core/di/service-identifiers.js";
import {
  buildNodeSyncStartedEvent,
  buildNodeSyncCompletedEvent,
  buildNodeSyncFailedEvent,
} from "../../utils/event/index.js";
import { emit } from "../../../../core/utils/event/event-emitter.js";
import { getErrorOrNew } from "@wf-agent/common-utils";

const logger = createContextualLogger({ component: "sync-handler" });

/**
 * Check if the node can be executed
 */
function canExecute(workflowExecutionEntity: WorkflowExecutionEntity): boolean {
  if (workflowExecutionEntity.getStatus() !== "RUNNING") {
    return false;
  }
  return true;
}

/**
 * Find source branch execution entity by path ID using parent's SyncBarrier
 * 
 * This implementation uses the SyncBarrier attached to the parent execution
 * to locate sibling branch executions by their forkPathId.
 * 
 * @param workflowExecutionEntity Current execution entity (child branch)
 * @param sourcePathId The fork path ID to find
 * @param executionRegistry Registry to retrieve execution entities
 * @returns Source execution entity or undefined
 */
async function findSourceExecution(
  workflowExecutionEntity: WorkflowExecutionEntity,
  sourcePathId: string,
  executionRegistry: WorkflowExecutionRegistry
): Promise<WorkflowExecutionEntity | undefined> {
  // Get parent execution to access its SyncBarrier
  const parentContext = workflowExecutionEntity.getParentContext();
  
  if (!parentContext || parentContext.parentType !== 'WORKFLOW') {
    logger.warn("SYNC node: Cannot find parent execution", {
      currentExecutionId: workflowExecutionEntity.id,
      sourcePathId,
      note: "SYNC nodes must be executed within a fork branch that has a WORKFLOW parent"
    });
    return undefined;
  }
  
  const parentExecutionId = parentContext.parentId;
  
  // Get parent execution entity from registry
  const parentExecutionEntity = executionRegistry.get(parentExecutionId);
  
  if (!parentExecutionEntity) {
    logger.error("SYNC node: Parent execution not found in registry", {
      currentExecutionId: workflowExecutionEntity.id,
      parentExecutionId,
    });
    return undefined;
  }
  
  // Get parent's SyncBarrier
  const syncBarrier = parentExecutionEntity.getSyncBarrier();
  
  if (!syncBarrier) {
    logger.error("SYNC node: Parent execution does not have SyncBarrier initialized", {
      currentExecutionId: workflowExecutionEntity.id,
      parentExecutionId,
      note: "This indicates the FORK node was not processed correctly",
    });
    return undefined;
  }
  
  // Use SyncBarrier to lookup execution ID by path ID
  const sourceExecutionId = syncBarrier.getExecutionIdByPath(sourcePathId);
  
  if (!sourceExecutionId) {
    logger.error("SYNC node: Fork path not registered in SyncBarrier", {
      currentExecutionId: workflowExecutionEntity.id,
      sourcePathId,
      availablePaths: syncBarrier.getAllPathIds(),
    });
    return undefined;
  }
  
  // Get source execution entity from registry
  const sourceExecutionEntity = executionRegistry.get(sourceExecutionId);
  
  if (!sourceExecutionEntity) {
    logger.error("SYNC node: Source execution not found in registry", {
      currentExecutionId: workflowExecutionEntity.id,
      sourceExecutionId,
      sourcePathId,
    });
    return undefined;
  }
  
  logger.debug("Found source execution via SyncBarrier", {
    currentExecutionId: workflowExecutionEntity.id,
    sourcePathId,
    sourceExecutionId,
    parentExecutionId,
  });
  
  return sourceExecutionEntity;
}

/**
 * Wait for source execution to complete using SyncBarrier
 * 
 * Uses the parent execution's SyncBarrier to wait for the source branch
 * to complete via event-driven mechanism with optional timeout.
 * 
 * @param workflowExecutionEntity Current execution entity
 * @param sourcePathId The fork path ID to wait for
 * @param timeout Timeout in seconds (0 = no timeout)
 * @param executionRegistry Registry to retrieve execution entities
 */
async function waitForSourceCompletion(
  workflowExecutionEntity: WorkflowExecutionEntity,
  sourcePathId: string,
  timeout: number,
  executionRegistry: WorkflowExecutionRegistry
): Promise<void> {
  // Get parent execution to access its SyncBarrier
  const parentContext = workflowExecutionEntity.getParentContext();
  
  if (!parentContext || parentContext.parentType !== 'WORKFLOW') {
    throw new Error(
      `SYNC node cannot wait for completion: No WORKFLOW parent found for execution ${workflowExecutionEntity.id}`
    );
  }
  
  const parentExecutionEntity = executionRegistry.get(parentContext.parentId);
  
  if (!parentExecutionEntity) {
    throw new Error(
      `SYNC node cannot wait for completion: Parent execution not found: ${parentContext.parentId}`
    );
  }
  
  // Get parent's SyncBarrier
  const syncBarrier = parentExecutionEntity.getSyncBarrier();
  
  if (!syncBarrier) {
    throw new Error(
      `SYNC node cannot wait for completion: Parent execution has no SyncBarrier`
    );
  }
  
  // Use SyncBarrier to wait for branch completion (reuses existing logic)
  logger.debug("Waiting for source branch completion via SyncBarrier", {
    sourcePathId,
    timeout: timeout > 0 ? `${timeout}s` : "infinite",
  });
  
  await syncBarrier.waitForBranchCompletion(sourcePathId, timeout);
  
  logger.debug("Source branch completed", {
    sourcePathId,
  });
}

/**
 * Sync Node Processing Function
 * 
 * Performs explicit variable synchronization from source branch to target branch
 * with deep cloning to maintain complete isolation.
 * 
 * @param globalContext Global application context (for event emission and registry access)
 * @param workflowExecutionEntity Current workflow execution entity (target branch)
 * @param node Runtime node definition containing sync configuration
 * @returns Execution result
 */
export async function syncHandler(
  globalContext: GlobalContext,
  workflowExecutionEntity: WorkflowExecutionEntity,
  node: RuntimeNode
): Promise<unknown> {
  // Check if execution is possible
  if (!canExecute(workflowExecutionEntity)) {
    return {
      nodeId: node.id,
      nodeType: node.type,
      status: "SKIPPED",
      step: workflowExecutionEntity.getNodeResults().length + 1,
      executionTime: 0,
    };
  }

  const config = node.config as SyncNodeConfig;

  // Validate configuration
  if (!config.sourcePathId) {
    throw new RuntimeValidationError("Sync node must have sourcePathId configured", {
      operation: "syncHandler",
      field: "sourcePathId",
      context: { nodeId: node.id },
    });
  }

  if (!config.variableMappings || config.variableMappings.length === 0) {
    logger.warn("Sync node has no variable mappings configured", {
      nodeId: node.id,
      sourcePathId: config.sourcePathId,
    });
    // Continue anyway - might be used for synchronization only
  }

  logger.debug("Executing SYNC node", {
    nodeId: node.id,
    sourcePathId: config.sourcePathId,
    targetPathId: config.targetPathId,
    variableCount: config.variableMappings?.length || 0,
    waitForCompletion: config.waitForCompletion ?? true,
  });

  // Get required dependencies from global context
  const executionRegistry = globalContext.container.get(
    Identifiers.WorkflowExecutionRegistry
  ) as WorkflowExecutionRegistry;
  
  const eventManager = globalContext.container.get(
    Identifiers.EventRegistry
  ) as EventRegistry;

  if (!executionRegistry) {
    throw new Error("WorkflowExecutionRegistry not available in global context");
  }
  
  if (!eventManager) {
    throw new Error("EventRegistry not available in global context");
  }

  // Get parent execution ID for event correlation
  const parentContext = workflowExecutionEntity.getParentContext();
  const parentExecutionId = parentContext?.parentType === 'WORKFLOW' && parentContext.parentId
    ? parentContext.parentId
    : '';

  // Emit NODE_SYNC_STARTED event with parent execution ID
  await emit(eventManager, buildNodeSyncStartedEvent({
    executionId: workflowExecutionEntity.id,
    workflowId: workflowExecutionEntity.getWorkflowId(),
    nodeId: node.id,
    sourcePathId: config.sourcePathId,
    parentExecutionId,
    targetPathId: config.targetPathId,
  }));

  try {
    // Step 1: Find source execution entity via SyncBarrier
    const sourceExecutionEntity = await findSourceExecution(
      workflowExecutionEntity,
      config.sourcePathId,
      executionRegistry
    );

    if (!sourceExecutionEntity) {
      throw new RuntimeValidationError(
        `Source execution not found for pathId: ${config.sourcePathId}`,
        {
          operation: "syncHandler",
          field: "sourcePathId",
          context: {
            nodeId: node.id,
            sourcePathId: config.sourcePathId,
            currentExecutionId: workflowExecutionEntity.id,
          },
        }
      );
    }

    // Step 2: Wait for source completion if configured
    const shouldWait = config.waitForCompletion ?? true;
    if (shouldWait) {
      const timeout = config.timeout ?? 0;
      logger.debug("Waiting for source execution completion", {
        sourceExecutionId: sourceExecutionEntity.id,
        timeout,
      });
      
      await waitForSourceCompletion(
        workflowExecutionEntity,
        config.sourcePathId,
        timeout,
        executionRegistry
      );
    }

    // Step 3: Import variables from source to target with deep cloning
    if (config.variableMappings && config.variableMappings.length > 0) {
      logger.debug("Importing variables from source to target", {
        mappingCount: config.variableMappings.length,
      });

      workflowExecutionEntity.variableStateManager.importVariables(
        sourceExecutionEntity.variableStateManager,
        config.variableMappings
      );

      logger.info("SYNC completed successfully", {
        nodeId: node.id,
        importedVariableCount: config.variableMappings.length,
      });
    }

    // Step 4: Process dataInputs - map execution input data to target branch variables
    if (config.dataInputs && config.dataInputs.length > 0) {
      logger.debug("Processing data inputs for target branch", {
        count: config.dataInputs.length,
      });
      const input = workflowExecutionEntity.getInput ? workflowExecutionEntity.getInput() : {};
      for (const inputDef of config.dataInputs) {
        const { parentField, internalName, required, defaultValue } = inputDef;
        let value = input[parentField];
        if (value === undefined) {
          if (defaultValue !== undefined) {
            value = defaultValue;
          } else if (required) {
            throw new RuntimeValidationError(
              `Required data input '${parentField}' (mapped to variable '${internalName}') is missing`,
              { operation: "syncHandler", field: parentField }
            );
          }
        }
        if (value !== undefined) {
          workflowExecutionEntity.variableStateManager.setVariable(internalName, value);
        }
      }
    }

    // Step 5: Sync message contexts from source to target if configured
    if (config.messageInputs && config.messageInputs.length > 0) {
      logger.debug("Syncing message contexts from source to target", {
        count: config.messageInputs.length,
      });
      const sourceExecution = sourceExecutionEntity.getExecution();
      const targetExecution = workflowExecutionEntity.getExecution();
      const sourceRegistry = (sourceExecution as WorkflowExecution & { messageContextRegistry?: MessageContextRegistry }).messageContextRegistry;
      const targetRegistry = (targetExecution as WorkflowExecution & { messageContextRegistry?: MessageContextRegistry }).messageContextRegistry;
      
      if (sourceRegistry && targetRegistry) {
        for (const inputDef of config.messageInputs) {
          const { externalName, internalName, required, defaultMessages } = inputDef;
          const sourceContext = sourceRegistry.get(externalName);
          
          if (sourceContext) {
            targetRegistry.register({
              id: internalName,
              messages: [...sourceContext.messages],
              createdAt: Date.now(),
              updatedAt: Date.now(),
              metadata: {
                ...sourceContext.metadata,
                syncedFrom: externalName,
                sourceExecutionId: sourceExecutionEntity.id,
              } as Record<string, unknown>,
            });
          } else if (required) {
            throw new RuntimeValidationError(
              `Required message context '${externalName}' not found in source branch`,
              { operation: "syncHandler", field: "messageInputs", value: externalName }
            );
          } else if (defaultMessages && defaultMessages.length > 0) {
            targetRegistry.register({
              id: internalName,
              messages: [...defaultMessages],
              createdAt: Date.now(),
              updatedAt: Date.now(),
            });
          }
        }
      } else {
        logger.warn("Message context registries not available for context sync", {
          sourceRegistryAvailable: !!sourceRegistry,
          targetRegistryAvailable: !!targetRegistry,
        });
      }
    }

    // Emit NODE_SYNC_COMPLETED event with enhanced fields
    await emit(eventManager, buildNodeSyncCompletedEvent({
      executionId: workflowExecutionEntity.id,
      workflowId: workflowExecutionEntity.getWorkflowId(),
      nodeId: node.id,
      sourcePathId: config.sourcePathId,
      parentExecutionId,
      variableCount: config.variableMappings?.length || 0,
      dataCount: config.dataInputs?.length || 0,
      messageCount: config.messageInputs?.length || 0,
    }));

    // Build synced variables record from mappings
    const syncedVariables: Record<string, unknown> = {};
    if (config.variableMappings) {
      for (const mapping of config.variableMappings) {
        const value = sourceExecutionEntity.variableStateManager.getVariable(mapping.externalName);
        if (value !== undefined) {
          syncedVariables[mapping.internalName] = value;
        }
      }
    }

    return {
      syncedFromPath: config.sourcePathId,
      syncedVariables: Object.keys(syncedVariables).length > 0 ? syncedVariables : undefined,
      syncedVariableCount: config.variableMappings?.length || 0,
      syncedDataCount: config.dataInputs?.length || 0,
      syncedMessageCount: config.messageInputs?.length || 0,
      completed: true,
    };
    
  } catch (error) {
    // Emit NODE_SYNC_FAILED event with parent execution ID
    await emit(eventManager, buildNodeSyncFailedEvent({
      executionId: workflowExecutionEntity.id,
      workflowId: workflowExecutionEntity.getWorkflowId(),
      nodeId: node.id,
      sourcePathId: config.sourcePathId,
      parentExecutionId,
      error: getErrorOrNew(error),
    }));
    
    throw error;
  }
}
