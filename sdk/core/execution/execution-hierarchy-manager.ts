/**
 * Execution Hierarchy Manager
 * 
 * Unified management of parent-child relationships for all execution instances.
 * Encapsulates all operations related to hierarchy management including:
 * - Setting parent context
 * - Registering/unregistering children
 * - Calculating depth and root execution
 * - Cycle detection
 * 
 * This manager is used by both WorkflowExecutionEntity and AgentLoopEntity
 * to provide a consistent API for hierarchy management.
 */

import type {
  ExecutionType,
  ParentExecutionContext,
  ChildExecutionReference,
  ExecutionHierarchyMetadata,
  ID,
} from '@wf-agent/types';

/**
 * Maximum allowed hierarchy depth to prevent infinite nesting
 * Can be configured via environment variable or configuration file
 */
const MAX_DEPTH = parseInt(process.env['MAX_EXECUTION_DEPTH'] || '10', 10);

/**
 * Execution Hierarchy Manager
 * 
 * Manages the hierarchical relationship of a single execution instance.
 * Each entity (Workflow or Agent) has its own HierarchyManager instance.
 */
export class ExecutionHierarchyManager {
  private executionId: ID;
  private executionType: ExecutionType;
  private parent?: ParentExecutionContext;
  private children: Map<string, ChildExecutionReference> = new Map();
  private depth: number = 0;
  private rootExecutionId: ID;
  private rootExecutionType: ExecutionType;

  /**
   * Creates a new ExecutionHierarchyManager
   * 
   * @param executionId - The ID of this execution instance
   * @param executionType - The type of this execution (WORKFLOW or AGENT_LOOP)
   * @param existingHierarchy - Optional existing hierarchy metadata to restore from
   */
  constructor(
    executionId: ID,
    executionType: ExecutionType,
    existingHierarchy?: ExecutionHierarchyMetadata
  ) {
    this.executionId = executionId;
    this.executionType = executionType;

    // Restore state from existing hierarchy metadata if provided
    if (existingHierarchy) {
      this.parent = existingHierarchy.parent;
      this.depth = existingHierarchy.depth;
      this.rootExecutionId = existingHierarchy.rootExecutionId;
      this.rootExecutionType = existingHierarchy.rootExecutionType;

      // Restore child references
      for (const child of existingHierarchy.children) {
        const key = `${child.childType}:${child.childId}`;
        this.children.set(key, child);
      }
    } else {
      // New instance: this is the root node
      this.rootExecutionId = executionId;
      this.rootExecutionType = executionType;
    }
  }

  /**
   * Sets the parent execution context
   * 
   * Performs validation including:
   * - Cycle detection
   * - Depth limit enforcement
   * 
   * @param parentContext - The parent execution context
   * @throws Error if cycle detected or depth limit exceeded
   */
  setParent(parentContext: ParentExecutionContext): void {
    // Validate before setting
    this.validateParentChange(parentContext);

    this.parent = parentContext;

    // Recalculate depth and root based on parent
    this.recalculateHierarchy();
  }

  /**
   * Gets the current parent execution context
   * 
   * @returns The parent context, or undefined if this is a root execution
   */
  getParent(): ParentExecutionContext | undefined {
    return this.parent;
  }

  /**
   * Adds a child execution reference
   * 
   * @param childRef - The child execution reference to add
   */
  addChild(childRef: ChildExecutionReference): void {
    const key = `${childRef.childType}:${childRef.childId}`;
    this.children.set(key, childRef);
  }

  /**
   * Removes a child execution reference
   * 
   * @param childId - The ID of the child to remove
   * @param childType - The type of the child to remove
   * @returns true if the child was found and removed, false otherwise
   */
  removeChild(childId: ID, childType: ExecutionType): boolean {
    const key = `${childType}:${childId}`;
    return this.children.delete(key);
  }

  /**
   * Gets all child execution references
   * 
   * @returns Array of all child references
   */
  getChildren(): ChildExecutionReference[] {
    return Array.from(this.children.values());
  }

  /**
   * Gets the current hierarchy depth
   * 
   * @returns The depth (0 for root nodes)
   */
  getDepth(): number {
    return this.depth;
  }

  /**
   * Gets the root execution ID
   * 
   * @returns The ID of the root execution in this hierarchy tree
   */
  getRootExecutionId(): ID {
    return this.rootExecutionId;
  }

  /**
   * Gets the root execution type
   * 
   * @returns The type of the root execution
   */
  getRootExecutionType(): ExecutionType {
    return this.rootExecutionType;
  }

  /**
   * Converts the current state to serializable metadata
   * 
   * @returns ExecutionHierarchyMetadata suitable for serialization
   */
  toMetadata(): ExecutionHierarchyMetadata {
    return {
      parent: this.parent,
      children: this.getChildren(),
      depth: this.depth,
      rootExecutionId: this.rootExecutionId,
      rootExecutionType: this.rootExecutionType,
    };
  }

  /**
   * Validates a parent context change
   * 
   * Checks for:
   * - Circular references
   * - Depth limit violations
   * 
   * @param parentContext - The proposed parent context
   * @throws Error if validation fails
   */
  private validateParentChange(parentContext: ParentExecutionContext): void {
    // Check for circular reference
    if (this.wouldCreateCycle(parentContext.parentId)) {
      throw new Error(
        `Circular reference detected: cannot set ${parentContext.parentId} as parent of ${this.executionId}`
      );
    }

    // Calculate what the new depth would be
    const parentDepth = this.getParentDepth(parentContext);
    const newDepth = parentDepth + 1;

    if (newDepth > MAX_DEPTH) {
      throw new Error(
        `Maximum hierarchy depth exceeded: ${newDepth} > ${MAX_DEPTH}. ` +
          `Consider restructuring your execution hierarchy.`
      );
    }
  }

  /**
   * Checks if setting the given parent would create a circular reference
   * 
   * Traverses up the parent chain to see if we encounter our own ID
   * 
   * @param targetParentId - The ID of the proposed parent
   * @returns true if a cycle would be created
   */
  private wouldCreateCycle(targetParentId: ID): boolean {
    // Simple case: can't be parent of self
    if (targetParentId === this.executionId) {
      return true;
    }

    // For now, we only check direct self-reference
    // In a full implementation, we would need access to a registry
    // to traverse the entire parent chain
    // TODO: Implement full cycle detection with registry access
    return false;
  }

  /**
   * Gets the depth of a parent execution
   * 
   * In a full implementation, this would query the registry
   * For now, we use a simplified approach
   * 
   * @param parentContext - The parent context
   * @returns The depth of the parent execution
   */
  private getParentDepth(parentContext: ParentExecutionContext): number {
    // Simplified implementation
    // In production, this should query the ExecutionHierarchyRegistry
    // to get the actual parent's depth
    
    // For root parents (no parent themselves), depth is 0
    // For non-root parents, we'd need to look them up
    return 0; // Temporary implementation
  }

  /**
   * Recalculates hierarchy information after parent change
   * 
   * Updates:
   * - depth: parent's depth + 1
   * - rootExecutionId: inherited from parent's root
   * - rootExecutionType: inherited from parent's root
   */
  private recalculateHierarchy(): void {
    if (!this.parent) {
      // No parent: this is the root
      this.depth = 0;
      this.rootExecutionId = this.executionId;
      this.rootExecutionType = this.executionType;
    } else {
      // Has parent: calculate based on parent's hierarchy
      const parentDepth = this.getParentDepth(this.parent);
      this.depth = parentDepth + 1;
      
      // Root information comes from parent's root
      // TODO: In full implementation, query registry for parent's root info
      this.rootExecutionId = this.parent.parentId; // Simplified
      this.rootExecutionType = this.parent.parentType; // Simplified
    }
  }
}
