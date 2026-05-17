/**
 * Subworkflow Execution Result Types
 * 
 * Standardized result interfaces for different subworkflow execution types:
 * - SUBGRAPH (synchronous child workflows)
 * - FORK_BRANCH (parallel fork branches)
 * - TRIGGERED_SUBWORKFLOW (asynchronously triggered subworkflows)
 * 
 * These interfaces provide consistent access to execution results across all subworkflow types.
 */

import type { WorkflowExecutionEntity } from "../../entities/workflow-execution-entity.js";
import type { WorkflowExecutionResult } from "@wf-agent/types";
import type { ExecutedSubworkflowResult } from "./triggered-subworkflow.types.js";

/**
 * SUBGRAPH Execution Result
 * 
 * Represents the result of executing a SUBGRAPH node (synchronous child workflow).
 * SUBGRAPH nodes execute synchronously within the parent workflow's execution flow.
 * 
 * @example
 * ```typescript
 * const result: SubgraphExecutionResult = await executeSubgraph(...);
 * console.log(result.output); // Access output variables
 * console.log(result.executionTime); // Check execution duration
 * ```
 */
export interface SubgraphExecutionResult {
  /** 
   * Subgraph execution entity - provides access to internal state, variables, and metadata.
   * Use this for debugging or when you need to inspect the execution context.
   * For normal usage, prefer `executionResult.output` to get the final output.
   */
  subgraphEntity: WorkflowExecutionEntity;
  
  /** 
   * Execution result containing output data, status, and other metadata.
   * This is the primary source for retrieving the subgraph's output.
   */
  executionResult: WorkflowExecutionResult;
  
  /** Execution time in milliseconds */
  executionTime: number;
}

/**
 * Fork Branch Execution Result
 * 
 * Represents the result of executing a single branch in a FORK node.
 * Each fork branch executes independently with isolated variable state.
 * 
 * @example
 * ```typescript
 * const results: ForkBranchResult[] = await executeForkBranches(...);
 * for (const result of results) {
 *   console.log(`Branch ${result.forkPathId}:`, result.output);
 * }
 * ```
 */
export interface ForkBranchResult {
  /** 
   * Fork branch ID - identifies which branch this result belongs to.
   * Corresponds to the path ID in the FORK node configuration.
   */
  forkPathId: string;
  
  /** 
   * Fork branch execution entity - provides access to internal state, variables, and metadata.
   * Each branch has its own isolated execution context.
   */
  branchEntity: WorkflowExecutionEntity;
  
  /** 
   * Execution result containing output data, status, and other metadata.
   * This is the primary source for retrieving the branch's output.
   */
  executionResult: WorkflowExecutionResult;
  
  /** Execution time in milliseconds */
  executionTime: number;
}

/**
 * Common Subworkflow Execution Result (Union Type)
 * 
 * A discriminated union that can represent any type of subworkflow execution result.
 * Use the `executionType` field to narrow down the specific type.
 * 
 * @example
 * ```typescript
 * function handleSubworkflowResult(result: SubWorkflowExecutionResult) {
 *   switch (result.executionType) {
 *     case 'SUBGRAPH':
 *       console.log('Subgraph output:', result.subgraphEntity.getOutput());
 *       break;
 *     case 'FORK_BRANCH':
 *       console.log(`Fork branch ${result.forkPathId} output:`, result.branchEntity.getOutput());
 *       break;
 *     case 'TRIGGERED_SUBWORKFLOW':
 *       console.log('Triggered subworkflow output:', result.subworkflowEntity.getOutput());
 *       break;
 *   }
 * }
 * ```
 */
export type SubWorkflowExecutionResult =
  | {
      /** Execution type: SUBGRAPH (synchronous child workflow) */
      executionType: 'SUBGRAPH';
      /** Subgraph execution entity */
      subgraphEntity: WorkflowExecutionEntity;
      /** Execution result */
      executionResult: WorkflowExecutionResult;
      /** Execution time in milliseconds */
      executionTime: number;
    }
  | {
      /** Execution type: FORK_BRANCH (parallel fork branch) */
      executionType: 'FORK_BRANCH';
      /** Fork branch ID */
      forkPathId: string;
      /** Fork branch execution entity */
      branchEntity: WorkflowExecutionEntity;
      /** Execution result */
      executionResult: WorkflowExecutionResult;
      /** Execution time in milliseconds */
      executionTime: number;
    }
  | {
      /** Execution type: TRIGGERED_SUBWORKFLOW (asynchronously triggered) */
      executionType: 'TRIGGERED_SUBWORKFLOW';
      /** Trigger ID that initiated this subworkflow */
      triggerId: string;
      /** Subworkflow execution entity */
      subworkflowEntity: WorkflowExecutionEntity;
      /** Execution result */
      executionResult: WorkflowExecutionResult;
      /** Execution time in milliseconds */
      executionTime: number;
    };

/**
 * Helper function to create a SUBGRAPH execution result
 */
export function createSubgraphResult(
  subgraphEntity: WorkflowExecutionEntity,
  executionResult: WorkflowExecutionResult,
  executionTime: number,
): SubgraphExecutionResult {
  return {
    subgraphEntity,
    executionResult,
    executionTime,
  };
}

/**
 * Helper function to create a Fork Branch execution result
 */
export function createForkBranchResult(
  forkPathId: string,
  branchEntity: WorkflowExecutionEntity,
  executionResult: WorkflowExecutionResult,
  executionTime: number,
): ForkBranchResult {
  return {
    forkPathId,
    branchEntity,
    executionResult,
    executionTime,
  };
}

/**
 * Helper function to convert specific result type to common SubWorkflowExecutionResult
 */
export function toSubWorkflowResult(result: SubgraphExecutionResult): SubWorkflowExecutionResult;
export function toSubWorkflowResult(result: ForkBranchResult): SubWorkflowExecutionResult;
export function toSubWorkflowResult(result: ExecutedSubworkflowResult & { triggerId: string }): SubWorkflowExecutionResult;
export function toSubWorkflowResult(
  result: SubgraphExecutionResult | ForkBranchResult | (ExecutedSubworkflowResult & { triggerId: string }),
): SubWorkflowExecutionResult {
  if ('subgraphEntity' in result && !('forkPathId' in result)) {
    // SubgraphExecutionResult
    const subgraphResult = result as SubgraphExecutionResult;
    return {
      executionType: 'SUBGRAPH',
      subgraphEntity: subgraphResult.subgraphEntity,
      executionResult: subgraphResult.executionResult,
      executionTime: subgraphResult.executionTime,
    };
  } else if ('forkPathId' in result) {
    // ForkBranchResult
    const forkResult = result as ForkBranchResult;
    return {
      executionType: 'FORK_BRANCH',
      forkPathId: forkResult.forkPathId,
      branchEntity: forkResult.branchEntity,
      executionResult: forkResult.executionResult,
      executionTime: forkResult.executionTime,
    };
  } else {
    // ExecutedSubworkflowResult with triggerId
    const triggeredResult = result as ExecutedSubworkflowResult & { triggerId: string };
    return {
      executionType: 'TRIGGERED_SUBWORKFLOW',
      triggerId: triggeredResult.triggerId,
      subworkflowEntity: triggeredResult.subworkflowEntity,
      executionResult: triggeredResult.executionResult,
      executionTime: triggeredResult.executionTime,
    };
  }
}
