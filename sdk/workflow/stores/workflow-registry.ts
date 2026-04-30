/**
 * WorkflowRegistry - Workflow Registry
 * Responsible for the registration, querying, and management of workflow definitions
 * This includes the management of workflow relationships and references
 *
 * The pre-processed graph is managed by WorkflowGraphRegistry.
 *
 * This module only exports class definitions; instances are managed uniformly through SingletonRegistry.
 *
 */

import type {
  WorkflowDefinition,
  WorkflowRelationship,
  WorkflowHierarchy,
  RegisterOptions,
  BatchRegisterOptions,
  UnregisterOptions,
  BatchUnregisterOptions,
  UpdateOptions,
} from "@wf-agent/types";
import type {
  WorkflowReferenceInfo,
  WorkflowReferenceRelation,
  WorkflowReferenceType,
} from "@wf-agent/types";
import type { WorkflowSummary } from "../../api/workflow/resources/workflows/workflow-registry-api.js";
import type { WorkflowExecutionRegistry } from "./workflow-execution-registry.js";
import {
  ExecutionError,
  ConfigurationValidationError,
  WorkflowNotFoundError,
} from "@wf-agent/types";
import type { WorkflowGraphRegistry } from "./workflow-graph-registry.js";
import { WorkflowGraphBuilder } from "../builder/workflow-graph-builder.js";
import { getContainer } from "../../core/di/container-config.js";
import * as Identifiers from "../../core/di/service-identifiers.js";
import { getErrorMessage } from "@wf-agent/common-utils";
import { checkWorkflowReferences } from "../execution/utils/workflow-reference-checker.js";
import { createContextualLogger } from "../../utils/contextual-logger.js";

const logger = createContextualLogger();

/**
 * Workflow version information
 */
export interface WorkflowVersion {
  version: string;
  createdAt: number;
  workflow: WorkflowDefinition;
}

/**
 * WorkflowRegistry - Workflow Registry
 */
export class WorkflowRegistry {
  private workflows: Map<string, WorkflowDefinition> = new Map();
  private workflowRelationships: Map<string, WorkflowRelationship> = new Map();
  private activeWorkflows: Set<string> = new Set();
  private referenceRelations: Map<string, WorkflowReferenceRelation[]> = new Map();
  private maxRecursionDepth: number;
  private workflowExecutionRegistry: WorkflowExecutionRegistry | undefined;

  constructor(
    options: {
      maxRecursionDepth?: number;
    } = {},
    workflowExecutionRegistry?: WorkflowExecutionRegistry,
  ) {
    this.maxRecursionDepth = options.maxRecursionDepth ?? 10;
    this.workflowExecutionRegistry = workflowExecutionRegistry;
  }

  /**
   * Obtain a WorkflowExecutionRegistry instance (with delayed retrieval)
   * @returns A WorkflowExecutionRegistry instance or undefined
   */
  private getWorkflowExecutionRegistry(): WorkflowExecutionRegistry | undefined {
    if (!this.workflowExecutionRegistry) {
      const container = getContainer();
      this.workflowExecutionRegistry = container.get(Identifiers.WorkflowExecutionRegistry) as WorkflowExecutionRegistry;
    }
    return this.workflowExecutionRegistry;
  }

  /**
   * Obtain a WorkflowGraphRegistry instance (with delayed retrieval)
   * @returns WorkflowGraphRegistry instance
   */
  private getWorkflowGraphRegistry(): WorkflowGraphRegistry {
    const container = getContainer();
    return container.get(Identifiers.WorkflowRegistry) as WorkflowGraphRegistry;
  }

  /**
   * Add an active workflow
   * @param workflowId Workflow ID
   */
  addActiveWorkflow(workflowId: string): void {
    this.activeWorkflows.add(workflowId);
  }

  /**
   * Remove active workflows
   * @param workflowId Workflow ID
   */
  removeActiveWorkflow(workflowId: string): void {
    this.activeWorkflows.delete(workflowId);
  }

  /**
   * Check if the workflow is active.
   * @param workflowId: Workflow ID
   * @returns: Whether the workflow is active
   */
  isWorkflowActive(workflowId: string): boolean {
    return this.activeWorkflows.has(workflowId);
  }

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
   * @param sourceWorkflowId: Source workflow ID
   * @param targetWorkflowId: Target workflow ID
   * @param referenceType: Reference type
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
   * @param workflowId: Workflow ID
   * @returns: Whether there are any references
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
   * Retrieve workflow reference relationships
   * @param workflowId: Workflow ID
   * @returns: List of reference relationships
   */
  getReferenceRelations(workflowId: string): WorkflowReferenceRelation[] {
    return this.referenceRelations.get(workflowId) || [];
  }

  /**
   * Clear workflow reference relationships
   * @param workflowId Workflow ID
   */
  clearReferenceRelations(workflowId: string): void {
    this.referenceRelations.delete(workflowId);
  }

  /**
   * Clean up all reference relationships in the specified workflow.
   * @param workflowId: Workflow ID
   */
  cleanupWorkflowReferences(workflowId: string): void {
    // Clear the reference relationships for this workflow.
    this.clearReferenceRelations(workflowId);

    // Remove references to this workflow from the reference relationships of other workflows.
    for (const [targetId, relations] of this.referenceRelations.entries()) {
      const filteredRelations = relations.filter(
        relation => relation.sourceWorkflowId !== workflowId,
      );
      if (filteredRelations.length === 0) {
        this.referenceRelations.delete(targetId);
      } else {
        this.referenceRelations.set(targetId, filteredRelations);
      }
    }
  }

  /**
   * Get all active workflow IDs
   * @returns Array of active workflow IDs
   */
  getActiveWorkflows(): string[] {
    return Array.from(this.activeWorkflows);
  }

  /**
   * Register workflow definition (only for new additions)
   * Note: This is a synchronous registration that does NOT perform preprocessing.
   * For full validation and preprocessing, use registerAsync() instead.
   * @param workflow: The workflow definition
   * @param options: Registration options
   * @throws ValidationError: If the workflow definition is invalid or the ID already exists
   */
  register(workflow: WorkflowDefinition, options?: RegisterOptions): void {
    // Verify the workflow definition.
    const validationResult = this.validate(workflow);
    if (!validationResult.valid) {
      throw new ConfigurationValidationError(
        `Workflow validation failed: ${validationResult.errors.join(", ")}`,
        {
          configType: "workflow",
          configPath: "workflow",
        },
      );
    }

    // Check if the ID already exists.
    if (this.workflows.has(workflow.id)) {
      if (options?.skipIfExists) {
        // Idempotent operation: Skip existing items.
        return;
      }
      throw new ConfigurationValidationError(
        `Workflow with ID '${workflow.id}' already exists. Use update() to modify or upsert() to create or update.`,
        {
          configType: "workflow",
          configPath: "workflow.id",
        },
      );
    }

    // Save the workflow definition.
    this.workflows.set(workflow.id, workflow);

    // Note: Preprocessing is NOT performed in synchronous register().
    // Use registerAsync() for full validation and preprocessing.
  }

  /**
   * Register workflow definition asynchronously (with async preprocessing)
   * @param workflow: The workflow definition
   * @param options: Registration options
   * @throws ValidationError: If the workflow definition is invalid or the ID already exists
   */
  async registerAsync(workflow: WorkflowDefinition, options?: RegisterOptions): Promise<void> {
    // Verify the workflow definition.
    const validationResult = this.validate(workflow);
    if (!validationResult.valid) {
      throw new ConfigurationValidationError(
        `Workflow validation failed: ${validationResult.errors.join(", ")}`,
        {
          configType: "workflow",
          configPath: "workflow",
        },
      );
    }

    // Check if the ID already exists.
    if (this.workflows.has(workflow.id)) {
      if (options?.skipIfExists) {
        // Idempotent operation: Skip existing items.
        return;
      }
      throw new ConfigurationValidationError(
        `Workflow with ID '${workflow.id}' already exists. Use update() to modify or upsert() to create or update.`,
        {
          configType: "workflow",
          configPath: "workflow.id",
        },
      );
    }

    // Save the workflow definition.
    this.workflows.set(workflow.id, workflow);

    // Preprocessing workflow asynchronously
    try {
      await this.preprocessWorkflow(workflow);
    } catch (error) {
      // Remove the workflow if preprocessing fails
      this.workflows.delete(workflow.id);
      throw new ConfigurationValidationError(
        `Workflow preprocessing failed: ${getErrorMessage(error)}`,
        {
          configType: "workflow",
          configPath: "workflow.definition",
          context: {
            workflowId: workflow.id,
            operation: "workflow_preprocessing",
          },
        },
      );
    }
  }

  /**
   * Preprocessing Workflow
   * @param workflow: Workflow definition
   * @returns: Preprocessed graph
   */
  private async preprocessWorkflow(workflow: WorkflowDefinition): Promise<void> {
    const graphRegistry = this.getWorkflowGraphRegistry();

    // Check if it has already been preprocessed.
    if (graphRegistry.has(workflow.id)) {
      return;
    }

    // Use WorkflowGraphBuilder to build and validate the graph
    const { graph, isValid, errors } = WorkflowGraphBuilder.buildAndValidate(workflow, {
      detectCycles: true,
      analyzeReachability: true,
    });

    if (!isValid) {
      throw new ConfigurationValidationError(
        `Workflow validation failed: ${errors.join(", ")}`,
        { configPath: workflow.id, context: { errors } },
      );
    }

    // Cache processing results - WorkflowGraphData is already compatible with WorkflowGraph
    graphRegistry.register(graph as unknown as import("@wf-agent/types").WorkflowGraph);
  }

  /**
   * Batch registration workflow definitions
   * @param workflows: An array of workflow definitions
   * @param options: Batch registration options
   */
  registerBatch(workflows: WorkflowDefinition[], options?: BatchRegisterOptions): void {
    for (const workflow of workflows) {
      try {
        this.register(workflow, options);
      } catch (error) {
        if (options?.skipErrors) {
          continue;
        }
        throw error;
      }
    }
  }

  /**
   * Update workflow definition (only modifications)
   * @param workflowId Workflow ID
   * @param updates Update content
   * @param options Update options
   * @throws NotFoundError If the workflow does not exist
   * @throws ValidationError If the updated configuration is invalid
   */
  update(workflowId: string, updates: Partial<WorkflowDefinition>, options?: UpdateOptions): void {
    const workflow = this.workflows.get(workflowId);
    if (!workflow) {
      if (options?.createIfNotExists && updates.id === workflowId) {
        // Allow automatic creation
        const newWorkflow = { ...updates, id: workflowId } as WorkflowDefinition;
        this.register(newWorkflow);
        return;
      }
      throw new WorkflowNotFoundError(
        `Workflow '${workflowId}' not found. Use register() to create or upsert() to create or update.`,
        workflowId,
      );
    }

    // Create an updated workflow
    const updatedWorkflow: WorkflowDefinition = {
      ...workflow,
      ...updates,
      id: workflow.id, // The ID cannot be changed.
      updatedAt: Date.now(),
    };

    // Verify the updated workflow.
    const validationResult = this.validate(updatedWorkflow);
    if (!validationResult.valid) {
      throw new ConfigurationValidationError(
        `Workflow validation failed: ${validationResult.errors.join(", ")}`,
        {
          configType: "workflow",
          configPath: "workflow",
        },
      );
    }

    // Update the workflow
    this.workflows.set(workflowId, updatedWorkflow);
  }

  /**
   * Register or update the workflow definition (update if it exists, create if it doesn't).
   * @param workflow The workflow definition
   */
  upsert(workflow: WorkflowDefinition): void {
    if (this.workflows.has(workflow.id)) {
      this.update(workflow.id, workflow);
    } else {
      this.register(workflow);
    }
  }

  /**
   * Get workflow definition
   * @param workflowId: Workflow ID
   * @returns: Workflow definition; returns undefined if it does not exist
   */
  get(workflowId: string): WorkflowDefinition | undefined {
    return this.workflows.get(workflowId);
  }

  /**
   * Get the workflow definition by name
   * @param name: Workflow name
   * @returns: Workflow definition; returns undefined if it does not exist
   */
  getByName(name: string): WorkflowDefinition | undefined {
    for (const workflow of this.workflows.values()) {
      if (workflow.name === name) {
        return workflow;
      }
    }
    return undefined;
  }

  /**
   * Get a list of workflow definitions by tag
   * @param tags An array of tags
   * @returns A list of workflow definitions that match the provided tags
   */
  getByTags(tags: string[]): WorkflowDefinition[] {
    const result: WorkflowDefinition[] = [];
    for (const workflow of this.workflows.values()) {
      const workflowTags = workflow.metadata?.tags || [];
      if (tags.every(tag => workflowTags.includes(tag))) {
        result.push(workflow);
      }
    }
    return result;
  }

  /**
   * Get a list of workflow definitions by category
   * @param category: The category
   * @returns: A list of workflow definitions that match the specified category
   */
  getByCategory(category: string): WorkflowDefinition[] {
    const result: WorkflowDefinition[] = [];
    for (const workflow of this.workflows.values()) {
      if (workflow.metadata?.category === category) {
        result.push(workflow);
      }
    }
    return result;
  }

  /**
   * Get a list of workflow definitions by author
   * @param author Author
   * @returns List of matching workflow definitions
   */
  getByAuthor(author: string): WorkflowDefinition[] {
    const result: WorkflowDefinition[] = [];
    for (const workflow of this.workflows.values()) {
      if (workflow.metadata?.author === author) {
        result.push(workflow);
      }
    }
    return result;
  }

  /**
   * List all summary information for the workflows
   * @returns List of workflow summary information
   */
  list(): WorkflowSummary[] {
    const summaries: WorkflowSummary[] = [];
    for (const workflow of this.workflows.values()) {
      summaries.push({
        id: workflow.id,
        name: workflow.name,
        description: workflow.description,
        version: workflow.version,
        nodeCount: workflow.nodes.length,
        edgeCount: workflow.edges.length,
        createdAt: workflow.createdAt,
        updatedAt: workflow.updatedAt,
        tags: workflow.metadata?.tags,
        category: workflow.metadata?.category,
      });
    }
    return summaries;
  }

  /**
   * Search Workflow
   * @param keyword Search keyword
   * @returns List of matching workflow summary information
   */
  search(keyword: string): WorkflowSummary[] {
    const lowerKeyword = keyword.toLowerCase();
    return this.list().filter(
      summary =>
        summary.name.toLowerCase().includes(lowerKeyword) ||
        summary.description?.toLowerCase().includes(lowerKeyword) ||
        summary.id.toLowerCase().includes(lowerKeyword),
    );
  }

  /**
   * Check workflow reference
   * @param workflowId Workflow ID
   * @returns Reference information
   */
  checkWorkflowReferences(workflowId: string): WorkflowReferenceInfo {
    const workflowExecutionRegistry = this.getWorkflowExecutionRegistry();
    if (!workflowExecutionRegistry) {
      throw new ExecutionError("WorkflowExecutionRegistry not available", undefined, workflowId, {
        operation: "check_workflow_references",
      });
    }
    return checkWorkflowReferences(this, workflowExecutionRegistry, workflowId);
  }

  /**
   * Format citation details information
   * @param references List of references
   * @returns Formatted string
   */
  private formatReferenceDetails(
    references: import("@wf-agent/types").WorkflowReference[],
  ): string {
    if (references.length === 0) {
      return "  No references found.";
    }

    return references
      .map((ref, index) => {
        const details = Object.entries(ref.details)
          .map(([key, value]) => `${key}: ${value}`)
          .join(", ");

        return `  ${index + 1}. [${ref.type}] ${ref.sourceName} (${ref.sourceId}) - ${ref.isRuntimeReference ? "Runtime" : "Static"}${details ? ` - ${details}` : ""}`;
      })
      .join("\n");
  }

  /**
   * Check if the workflow can be safely deleted.
   * @param workflowId: Workflow ID
   * @param options: Deletion options
   * @returns: Whether the workflow can be deleted and detailed information
   */
  canSafelyDelete(
    workflowId: string,
    options?: UnregisterOptions,
  ): { canDelete: boolean; details: string } {
    const referenceInfo = this.checkWorkflowReferences(workflowId);

    if (!referenceInfo.hasReferences) {
      return { canDelete: true, details: "No references found" };
    }

    if (options?.force) {
      if (referenceInfo.stats.runtimeReferences > 0) {
        const runtimeReferences = referenceInfo.references.filter(ref => ref.isRuntimeReference);
        const runtimeDetails = this.formatReferenceDetails(runtimeReferences);
        return {
          canDelete: true,
          details: `Force deleting workflow with ${referenceInfo.stats.runtimeReferences} active references:\n${runtimeDetails}`,
        };
      }
      return { canDelete: true, details: "Force delete enabled" };
    }

    const referenceDetails = this.formatReferenceDetails(referenceInfo.references);
    return {
      canDelete: false,
      details: `Cannot delete workflow: it is referenced by ${referenceInfo.references.length} other components.\n\nReferences:\n${referenceDetails}\n\nUse force=true to override, or check references first.`,
    };
  }

  /**
   * Get all source workflow IDs that reference the target workflow.
   * @param targetWorkflowId: The ID of the target workflow.
   * @returns: An array of source workflow IDs.
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

  /**
   * Remove workflow definition
   * @param workflowId Workflow ID
   * @param options Options for deletion
   */
  unregister(workflowId: string, options?: UnregisterOptions): void {
    // Check if the workflow exists.
    if (!this.workflows.has(workflowId)) {
      throw new WorkflowNotFoundError(`Workflow '${workflowId}' not found`, workflowId);
    }

    const shouldCheck = options?.checkReferences !== false;

    if (shouldCheck) {
      const checkResult = this.canSafelyDelete(workflowId, options);
      if (!checkResult.canDelete) {
        throw new ConfigurationValidationError(checkResult.details, {
          configType: "workflow",
          configPath: "workflow.delete.referenced",
        });
      }

      if (options?.force && checkResult.details.includes("active references")) {
        // Log the warning but do not interrupt the execution.
        logger.warn("Deleting workflow with active references", {
          workflowId,
          operation: "workflow_delete",
        });
      }
    }

    this.workflows.delete(workflowId);

    // Clean up reference relationships
    this.cleanupWorkflowReferences(workflowId);
  }

  /**
   * Batch remove workflow definitions
   * @param workflowIds Array of workflow IDs
   * @param options Options for batch deletion
   */
  unregisterBatch(workflowIds: string[], options?: BatchUnregisterOptions): void {
    for (const workflowId of workflowIds) {
      try {
        this.unregister(workflowId, options);
      } catch (error) {
        if (options?.skipErrors) {
          continue;
        }
        throw error;
      }
    }
  }

  /**
   * Clear all workflow definitions.
   */
  clear(): void {
    this.workflows.clear();
    this.workflowRelationships.clear();
    this.activeWorkflows.clear();
    this.referenceRelations.clear();
  }

  /**
   * Verify workflow definition (basic validation)
   * @param workflow: Workflow definition
   * @returns: Validation result
   */
  validate(workflow: WorkflowDefinition): { valid: boolean; errors: string[] } {
    const errors: string[] = [];

    // Basic validation
    if (!workflow.id) {
      errors.push("Workflow ID is required");
    }

    if (!workflow.name) {
      errors.push("Workflow name is required");
    }

    if (!workflow.nodes || workflow.nodes.length === 0) {
      errors.push("Workflow must have at least one node");
    }

    if (!workflow.edges) {
      errors.push("Workflow edges are required");
    }

    return {
      valid: errors.length === 0,
      errors,
    };
  }

  /**
   * Batch validation of workflow definitions
   * @param workflows: An array of workflow definitions
   * @returns: An array of validation results
   */
  validateBatch(workflows: WorkflowDefinition[]): { valid: boolean; errors: string[] }[] {
    return workflows.map(workflow => this.validate(workflow));
  }

  /**
   * Check if the workflow exists
   * @param workflowId: Workflow ID
   * @returns: Whether it exists or not
   */
  has(workflowId: string): boolean {
    return this.workflows.has(workflowId);
  }

  /**
   * Get the number of registered workflows
   * @returns Number of workflows
   */
  size(): number {
    return this.workflows.size;
  }

  /**
   * The export workflow is defined as a JSON string.
   * @param workflowId: Workflow ID
   * @returns: JSON string
   * @throws: ValidationError if the workflow does not exist
   */
  export(workflowId: string): string {
    const workflow = this.workflows.get(workflowId);
    if (!workflow) {
      throw new WorkflowNotFoundError(
        `Workflow with ID '${workflowId}' does not exist`,
        workflowId,
      );
    }

    return JSON.stringify(workflow, null, 2);
  }

  /**
   * Import workflow definition from a JSON string
   * @param json JSON string
   * @param options Registration options
   * @returns Imported workflow ID
   * @throws ValidationError If the JSON is invalid or the workflow definition is invalid
   */
  import(json: string, options?: RegisterOptions): string {
    try {
      const workflow = JSON.parse(json) as WorkflowDefinition;
      this.register(workflow, options);
      return workflow.id;
    } catch (error) {
      throw new ConfigurationValidationError(
        `Failed to import workflow: ${getErrorMessage(error)}`,
        {
          configType: "workflow",
          configPath: "json",
        },
      );
    }
  }

  /**
   * Register subgraph relationship
   * @param parentWorkflowId: Parent workflow ID
   * @param subgraphNodeId: SUBGRAPH node ID
   * @param childWorkflowId: Child workflow ID
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
   * @param workflowId: Workflow ID
   * @returns: Hierarchy structure information
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
   * @param workflowId: Workflow ID
   * @returns: Parent workflow ID or null
   */
  getParentWorkflow(workflowId: string): string | null {
    const relationship = this.workflowRelationships.get(workflowId);
    return relationship?.parentWorkflowId || null;
  }

  /**
   * Get Sub-workflows
   * @param workflowId: Workflow ID
   * @returns: Array of Sub-workflow IDs
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
}
