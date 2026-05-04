/**
 * Execution Hierarchy Type Definitions
 * 
 * Unified parent-child relationship management using TypeScript Union Types.
 * Supports tracking hierarchical relationships between all execution instances
 * (Workflow and Agent Loop) in a type-safe manner.
 * 
 * ## Design Goals
 * 
 * - Type safety: Compile-time validation of parent/child types
 * - Unified API: Consistent interface across all execution types
 * - Extensibility: Easy to add new execution types (e.g., Tool Execution)
 * - Complete tracing: Support arbitrary depth hierarchy trees
 * 
 * ## Supported Scenarios
 * 
 * | Scenario | Parent Type | Child Type |
 * |----------|-------------|------------|
 * | Workflow → Workflow | WorkflowExecution | WorkflowExecution |
 * | Workflow → Agent | WorkflowExecution | AgentLoopExecution |
 * | Agent → Agent | AgentLoopExecution | AgentLoopExecution |
 * | Agent → Workflow | AgentLoopExecution | WorkflowExecution |
 */

import type { ID, Timestamp } from "../common.js";

/**
 * Execution Instance Type
 * 
 * Represents the type of an execution instance in the hierarchy.
 * Can be extended in the future to support additional types like 'TOOL_EXECUTION'.
 */
export type ExecutionType = 'WORKFLOW' | 'AGENT_LOOP';

/**
 * Execution Identity (with type label)
 * 
 * Uniquely identifies an execution instance with its type.
 */
export interface ExecutionIdentity {
  /** Execution type */
  type: ExecutionType;
  /** Execution ID */
  id: ID;
}

/**
 * Parent Execution Context (Union Type)
 * 
 * Uses discriminated union to ensure type safety when referencing parent executions.
 * The `parentType` field acts as the discriminant.
 * 
 * @example
 * ```typescript
 * // Workflow parent
 * const workflowParent: ParentExecutionContext = {
 *   parentType: 'WORKFLOW',
 *   parentId: 'workflow-123',
 *   nodeId: 'agent-node-1',
 * };
 * 
 * // Agent parent
 * const agentParent: ParentExecutionContext = {
 *   parentType: 'AGENT_LOOP',
 *   parentId: 'agent-456',
 *   delegationPurpose: 'Code review task delegation',
 * };
 * ```
 */
export type ParentExecutionContext =
  | {
      /** Parent is a Workflow execution */
      parentType: 'WORKFLOW';
      /** Parent workflow execution ID */
      parentId: ID;
      /** Node ID in parent workflow (optional, for AGENT_LOOP nodes) */
      nodeId?: ID;
    }
  | {
      /** Parent is an Agent Loop execution */
      parentType: 'AGENT_LOOP';
      /** Parent agent loop execution ID */
      parentId: ID;
      /** Delegation purpose/reason (optional, for documentation) */
      delegationPurpose?: string;
    };

/**
 * Child Execution Reference (Union Type)
 * 
 * Used by parent executions to track all child executions.
 * Includes creation timestamp for ordering and lifecycle management.
 * 
 * @example
 * ```typescript
 * // Workflow child reference
 * const workflowChild: ChildExecutionReference = {
 *   childType: 'WORKFLOW',
 *   childId: 'sub-workflow-789',
 *   createdAt: Date.now(),
 * };
 * 
 * // Agent child reference
 * const agentChild: ChildExecutionReference = {
 *   childType: 'AGENT_LOOP',
 *   childId: 'sub-agent-012',
 *   createdAt: Date.now(),
 * };
 * ```
 */
export type ChildExecutionReference =
  | {
      /** Child is a Workflow execution */
      childType: 'WORKFLOW';
      /** Child workflow execution ID */
      childId: ID;
      /** Creation timestamp */
      createdAt: Timestamp;
    }
  | {
      /** Child is an Agent Loop execution */
      childType: 'AGENT_LOOP';
      /** Child agent loop execution ID */
      childId: ID;
      /** Creation timestamp */
      createdAt: Timestamp;
    };

/**
 * Execution Hierarchy Metadata
 * 
 * Attached to each execution instance to describe its position in the hierarchy tree.
 * This metadata enables complete lineage tracking from root to leaf nodes.
 * 
 * @example
 * ```typescript
 * const metadata: ExecutionHierarchyMetadata = {
 *   parent: {
 *     parentType: 'WORKFLOW',
 *     parentId: 'root-workflow',
 *   },
 *   children: [
 *     { childType: 'AGENT_LOOP', childId: 'agent-1', createdAt: 1234567890 },
 *   ],
 *   depth: 1,
 *   rootExecutionId: 'root-workflow',
 *   rootExecutionType: 'WORKFLOW',
 * };
 * ```
 */
export interface ExecutionHierarchyMetadata {
  /** Parent execution context (if exists) */
  parent?: ParentExecutionContext;
  
  /** List of child execution references */
  children: ChildExecutionReference[];
  
  /** Hierarchy depth (root node is 0) */
  depth: number;
  
  /** Root execution ID (the root of the hierarchy tree) */
  rootExecutionId: ID;
  
  /** Root execution type */
  rootExecutionType: ExecutionType;
}
