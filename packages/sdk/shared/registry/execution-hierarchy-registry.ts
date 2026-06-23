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
 *
 * @note Validation/repair logic is delegated to HierarchyIntegrityService.
 * @note Tree traversal operations are delegated to HierarchyTraversalService.
 */

import type { ID, ExecutionType } from "@wf-agent/types";
import type { WorkflowExecutionEntity } from "../../workflow/entities/workflow-execution-entity.js";
import type { AgentLoopEntity } from "../../agent/entities/agent-loop-entity.js";
import type { ExecutionHierarchyMetadata } from "@wf-agent/types";
import {
  HierarchyIntegrityService,
  type HierarchyValidationResult,
  type IHierarchyRegistry,
} from "../execution/hierarchy-integrity-service.js";
import { HierarchyTraversalService } from "./utils/index.js";
import { createRegistry } from "./utils/index.js";

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
 * Focused on registration and discovery of execution instances.
 * Tree traversal and hierarchy operations are delegated to HierarchyTraversalService.
 * Validation/repair logic is delegated to HierarchyIntegrityService.
 *
 * Key Features:
 * - Register/unregister execution instances
 * - Query descendants recursively (via HierarchyTraversalService)
 * - Clean up entire hierarchy trees (via HierarchyTraversalService)
 * - Group executions by root (via HierarchyTraversalService)
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
export class ExecutionHierarchyRegistry implements IHierarchyRegistry {
  private items = createRegistry<AnyExecutionEntity>();
  private traversalService: HierarchyTraversalService;

  constructor() {
    this.traversalService = new HierarchyTraversalService(this);
  }

  // ==================== Core Registration/Discovery ====================

  /**
   * Registers an execution instance
   */
  register(execution: AnyExecutionEntity): void {
    this.items.set(execution.id, execution);
  }

  /**
   * Unregisters an execution instance
   */
  unregister(executionId: ID): boolean {
    return this.items.delete(executionId);
  }

  /**
   * Gets an execution instance by ID
   */
  get(executionId: ID): AnyExecutionEntity | undefined {
    return this.items.get(executionId);
  }

  /**
   * Checks if an execution instance exists
   */
  has(executionId: ID): boolean {
    return this.items.has(executionId);
  }

  /**
   * Gets all registered execution instances
   */
  getAll(): AnyExecutionEntity[] {
    return this.items.list();
  }

  /**
   * Gets all registered execution IDs
   */
  getAllIds(): ID[] {
    return this.items.keys();
  }

  /**
   * Gets the number of registered executions
   */
  size(): number {
    return this.items.size;
  }

  /**
   * Clears all registered executions
   */
  clear(): void {
    this.items.clear();
  }

  // ==================== Hierarchy Traversal (delegated) ====================

  /**
   * Gets all descendant executions of a given execution (recursive)
   */
  getAllDescendants(executionId: ID, includeSelf: boolean = false): AnyExecutionEntity[] {
    return this.traversalService.getAllDescendants(executionId, includeSelf);
  }

  /**
   * Gets direct children of a given execution
   */
  getDirectChildren(executionId: ID): AnyExecutionEntity[] {
    return this.traversalService.getDirectChildren(executionId);
  }

  /**
   * Cleans up an execution and all its descendants
   */
  cleanupHierarchy(executionId: ID): number {
    return this.traversalService.cleanupHierarchy(executionId);
  }

  /**
   * Gets all executions under a given root execution, grouped by type
   */
  getExecutionsByRoot(rootExecutionId: ID): ExecutionsByRoot {
    return this.traversalService.getExecutionsByRoot(rootExecutionId);
  }

  /**
   * Gets all root executions (executions without parents)
   */
  getRootExecutions(): AnyExecutionEntity[] {
    return this.traversalService.getRootExecutions();
  }

  /**
   * Gets all executions that have a specific parent
   */
  getChildrenOf(parentId: ID): AnyExecutionEntity[] {
    return this.traversalService.getChildrenOf(parentId);
  }

  /**
   * Gets all executions of a specific type
   */
  getByType(type: ExecutionType): AnyExecutionEntity[] {
    return this.traversalService.getByType(type);
  }

  /**
   * Checks if an execution is part of a hierarchy tree rooted at the given ID
   */
  isInHierarchy(executionId: ID, rootExecutionId: ID): boolean {
    return this.traversalService.isInHierarchy(executionId, rootExecutionId);
  }

  // ==================== Integrity Validation (delegated) ====================

  /**
   * Validates hierarchy integrity against the registry
   */
  validateHierarchyIntegrity(hierarchy: ExecutionHierarchyMetadata): HierarchyValidationResult {
    return HierarchyIntegrityService.validateIntegrity(hierarchy, this);
  }

  /**
   * Cleans up orphaned references in hierarchy metadata
   */
  cleanupOrphanedReferences(hierarchy: ExecutionHierarchyMetadata): ExecutionHierarchyMetadata {
    return HierarchyIntegrityService.cleanupOrphanedReferences(hierarchy, this);
  }

  /**
   * Repairs hierarchy by recalculating root information
   */
  repairRootInfo(hierarchy: ExecutionHierarchyMetadata): ExecutionHierarchyMetadata {
    return HierarchyIntegrityService.repairRootInfo(hierarchy, this);
  }
}
