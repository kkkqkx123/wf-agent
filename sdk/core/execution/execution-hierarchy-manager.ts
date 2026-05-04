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
import { sdkLogger as logger } from '../../utils/logger.js';
import type { ExecutionHierarchyRegistry } from '../registry/execution-hierarchy-registry.js';

/**
 * Maximum allowed hierarchy depth to prevent infinite nesting
 * Can be configured via environment variable or configuration file
 */
const MAX_DEPTH = parseInt(process.env['MAX_EXECUTION_DEPTH'] || '10', 10);

/**
 * Maximum number of ancestors to traverse during cycle detection
 * This prevents excessive traversal in extremely deep hierarchies
 * Should be >= MAX_DEPTH for correctness
 */
const MAX_CYCLE_CHECK_DEPTH = MAX_DEPTH + 5;

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
  private registry?: ExecutionHierarchyRegistry;

  /**
   * Creates a new ExecutionHierarchyManager
   * 
   * @param executionId - The ID of this execution instance
   * @param executionType - The type of this execution (WORKFLOW or AGENT_LOOP)
   * @param existingHierarchy - Optional existing hierarchy metadata to restore from
   * @param registry - Optional registry reference for cycle detection and depth calculation
   */
  constructor(
    executionId: ID,
    executionType: ExecutionType,
    existingHierarchy?: ExecutionHierarchyMetadata,
    registry?: ExecutionHierarchyRegistry
  ) {
    this.executionId = executionId;
    this.executionType = executionType;
    this.registry = registry;

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

    // Recalculate depth and root based on parent (cached values updated)
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
   * Returns cached value (O(1) operation).
   * Cache is automatically invalidated and recalculated when parent changes.
   * 
   * @returns The depth (0 for root nodes)
   */
  getDepth(): number {
    return this.depth;
  }

  /**
   * Gets the root execution ID
   * 
   * Returns cached value (O(1) operation).
   * Cache is automatically invalidated and recalculated when parent changes.
   * 
   * @returns The ID of the root execution in this hierarchy tree
   */
  getRootExecutionId(): ID {
    return this.rootExecutionId;
  }

  /**
   * Gets the root execution type
   * 
   * Returns cached value (O(1) operation).
   * Cache is automatically invalidated and recalculated when parent changes.
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
   * Traverses up the parent chain to see if we encounter our own ID.
   * Uses the registry to access parent entities and traverse the full ancestor chain.
   * Optimized with early termination and depth limits to prevent excessive traversal.
   * 
   * @param targetParentId - The ID of the proposed parent
   * @returns true if a cycle would be created
   */
  private wouldCreateCycle(targetParentId: ID): boolean {
    // Simple case: can't be parent of self
    if (targetParentId === this.executionId) {
      return true;
    }

    // If registry is not available, we can only check direct self-reference
    if (!this.registry) {
      logger.warn('Registry not available, skipping full cycle detection');
      return false;
    }

    // Traverse the ancestor chain to detect cycles
    let currentId: ID | undefined = targetParentId;
    const visited = new Set<ID>();
    let traversalDepth = 0;
    
    while (currentId) {
      // Safety check: prevent excessive traversal
      if (traversalDepth > MAX_CYCLE_CHECK_DEPTH) {
        logger.warn('Cycle detection exceeded maximum traversal depth', {
          targetParentId,
          maxDepth: MAX_CYCLE_CHECK_DEPTH,
        });
        // Assume no cycle to avoid blocking legitimate deep hierarchies
        // The depth limit validation will catch excessively deep trees
        return false;
      }
      
      // Check if we've reached ourselves (cycle detected)
      if (currentId === this.executionId) {
        return true;
      }
      
      // Check if we've already visited this node (cycle in parent chain)
      if (visited.has(currentId)) {
        logger.warn('Cycle detected in parent chain during traversal', { currentId });
        return true;
      }
      
      visited.add(currentId);
      traversalDepth++;
      
      // Get the parent entity from registry
      const parentEntity = this.registry.get(currentId);
      if (!parentEntity || !('getParentContext' in parentEntity)) {
        // Reached root node or entity not found
        break;
      }
      
      // Move up to the next parent
      const parentContext = parentEntity.getParentContext();
      currentId = parentContext?.parentId;
    }
    
    return false;
  }

  /**
   * Gets the depth of a parent execution
   * 
   * Queries the registry to get the actual parent's depth.
   * Falls back to 0 if registry is not available or parent is not found.
   * 
   * @param parentContext - The parent context
   * @returns The depth of the parent execution
   */
  private getParentDepth(parentContext: ParentExecutionContext): number {
    if (!this.registry) {
      logger.warn('Registry not available, assuming parent is root (depth=0)');
      return 0;
    }

    const parentEntity = this.registry.get(parentContext.parentId);
    if (!parentEntity || !('getHierarchyDepth' in parentEntity)) {
      // Parent not found or doesn't have hierarchy methods, assume it's root
      return 0;
    }
    
    return parentEntity.getHierarchyDepth();
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
      
      // Inherit root information from parent
      this.inheritRootInfoFromParent();
    }
  }

  /**
   * Inherits root execution information from parent
   * 
   * Queries the registry to get the parent's root execution info.
   * Falls back to parent's own ID if registry is not available.
   */
  private inheritRootInfoFromParent(): void {
    if (!this.registry || !this.parent) {
      // Fallback: assume parent is root
      logger.warn('Registry not available or parent is undefined, assuming parent is root');
      if (this.parent) {
        this.rootExecutionId = this.parent.parentId;
        this.rootExecutionType = this.parent.parentType;
      }
      return;
    }

    const parentEntity = this.registry.get(this.parent.parentId);
    if (parentEntity && 'getRootExecutionId' in parentEntity && 'getRootExecutionType' in parentEntity) {
      // Inherit from parent's root
      this.rootExecutionId = parentEntity.getRootExecutionId();
      this.rootExecutionType = parentEntity.getRootExecutionType();
    } else {
      // Fallback: assume parent is root
      logger.warn('Parent entity not found or missing hierarchy methods, assuming parent is root', {
        parentId: this.parent.parentId,
      });
      this.rootExecutionId = this.parent.parentId;
      this.rootExecutionType = this.parent.parentType;
    }
  }
}
