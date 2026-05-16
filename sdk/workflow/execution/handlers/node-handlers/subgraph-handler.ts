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

const logger = createContextualLogger({ component: "subgraph-node-handler" });

/**
 * SUBGRAPH Node Handler Context
 */
export interface SubgraphHandlerContext {
  // Context will be provided by NodeHandlerContextFactory
}

/**
 * Execute SUBGRAPH node - Create and execute independent child workflow
 * 
 * NOTE: SUBGRAPH only supports synchronous execution.
 * For asynchronous/parallel execution, use FORK nodes instead.
 * 
 * @param globalContext Global application context
 * @param workflowExecutionEntity Parent workflow execution entity
 * @param node SUBGRAPH runtime node
 * @param context Handler context (optional)
 * @returns Execution result
 */
export async function subgraphHandler(
  globalContext: GlobalContext,
  workflowExecutionEntity: WorkflowExecutionEntity,
  node: RuntimeNode,
  _context?: SubgraphHandlerContext,
): Promise<unknown> {
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

  // Step 1: Get WorkflowExecutionBuilder from DI container
  const executionBuilder = globalContext.container.get(
    Identifiers.WorkflowExecutionBuilder
  ) as WorkflowExecutionBuilder;

  if (!executionBuilder) {
    throw new Error("WorkflowExecutionBuilder not available in DI container");
  }

  // Step 2: Build variable mappings from node config
  const variableMapping = {
    inputs: config.variableInputs || [],
    outputs: config.variableOutputs || [],
  };

  // Step 3: Create independent subgraph execution entity
  const buildResult = await executionBuilder.createSubgraph(workflowExecutionEntity, {
    subworkflowId,
    nodeId: node.id,
    variableMapping,
    async: false, // SUBGRAPH always executes synchronously
  });

  const subgraphEntity = buildResult.workflowExecutionEntity;

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
  const result = await executeSync(globalContext, subgraphEntity);
  logger.info("Subgraph completed", {
    subgraphExecutionId: subgraphEntity.id,
  });

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

  return result;
}

/**
 * Execute subgraph synchronously
 * @param globalContext Global application context
 * @param subgraphEntity Subgraph execution entity
 * @returns Execution result
 */
async function executeSync(
  globalContext: GlobalContext,
  subgraphEntity: WorkflowExecutionEntity,
): Promise<unknown> {
  // Get WorkflowExecutor from DI container
  const executor = globalContext.container.get(
    Identifiers.WorkflowExecutor
  ) as WorkflowExecutor;

  if (!executor) {
    throw new Error("WorkflowExecutor not available in DI container");
  }

  // Execute the subgraph workflow
  const executionResult = await executor.executeWorkflow(subgraphEntity);

  return {
    executionId: subgraphEntity.id,
    output: subgraphEntity.getOutput(),
    status: subgraphEntity.getStatus(),
    executionResult,
  };
}
