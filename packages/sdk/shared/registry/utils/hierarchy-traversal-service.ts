/**
 * Hierarchy Traversal Service
 *
 * Extracted from ExecutionHierarchyRegistry to separate tree traversal concerns
 * from core registration/discovery logic.
 *
 * Responsibilities:
 * - Recursive hierarchy traversal (getAllDescendants, getDirectChildren)
 * - Batch cleanup operations (cleanupHierarchy)
 * - Type-based filtering (getByType, getExecutionsByRoot)
 * - Hierarchy relationship queries (isInHierarchy, getRootExecutions, getChildrenOf)
 */

import type { ID, ExecutionType } from "@wf-agent/types";
import type { ChildExecutionReference, ParentExecutionContext } from "@wf-agent/types";
import type { AnyExecutionEntity, ExecutionsByRoot } from "../execution-hierarchy-registry.js";
import type { WorkflowExecutionEntity } from "../../../workflow/entities/workflow-execution-entity.js";
import type { AgentLoopEntity } from "../../../agent/entities/agent-loop-entity.js";
import type { IHierarchyRegistry } from "../../execution/hierarchy-integrity-service.js";
import { createContextualLogger } from "../../../utils/contextual-logger.js";
import { getErrorOrNew } from "@wf-agent/common-utils";

const logger = createContextualLogger({ component: "HierarchyTraversalService" });

/**
 * Service for hierarchy tree traversal and batch operations.
 * Operates on an IHierarchyRegistry to decouple traversal logic from registry implementation.
 */
export class HierarchyTraversalService {
  constructor(private readonly registry: IHierarchyRegistry & {
    get: (id: ID) => AnyExecutionEntity | undefined;
    getAll: () => AnyExecutionEntity[];
    unregister: (id: ID) => boolean;
  }) {}

  /**
   * Gets all descendant executions of a given execution (recursive)
   */
  getAllDescendants(executionId: ID, includeSelf: boolean = false): AnyExecutionEntity[] {
    const result: AnyExecutionEntity[] = [];

    if (includeSelf) {
      const self = this.registry.get(executionId);
      if (self) {
        result.push(self);
      }
    }

    const entity = this.registry.get(executionId);
    if (!entity) {
      return result;
    }

    const children = this.getDirectChildren(executionId);

    for (const child of children) {
      result.push(child);
      result.push(...this.getAllDescendants(child.id, false));
    }

    return result;
  }

  /**
   * Gets direct children of a given execution
   */
  getDirectChildren(executionId: ID): AnyExecutionEntity[] {
    const parent = this.registry.get(executionId);
    if (!parent) {
      return [];
    }

    const children: AnyExecutionEntity[] = [];

    if ("getChildren" in parent && typeof parent.getChildren === "function") {
      const childRefs: ChildExecutionReference[] = parent.getChildren();

      for (const ref of childRefs) {
        const child = this.registry.get(ref.childId);
        if (child) {
          children.push(child);
        }
      }
    }

    return children;
  }

  /**
   * Cleans up an execution and all its descendants
   */
  cleanupHierarchy(executionId: ID): number {
    const descendants = this.getAllDescendants(executionId, true);
    let count = 0;

    for (const descendant of descendants) {
      if ("stop" in descendant && typeof descendant.stop === "function") {
        try {
          descendant.stop();
        } catch (error) {
          logger.warn(`Failed to stop execution ${descendant.id}`, { error: getErrorOrNew(error) });
        }
      }

      if ("cleanup" in descendant && typeof descendant.cleanup === "function") {
        try {
          descendant.cleanup();
        } catch (error) {
          logger.warn(`Failed to cleanup execution ${descendant.id}`, {
            error: getErrorOrNew(error),
          });
        }
      }

      this.registry.unregister(descendant.id);
      count++;
    }

    return count;
  }

  /**
   * Gets all executions under a given root execution, grouped by type
   */
  getExecutionsByRoot(rootExecutionId: ID): ExecutionsByRoot {
    const allDescendants = this.getAllDescendants(rootExecutionId, true);

    return {
      workflows: allDescendants.filter(
        (e): e is WorkflowExecutionEntity =>
          "getWorkflowId" in e && typeof e.getWorkflowId === "function" && !!e.getWorkflowId(),
      ),
      agents: allDescendants.filter(
        (e): e is AgentLoopEntity =>
          "getConversationManager" in e && typeof e.getConversationManager === "function",
      ),
    };
  }

  /**
   * Gets all root executions (executions without parents)
   */
  getRootExecutions(): AnyExecutionEntity[] {
    return this.registry.getAll().filter(entity => {
      if ("getParentContext" in entity && typeof entity.getParentContext === "function") {
        const parent = entity.getParentContext();
        return !parent;
      }
      return false;
    });
  }

  /**
   * Gets all executions that have a specific parent
   */
  getChildrenOf(parentId: ID): AnyExecutionEntity[] {
    return this.registry.getAll().filter(entity => {
      if ("getParentContext" in entity && typeof entity.getParentContext === "function") {
        const parent = entity.getParentContext();
        return parent?.parentId === parentId;
      }
      return false;
    });
  }

  /**
   * Gets all executions of a specific type
   */
  getByType(type: ExecutionType): AnyExecutionEntity[] {
    return this.registry.getAll().filter(entity => {
      if (type === "WORKFLOW") {
        return (
          "getWorkflowId" in entity &&
          typeof entity.getWorkflowId === "function" &&
          !!entity.getWorkflowId()
        );
      } else {
        return (
          "getConversationManager" in entity && typeof entity.getConversationManager === "function"
        );
      }
    });
  }

  /**
   * Checks if an execution is part of a hierarchy tree rooted at the given ID
   */
  isInHierarchy(executionId: ID, rootExecutionId: ID): boolean {
    if (executionId === rootExecutionId) {
      return true;
    }

    const entity = this.registry.get(executionId);
    if (!entity) {
      return false;
    }

    let current: AnyExecutionEntity | undefined = entity;
    while (current) {
      if ("getParentContext" in current && typeof current.getParentContext === "function") {
        const parent: ParentExecutionContext | undefined = current.getParentContext();
        if (!parent) {
          return false;
        }
        if (parent.parentId === rootExecutionId) {
          return true;
        }
        current = this.registry.get(parent.parentId);
      } else {
        return false;
      }
    }

    return false;
  }
}
