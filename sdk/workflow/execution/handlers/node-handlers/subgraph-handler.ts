/**
 * SUBGRAPH Node Handler - Subgraph Execution Handler (Phase 1: Scheme C)
 * 
 * Handles SUBGRAPH node execution by creating an independent child workflow execution entity.
 * This replaces the old graph expansion model with proper parent-child execution relationship.
 * 
 * Responsibilities:
 * - Create independent subgraph execution entity using WorkflowExecutionBuilder.createSubgraph()
 * - Execute the subgraph synchronously (async execution should use FORK instead)
 * - Export output variables back to parent workflow upon completion
 * - Handle message context passing (via subgraph-handler utilities)
 * 
 * Design Principles:
 * - Complete isolation: Child workflow has its own VariableManager, ExecutionState, etc.
 * - Explicit variable mapping: All cross-boundary transfers use importVariables/exportVariables
 * - Deep clone semantics: Variables are deep cloned on import/export to prevent state pollution
 * - Consistent with Fork/Triggered patterns: Uses same architecture as other child executions
 */

import type { RuntimeNode } from "@wf-agent/types";
import type { WorkflowExecutionEntity } from "../../../entities/workflow-execution-entity.js";
import type { GlobalContext } from "../../../../core/global-context.js";
import type { SubgraphNodeConfig } from "@wf-agent/types";
import { createContextualLogger } from "../../../../utils/contextual-logger.js";
import { enterSubgraph, exitSubgraph } from "../subgraph-handler.js";
import * as Identifiers from "../../../../core/di/service-identifiers.js";
import type { WorkflowExecutor } from "../../executors/workflow-executor.js";
import type { WorkflowExecutionBuilder } from "../../factories/workflow-execution-builder.js";
import { getErrorOrNew } from "@wf-agent/common-utils";
import {
  createSubgraphResult,
  type SubgraphExecutionResult,
} from "../../types/subworkflow-result.types.js";
import { cleanupFailedSubworkflow } from "../../utils/subworkflow-cleanup.js";

const logger = createContextualLogger({ component: "subgraph-node-handler" });

/**
 * SUBGRAPH Node Handler Context
 */
export interface SubgraphHandlerContext {
  /** Execution builder for creating subgraph execution entities */
  executionBuilder: WorkflowExecutionBuilder;
  /** Workflow executor for executing subgraphs */
  workflowExecutor: WorkflowExecutor;
}

/**
 * Execute SUBGRAPH node - Create and execute independent child workflow
 * 
 * NOTE: SUBGRAPH only supports synchronous execution.
 * For asynchronous/parallel execution, use FORK nodes instead.
 * 
 * @param globalContext Global application context (not used directly)
 * @param workflowExecutionEntity Parent workflow execution entity
 * @param node SUBGRAPH runtime node
 * @param context Handler context containing executionBuilder and workflowExecutor
 * @returns SubgraphExecutionResult with subgraph entity, execution result, and timing
 */
export async function subgraphHandler(
  globalContext: GlobalContext,
  workflowExecutionEntity: WorkflowExecutionEntity,
  node: RuntimeNode,
  context?: SubgraphHandlerContext,
): Promise<SubgraphExecutionResult> {
  const config = node.config as SubgraphNodeConfig;
  const subworkflowId = config.subgraphId;
  
  if (!subworkflowId) {
    throw new Error(`SUBGRAPH node '${node.id}' missing subgraphId configuration`);
  }

  logger.info("Executing SUBGRAPH node", {
    executionId: workflowExecutionEntity.id,
    nodeId: node.id,
    subworkflowId,
  });

  // Step 1: Validate required dependencies from context
  const executionBuilder = context?.executionBuilder;
  const executor = context?.workflowExecutor;

  if (!executionBuilder) {
    throw new Error('WorkflowExecutionBuilder required for SUBGRAPH execution');
  }
  if (!executor) {
    throw new Error('WorkflowExecutor required for SUBGRAPH execution');
  }

  // Step 2: Build variable mappings from node config
  const variableMapping = {
    inputs: config.variableInputs || [],
    outputs: config.variableOutputs || [],
  };

  let subgraphEntity: WorkflowExecutionEntity | null = null;
  const executionStartTime = Date.now();
  
  try {
    // Step 3: Create independent subgraph execution entity
    const buildResult = await executionBuilder.createSubgraph(workflowExecutionEntity, {
      subworkflowId,
      nodeId: node.id,
      variableMapping,
      async: false, // SUBGRAPH always executes synchronously
    });

    subgraphEntity = buildResult.workflowExecutionEntity;

    logger.debug("Subgraph execution entity created", {
      subgraphExecutionId: subgraphEntity.id,
      parentExecutionId: workflowExecutionEntity.id,
    });

    // Step 4: Handle message context entering (via subgraph-handler)
    // Note: We need to pass the original SUBGRAPH static node for message context validation
    const staticNode = ('originalNode' in node ? node.originalNode : node) as any;
    if (staticNode && staticNode.type === 'SUBGRAPH') {
      await enterSubgraph(
        workflowExecutionEntity,
        subworkflowId,
        workflowExecutionEntity.getWorkflowId(),
        {}, // Input will come from variable imports
        staticNode
      );
    }

    // Step 5: Execute subgraph synchronously
    const executionStartTime = Date.now();
    const executionResult = await executor.executeWorkflow(subgraphEntity);
    const executionDuration = Date.now() - executionStartTime;
    
    logger.info("Subgraph completed", {
      subgraphExecutionId: subgraphEntity.id,
      duration: executionDuration,
    });

    // Record SUBGRAPH-specific metrics
    try {
      const metricsRegistry = globalContext.container.get(Identifiers.MetricsRegistry);
      const nodeCollector = metricsRegistry?.getNodeCollector();
      if (nodeCollector && typeof nodeCollector.recordSubgraphExecution === 'function') {
        const hierarchyMetadata = subgraphEntity.getHierarchyMetadata();
        nodeCollector.recordSubgraphExecution(
          node.id,
          workflowExecutionEntity.getWorkflowId(),
          {
            success: subgraphEntity.getStatus() === 'COMPLETED',
            duration: executionDuration,
            subworkflowId,
            depth: hierarchyMetadata?.depth || 1,
            variableInputCount: variableMapping.inputs?.length || 0,
            variableOutputCount: variableMapping.outputs?.length || 0,
            errorType: undefined,
          }
        );
      }
    } catch (metricsError) {
      // Don't fail the execution if metrics recording fails
      logger.warn("Failed to record subgraph metrics", { error: getErrorOrNew(metricsError) });
    }

    // Step 6: Export variables back to parent
    if (variableMapping.outputs && variableMapping.outputs.length > 0) {
      workflowExecutionEntity.variableStateManager.exportVariables(
        subgraphEntity.variableStateManager,
        variableMapping.outputs
      );
      logger.debug("Exported variables from subgraph to parent", {
        count: variableMapping.outputs.length,
      });
    }

    // Step 7: Handle message context exiting (via subgraph-handler)
    if (staticNode && staticNode.type === 'SUBGRAPH') {
      await exitSubgraph(workflowExecutionEntity, staticNode);
    }

    // Step 8: Return standardized result
    return createSubgraphResult(subgraphEntity, executionResult, executionDuration);
  } catch (error) {
    const errorObj = getErrorOrNew(error);
    const executionDuration = Date.now() - executionStartTime;
    
    logger.error("Subgraph execution failed", {
      executionId: workflowExecutionEntity.id,
      nodeId: node.id,
      subworkflowId,
      subgraphExecutionId: subgraphEntity?.id,
      error: errorObj,
    });

    // Record failure metrics
    try {
      const metricsRegistry = globalContext.container.get(Identifiers.MetricsRegistry);
      const nodeCollector = metricsRegistry?.getNodeCollector();
      if (nodeCollector && typeof nodeCollector.recordSubgraphExecution === 'function') {
        const hierarchyMetadata = subgraphEntity?.getHierarchyMetadata();
        nodeCollector.recordSubgraphExecution(
          node.id,
          workflowExecutionEntity.getWorkflowId(),
          {
            success: false,
            duration: executionDuration,
            subworkflowId,
            depth: hierarchyMetadata?.depth || 1,
            variableInputCount: variableMapping.inputs?.length || 0,
            variableOutputCount: variableMapping.outputs?.length || 0,
            errorType: errorObj.name || 'Error',
          }
        );
      }
    } catch (metricsError) {
      // Don't fail the execution if metrics recording fails
      logger.warn("Failed to record subgraph failure metrics", { error: getErrorOrNew(metricsError) });
    }

    // Cleanup resources on failure
    if (subgraphEntity) {
      try {
        logger.debug("Cleaning up failed subgraph execution", {
          subgraphExecutionId: subgraphEntity.id,
        });

        const registry = globalContext.container.get(
          Identifiers.ExecutionHierarchyRegistry
        ) as any;
        
        await cleanupFailedSubworkflow(
          subgraphEntity,
          workflowExecutionEntity,
          registry
        );
      } catch (cleanupError) {
        logger.warn("Failed to cleanup subgraph after error", {
          subgraphExecutionId: subgraphEntity.id,
          cleanupError: getErrorOrNew(cleanupError),
        });
      }
    }

    // Re-throw with more context
    throw new Error(
      `Subgraph execution failed for node '${node.id}': ${errorObj.message}`,
      { cause: errorObj }
    );
  }
}


