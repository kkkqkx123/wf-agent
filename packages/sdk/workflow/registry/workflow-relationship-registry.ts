/**
 * WorkflowRelationshipRegistry - Workflow Relationship Registry
 * Manages hierarchical relationships and reference relationships between workflows.
 *
 * This module only exports class definitions; instances are managed uniformly through DI container.
 */

import type { WorkflowRelationship, WorkflowHierarchy } from "../types/relationship.js";
import type { WorkflowReferenceRelation, WorkflowReferenceType } from "../types/reference.js";

/**
 * WorkflowRelationshipRegistry
 * Responsible for managing parent-child hierarchical relationships and reference relationships between workflows.
 */
export class WorkflowRelationshipRegistry {
  private workflowRelationships: Map<string, WorkflowRelationship> = new Map();
  private referenceRelations: Map<string, WorkflowReferenceRelation[]> = new Map();

  // ============================================================
  // Reference Relationship Methods
  // ============================================================

  /**
   * Add workflow reference relationship
   * @param relation Reference relationship
   */
  addReferenceRelation(relation: WorkflowReferenceRelation): void {
    const key = relation.targetWorkflowId;
    if (!this.referenceRelations.has(key)) {
      this.referenceRelations.set(key, []);
    }
    this.referenceRelations.get(key)!.push(relation);
  }

  /**
   * Remove workflow reference relationship
   * @param sourceWorkflowId Source workflow ID
   * @param targetWorkflowId Target workflow ID
   * @param referenceType Reference type
   */
  removeReferenceRelation(
    sourceWorkflowId: string,
    targetWorkflowId: string,
    referenceType: WorkflowReferenceType,
  ): void {
    const relations = this.referenceRelations.get(targetWorkflowId);
    if (relations) {
      const filtered = relations.filter(
        (rel: WorkflowReferenceRelation) =>
          !(rel.sourceWorkflowId === sourceWorkflowId && rel.referenceType === referenceType),
      );
      if (filtered.length === 0) {
        this.referenceRelations.delete(targetWorkflowId);
      } else {
        this.referenceRelations.set(targetWorkflowId, filtered);
      }
    }
  }

  /**
   * Check if the workflow has any references.
   * @param workflowId Workflow ID
   * @returns Whether there are any references
   */
  hasReferences(workflowId: string): boolean {
    // Check the references (such as trigger, workflow execution, etc.) in referenceRelations.
    const hasReferenceRelations =
      this.referenceRelations.has(workflowId) &&
      this.referenceRelations.get(workflowId)!.length > 0;

    // Check the parent-child relationships within workflowRelationships.
    const hasParentRelationship = this.getParentWorkflow(workflowId) !== null;

    return hasReferenceRelations || hasParentRelationship;
  }

  /**
   * Get reference relationships for a workflow
   * @param workflowId Workflow ID
   * @returns Array of reference relationships
   */
  getReferenceRelations(workflowId: string): WorkflowReferenceRelation[] {
    return this.referenceRelations.get(workflowId) || [];
  }

  /**
   * Clear all reference relationships for a workflow
   * @param workflowId Workflow ID
   */
  clearReferenceRelations(workflowId: string): void {
    this.referenceRelations.delete(workflowId);
  }

  /**
   * Clean up all references related to the workflow
   * @param workflowId Workflow ID
   */
  cleanupWorkflowReferences(workflowId: string): void {
    // 1. Remove the references where the current workflow is the target.
    this.referenceRelations.delete(workflowId);

    // 2. Remove the references where the current workflow is the source.
    for (const [targetId, relations] of this.referenceRelations.entries()) {
      const filtered = relations.filter(
        (rel: WorkflowReferenceRelation) => rel.sourceWorkflowId !== workflowId,
      );
      if (filtered.length === 0) {
        this.referenceRelations.delete(targetId);
      } else {
        this.referenceRelations.set(targetId, filtered);
      }
    }

    // 3. Delete hierarchical relations where the current workflow is a child.
    this.workflowRelationships.delete(workflowId);
    // Also remove it from its parent's childWorkflowIds
    for (const [, relationship] of this.workflowRelationships.entries()) {
      relationship.childWorkflowIds.delete(workflowId);
    }
  }

  /**
   * Get all source workflow IDs that reference the target workflow.
   * @param targetWorkflowId Target workflow ID
   * @returns Array of source workflow IDs
   */
  getReferencingWorkflows(targetWorkflowId: string): string[] {
    const referencingWorkflows = new Set<string>();

    // Find from reference relationships
    const relations = this.getReferenceRelations(targetWorkflowId);
    relations.forEach(relation => {
      referencingWorkflows.add(relation.sourceWorkflowId);
    });

    // Find from parent-child relationship
    const parentId = this.getParentWorkflow(targetWorkflowId);
    if (parentId) {
      referencingWorkflows.add(parentId);
    }

    return Array.from(referencingWorkflows);
  }

  // ============================================================
  // Hierarchy Relationship Methods
  // ============================================================

  /**
   * Register subgraph relationship
   * @param parentWorkflowId Parent workflow ID
   * @param subgraphNodeId SUBGRAPH node ID
   * @param childWorkflowId Child workflow ID
   */
  registerSubgraphRelationship(
    parentWorkflowId: string,
    subgraphNodeId: string,
    childWorkflowId: string,
  ): void {
    // 1. Update the parent workflow relationship
    const parentRelationship = this.workflowRelationships.get(parentWorkflowId);
    if (parentRelationship) {
      parentRelationship.childWorkflowIds.add(childWorkflowId);
      parentRelationship.referencedBy.set(subgraphNodeId, childWorkflowId);
    } else {
      this.workflowRelationships.set(parentWorkflowId, {
        workflowId: parentWorkflowId,
        childWorkflowIds: new Set([childWorkflowId]),
        referencedBy: new Map([[subgraphNodeId, childWorkflowId]]),
        depth: 0,
      });
    }

    // 2. Update sub-workflow relationships
    const childRelationship = this.workflowRelationships.get(childWorkflowId);
    if (!childRelationship) {
      this.workflowRelationships.set(childWorkflowId, {
        workflowId: childWorkflowId,
        parentWorkflowId,
        childWorkflowIds: new Set(),
        referencedBy: new Map(),
        depth: this.calculateDepth(parentWorkflowId) + 1,
      });
    }
  }

  /**
   * Get the workflow hierarchy structure
   * @param workflowId Workflow ID
   * @returns Hierarchy structure information
   */
  getWorkflowHierarchy(workflowId: string): WorkflowHierarchy {
    const ancestors: string[] = [];
    const descendants: string[] = [];

    // Construct an ancestor chain
    let currentId = workflowId;
    while (currentId) {
      const relationship = this.workflowRelationships.get(currentId);
      if (relationship?.parentWorkflowId) {
        ancestors.unshift(relationship.parentWorkflowId);
        currentId = relationship.parentWorkflowId;
      } else {
        break;
      }
    }

    // Constructing a descendant chain (recursively)
    this.collectDescendants(workflowId, descendants);

    const relationship = this.workflowRelationships.get(workflowId);
    return {
      ancestors,
      descendants,
      depth: relationship?.depth || 0,
      rootWorkflowId: ancestors[0] || workflowId,
    };
  }

  /**
   * Get the parent workflow
   * @param workflowId Workflow ID
   * @returns Parent workflow ID or null
   */
  getParentWorkflow(workflowId: string): string | null {
    const relationship = this.workflowRelationships.get(workflowId);
    return relationship?.parentWorkflowId || null;
  }

  /**
   * Get sub-workflows
   * @param workflowId Workflow ID
   * @returns Array of sub-workflow IDs
   */
  getChildWorkflows(workflowId: string): string[] {
    const relationship = this.workflowRelationships.get(workflowId);
    return relationship ? Array.from(relationship.childWorkflowIds) : [];
  }

  /**
   * Collect all descendant workflows.
   */
  private collectDescendants(workflowId: string, result: string[]): void {
    const relationship = this.workflowRelationships.get(workflowId);
    if (!relationship) return;

    for (const childId of relationship.childWorkflowIds) {
      if (!result.includes(childId)) {
        result.push(childId);
        this.collectDescendants(childId, result);
      }
    }
  }

  /**
   * Calculate the depth of the workflow.
   */
  private calculateDepth(workflowId: string): number {
    const relationship = this.workflowRelationships.get(workflowId);
    return relationship?.depth || 0;
  }

  /**
   * Clear all relationships and references.
   */
  clear(): void {
    this.workflowRelationships.clear();
    this.referenceRelations.clear();
  }
}