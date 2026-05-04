/**
 * Execution Hierarchy Registry
 * 
 * Global registry for managing all execution instances and their hierarchical relationships.
 * Provides cross-type query and cleanup capabilities for the entire execution hierarchy tree.
 * 
 * This registry works in conjunction with ExecutionHierarchyManager to provide:
 * - Centralized management of all execution instances (Workflow and Agent)
 * - Recursive traversal of hierarchy trees
 * - Bulk operations (cleanup, query by root, etc.)
 * - Support for mixed hierarchy scenarios (Workflow → Agent → Agent, etc.)
 */

import type { ID, ExecutionType } from '@wf-agent/types';
import type { WorkflowExecutionEntity } from '../../workflow/entities/workflow-execution-entity.js';
import type { AgentLoopEntity } from '../../agent/entities/agent-loop-entity.js';
import type { ChildExecutionReference } from '@wf-agent/types';

/**
 * Union type for any execution entity
 */
export type AnyExecutionEntity = WorkflowExecutionEntity | AgentLoopEntity;

/**
 * Result of querying executions by root
 */
export interface ExecutionsByRoot {
  workflows: WorkflowExecutionEntity[];
  agents: AgentLoopEntity[];
}

/**
 * Execution Hierarchy Registry
 * 
 * Manages all execution instances globally and provides hierarchy-aware operations.
 * 
 * Key Features:
 * - Register/unregister execution instances
 * - Query descendants recursively
 * - Clean up entire hierarchy trees
 * - Group executions by root
 * 
 * Usage:
 * ```typescript
 * const registry = new ExecutionHierarchyRegistry();
 * registry.register(workflow);
 * registry.register(agent);
 * 
 * // Get all descendants
 * const descendants = registry.getAllDescendants(workflow.id);
 * 
 * // Cleanup entire hierarchy
 * registry.cleanupHierarchy(workflow.id);
 * ```
 */
export class ExecutionHierarchyRegistry {
  private executions: Map<ID, AnyExecutionEntity> = new Map();

  /**
   * Registers an execution instance
   * 
   * @param execution - The execution entity to register (Workflow or Agent)
   */
  register(execution: AnyExecutionEntity): void {
    this.executions.set(execution.id, execution);
  }

  /**
   * Unregisters an execution instance
   * 
   * @param executionId - The ID of the execution to unregister
   * @returns true if the execution was found and removed, false otherwise
   */
  unregister(executionId: ID): boolean {
    return this.executions.delete(executionId);
  }

  /**
   * Gets an execution instance by ID
   * 
   * @param executionId - The ID of the execution to retrieve
   * @returns The execution entity, or undefined if not found
   */
  get(executionId: ID): AnyExecutionEntity | undefined {
    return this.executions.get(executionId);
  }

  /**
   * Checks if an execution instance exists
   * 
   * @param executionId - The ID to check
   * @returns true if the execution exists, false otherwise
   */
  has(executionId: ID): boolean {
    return this.executions.has(executionId);
  }

  /**
   * Gets all registered execution instances
   * 
   * @returns Array of all execution entities
   */
  getAll(): AnyExecutionEntity[] {
    return Array.from(this.executions.values());
  }

  /**
   * Gets all registered execution IDs
   * 
   * @returns Array of all execution IDs
   */
  getAllIds(): ID[] {
    return Array.from(this.executions.keys());
  }

  /**
   * Gets the number of registered executions
   * 
   * @returns The count of registered executions
   */
  size(): number {
    return this.executions.size;
  }

  /**
   * Clears all registered executions
   */
  clear(): void {
    this.executions.clear();
  }

  /**
   * Gets all descendant executions of a given execution (recursive)
   * 
   * Traverses the entire hierarchy tree starting from the given execution.
   * 
   * @param executionId - The ID of the parent execution
   * @param includeSelf - Whether to include the parent execution itself in the result
   * @returns Array of all descendant execution entities
   * 
   * @example
   * ```typescript
   * // Get all descendants including the root
   * const allExecutions = registry.getAllDescendants('workflow-1', true);
   * 
   * // Get only children and deeper descendants
   * const descendants = registry.getAllDescendants('workflow-1', false);
   * ```
   */
  getAllDescendants(executionId: ID, includeSelf: boolean = false): AnyExecutionEntity[] {
    const result: AnyExecutionEntity[] = [];

    if (includeSelf) {
      const self = this.get(executionId);
      if (self) {
        result.push(self);
      }
    }

    const entity = this.get(executionId);
    if (!entity) {
      return result;
    }

    // Get direct children
    const children = this.getDirectChildren(executionId);

    // Recursively get descendants of each child
    for (const child of children) {
      result.push(child);
      result.push(...this.getAllDescendants(child.id, false));
    }

    return result;
  }

  /**
   * Gets direct children of a given execution
   * 
   * Only returns immediate children, not deeper descendants.
   * 
   * @param executionId - The ID of the parent execution
   * @returns Array of direct child execution entities
   */
  getDirectChildren(executionId: ID): AnyExecutionEntity[] {
    const parent = this.get(executionId);
    if (!parent) {
      return [];
    }

    const children: AnyExecutionEntity[] = [];

    // Get child references from the parent's hierarchy manager
    if ('getChildren' in parent && typeof parent.getChildren === 'function') {
      const childRefs: ChildExecutionReference[] = parent.getChildren();
      
      for (const ref of childRefs) {
        const child = this.get(ref.childId);
        if (child) {
          children.push(child);
        }
      }
    }

    return children;
  }

  /**
   * Cleans up an execution and all its descendants
   * 
   * Performs the following for each execution in the hierarchy:
   * 1. Stops the execution if it's running
   * 2. Calls cleanup() method if available
   * 3. Removes from registry
   * 
   * @param executionId - The ID of the root execution to clean up
   * @returns The number of executions cleaned up
   * 
   * @example
   * ```typescript
   * // Clean up workflow and all its children (agents, sub-workflows, etc.)
   * const cleanedCount = registry.cleanupHierarchy('workflow-1');
   * console.log(`Cleaned ${cleanedCount} executions`);
   * ```
   */
  cleanupHierarchy(executionId: ID): number {
    const descendants = this.getAllDescendants(executionId, true);
    let count = 0;

    for (const descendant of descendants) {
      // Stop execution if running
      if ('stop' in descendant && typeof descendant.stop === 'function') {
        try {
          descendant.stop();
        } catch (error) {
          // Log error but continue cleanup
          console.warn(`Failed to stop execution ${descendant.id}:`, error);
        }
      }

      // Cleanup resources
      if ('cleanup' in descendant && typeof descendant.cleanup === 'function') {
        try {
          descendant.cleanup();
        } catch (error) {
          // Log error but continue cleanup
          console.warn(`Failed to cleanup execution ${descendant.id}:`, error);
        }
      }

      // Remove from registry
      this.unregister(descendant.id);
      count++;
    }

    return count;
  }

  /**
   * Gets all executions under a given root execution, grouped by type
   * 
   * @param rootExecutionId - The ID of the root execution
   * @returns Object containing arrays of workflows and agents
   * 
   * @example
   * ```typescript
   * const { workflows, agents } = registry.getExecutionsByRoot('root-workflow');
   * console.log(`Found ${workflows.length} workflows and ${agents.length} agents`);
   * ```
   */
  getExecutionsByRoot(rootExecutionId: ID): ExecutionsByRoot {
    const allDescendants = this.getAllDescendants(rootExecutionId, true);

    return {
      workflows: allDescendants.filter(
        (e): e is WorkflowExecutionEntity => 'getWorkflowId' in e && typeof e.getWorkflowId === 'function' && !!e.getWorkflowId()
      ),
      agents: allDescendants.filter(
        (e): e is AgentLoopEntity => 'conversationManager' in e && !!e.conversationManager
      ),
    };
  }

  /**
   * Gets all root executions (executions without parents)
   * 
   * @returns Array of root execution entities
   */
  getRootExecutions(): AnyExecutionEntity[] {
    return this.getAll().filter(entity => {
      if ('getParentContext' in entity && typeof entity.getParentContext === 'function') {
        const parent = entity.getParentContext();
        return !parent;
      }
      return false;
    });
  }

  /**
   * Gets all executions that have a specific parent
   * 
   * @param parentId - The ID of the parent execution
   * @returns Array of child execution entities
   */
  getChildrenOf(parentId: ID): AnyExecutionEntity[] {
    return this.getAll().filter(entity => {
      if ('getParentContext' in entity && typeof entity.getParentContext === 'function') {
        const parent = entity.getParentContext();
        return parent?.parentId === parentId;
      }
      return false;
    });
  }

  /**
   * Gets all executions of a specific type
   * 
   * @param type - The execution type to filter by
   * @returns Array of execution entities of the specified type
   */
  getByType(type: ExecutionType): AnyExecutionEntity[] {
    return this.getAll().filter(entity => {
      if (type === 'WORKFLOW') {
        // Check if getWorkflowId returns a truthy value (not undefined)
        return 'getWorkflowId' in entity && typeof entity.getWorkflowId === 'function' && !!entity.getWorkflowId();
      } else {
        // Check if conversationManager is defined
        return 'conversationManager' in entity && !!entity.conversationManager;
      }
    });
  }

  /**
   * Checks if an execution is part of a hierarchy tree rooted at the given ID
   * 
   * @param executionId - The ID of the execution to check
   * @param rootExecutionId - The ID of the potential root
   * @returns true if the execution is in the hierarchy tree
   */
  isInHierarchy(executionId: ID, rootExecutionId: ID): boolean {
    if (executionId === rootExecutionId) {
      return true;
    }

    const entity = this.get(executionId);
    if (!entity) {
      return false;
    }

    // Traverse up the parent chain
    let current: AnyExecutionEntity | undefined = entity;
    while (current) {
      if ('getParentContext' in current && typeof current.getParentContext === 'function') {
        const parent = current.getParentContext();
        if (!parent) {
          // Reached root without finding target
          return false;
        }
        if (parent.parentId === rootExecutionId) {
          return true;
        }
        current = this.get(parent.parentId);
      } else {
        return false;
      }
    }

    return false;
  }
}
