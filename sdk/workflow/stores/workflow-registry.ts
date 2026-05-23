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
  WorkflowTemplate,
  WorkflowRelationship,
  WorkflowHierarchy,
  RegisterOptions,
  BatchRegisterOptions,
  UnregisterOptions,
  BatchUnregisterOptions,
  UpdateOptions,
  WorkflowGraph,
} from "@wf-agent/types";
import type {
  WorkflowReferenceInfo,
  WorkflowReferenceRelation,
  WorkflowReferenceType,
} from "@wf-agent/types";
import type { WorkflowSummary } from "../../api/workflow/resources/workflows/workflow-registry-api.js";
import type { WorkflowExecutionRegistry } from "./workflow-execution-registry.js";
import type { WorkflowStorageAdapter } from "@wf-agent/storage";
import {
  ExecutionError,
  ConfigurationValidationError,
  WorkflowNotFoundError,
} from "@wf-agent/types";
import type { WorkflowGraphRegistry } from "./workflow-graph-registry.js";
import { WorkflowGraphBuilder } from "../builder/workflow-graph-builder.js";
import * as Identifiers from "../../core/di/service-identifiers.js";
import type { GlobalContext } from "../../core/global-context.js";
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
  workflow: WorkflowTemplate;
}

/**
 * WorkflowRegistry - Workflow Registry
 */
export class WorkflowRegistry {
  private workflows: Map<string, WorkflowTemplate> = new Map();
  private workflowRelationships: Map<string, WorkflowRelationship> = new Map();
  private activeWorkflows: Set<string> = new Set();
  private referenceRelations: Map<string, WorkflowReferenceRelation[]> = new Map();
  private workflowExecutionRegistry: WorkflowExecutionRegistry | undefined;
  private storageAdapter: WorkflowStorageAdapter | null = null;

  constructor(
    private readonly globalContext: GlobalContext,
    options: {
      maxRecursionDepth?: number;
      storageAdapter?: WorkflowStorageAdapter;
    } = {},
    workflowExecutionRegistry?: WorkflowExecutionRegistry,
  ) {
    this.workflowExecutionRegistry = workflowExecutionRegistry;
    this.storageAdapter = options.storageAdapter || null;
  }

  /**
   * Obtain a WorkflowExecutionRegistry instance (with delayed retrieval)
   * @returns A WorkflowExecutionRegistry instance or undefined
   */
  private getWorkflowExecutionRegistry(): WorkflowExecutionRegistry | undefined {
    if (!this.workflowExecutionRegistry) {
      this.workflowExecutionRegistry = this.globalContext.container.get(Identifiers.WorkflowExecutionRegistry) as WorkflowExecutionRegistry;
    }
    return this.workflowExecutionRegistry;
  }

  /**
   * Obtain a WorkflowGraphRegistry instance (with delayed retrieval)
   * @returns WorkflowGraphRegistry instance
   */
  private getWorkflowGraphRegistry(): WorkflowGraphRegistry {
    return this.globalContext.container.get(Identifiers.WorkflowGraphRegistry) as WorkflowGraphRegistry;
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
  register(workflow: WorkflowTemplate, options?: RegisterOptions): void {
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
  async registerAsync(workflow: WorkflowTemplate, options?: RegisterOptions): Promise<void> {
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

    // Save the workflow definition to memory cache.
    this.workflows.set(workflow.id, workflow);

    // Persist to storage (async, non-blocking)
    this.persistToStorage(workflow).catch(error => {
      logger.error("Failed to persist workflow during registration", {
        workflowId: workflow.id,
        error: getErrorMessage(error),
      });
    });

    // Preprocessing workflow asynchronously
    try {
      await this.preprocessWorkflow(workflow);
    } catch (error) {
      // Remove the workflow from both memory and storage if preprocessing fails
      this.workflows.delete(workflow.id);
      this.removeFromStorage(workflow.id).catch(err => {
        logger.error("Failed to remove workflow from storage after preprocessing failure", {
          workflowId: workflow.id,
          error: getErrorMessage(err),
        });
      });
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
   */
  private async preprocessWorkflow(workflow: WorkflowTemplate): Promise<void> {
    const graphRegistry = this.getWorkflowGraphRegistry();

    // Check if it has already been preprocessed.
    if (graphRegistry.has(workflow.id)) {
      return;
    }

    // Recursively preprocess EMBED_GRAPH dependents first
    // This ensures subworkflow graphs are available when the parent graph is expanded
    for (const node of workflow.nodes) {
      if (node.type === "EMBED_GRAPH") {
        const embedId = (node.config as { embedId?: string })?.embedId;
        if (embedId && !graphRegistry.has(embedId)) {
          const subworkflow = this.get(embedId);
          if (subworkflow) {
            await this.preprocessWorkflow(subworkflow);
          }
        }
      }
    }

    // Use WorkflowGraphBuilder to build and validate the graph
    const { graph, isValid, errors } = WorkflowGraphBuilder.buildAndValidate(workflow);

    if (!isValid) {
      throw new ConfigurationValidationError(
        `Workflow validation failed: ${errors.join(", ")}`,
        { configPath: workflow.id, context: { errors } },
      );
    }

    // Expand EMBED_GRAPH nodes (merge subworkflow graphs into parent)
    const mergeResult = await WorkflowGraphBuilder.processSubgraphs(
      graph,
      this,
      graphRegistry as unknown as { get: (id: string) => import("../entities/workflow-graph-data.js").WorkflowGraphData | undefined },
    );

    if (!mergeResult.success) {
      throw new ConfigurationValidationError(
        `EMBED_GRAPH expansion failed for workflow '${workflow.id}': ${mergeResult.errors.join("; ")}`,
        { configPath: workflow.id, context: { errors: mergeResult.errors } },
      );
    }

    // Preserve the WorkflowGraphData class instance (with its prototype methods like getNode)
    // and add workflowId property directly without spreading (which would lose methods)
    (graph as unknown as Record<string, unknown>)['workflowId'] = workflow.id;
    (graph as unknown as Record<string, unknown>)['workflowVersion'] = workflow.version;
    
    // Cache processing results
    graphRegistry.register(graph as unknown as WorkflowGraph);
  }

  /**
   * Batch registration workflow definitions
   * @param workflows: An array of workflow definitions
   * @param options: Batch registration options
   */
  registerBatch(workflows: WorkflowTemplate[], options?: BatchRegisterOptions): void {
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
  update(workflowId: string, updates: Partial<WorkflowTemplate>, options?: UpdateOptions): void {
    const workflow = this.workflows.get(workflowId);
    if (!workflow) {
      if (options?.createIfNotExists && updates.id === workflowId) {
        // Allow automatic creation
        const newWorkflow = { ...updates, id: workflowId } as WorkflowTemplate;
        this.register(newWorkflow);
        return;
      }
      throw new WorkflowNotFoundError(
        `Workflow '${workflowId}' not found. Use register() to create or upsert() to create or update.`,
        workflowId,
      );
    }

    // Create an updated workflow
    const updatedWorkflow: WorkflowTemplate = {
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
  upsert(workflow: WorkflowTemplate): void {
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
  get(workflowId: string): WorkflowTemplate | undefined {
    // Check memory cache first
    const workflow = this.workflows.get(workflowId);
    
    // If not in memory and storage adapter is available, try to load from storage
    // Note: This is a simplified approach - ideally we'd have an async get() method
    // For now, we rely on initializeFromStorage() to pre-populate the cache
    
    return workflow;
  }

  /**
   * Get the workflow definition by name
   * @param name: Workflow name
   * @returns: Workflow definition; returns undefined if it does not exist
   */
  getByName(name: string): WorkflowTemplate | undefined {
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
  getByTags(tags: string[]): WorkflowTemplate[] {
    const result: WorkflowTemplate[] = [];
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
  getByCategory(category: string): WorkflowTemplate[] {
    const result: WorkflowTemplate[] = [];
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
  getByAuthor(author: string): WorkflowTemplate[] {
    const result: WorkflowTemplate[] = [];
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
  async list(): Promise<WorkflowSummary[]> {
    const memoryWorkflows = Array.from(this.workflows.values());
    
    // If no storage adapter, return only memory workflows
    if (!this.storageAdapter) {
      return this.buildWorkflowSummaries(memoryWorkflows);
    }

    try {
      // Get all IDs from storage
      const storageIds = await this.storageAdapter.list();
      
      // Load workflows that are not in memory cache
      const loadedWorkflows: WorkflowTemplate[] = [];
      for (const id of storageIds) {
        if (!this.workflows.has(id)) {
          const workflow = await this.loadFromStorage(id);
          if (workflow) {
            loadedWorkflows.push(workflow);
            // Cache in memory for future access
            this.workflows.set(id, workflow);
          }
        }
      }

      // Merge memory and storage workflows (memory takes precedence)
      const allWorkflows = [...memoryWorkflows, ...loadedWorkflows];
      return this.buildWorkflowSummaries(allWorkflows);
    } catch (error) {
      logger.error("Failed to list workflows from storage", {
        error: getErrorMessage(error),
      });
      // Return only memory workflows as fallback
      return this.buildWorkflowSummaries(memoryWorkflows);
    }
  }

  /**
   * Helper method to build workflow summaries
   * @param workflows Array of workflow templates
   * @returns Array of workflow summaries
   */
  private buildWorkflowSummaries(workflows: WorkflowTemplate[]): WorkflowSummary[] {
    return workflows.map(workflow => ({
      id: workflow.id,
      name: workflow.name,
      description: workflow.description,
      type: workflow.type,
      version: workflow.version,
      nodeCount: workflow.nodes.length,
      edgeCount: workflow.edges.length,
      createdAt: workflow.createdAt,
      updatedAt: workflow.updatedAt,
      tags: workflow.metadata?.tags,
      category: workflow.metadata?.category,
    }));
  }

  /**
   * Search Workflow
   * @param keyword Search keyword
   * @returns List of matching workflow summary information
   */
  async search(keyword: string): Promise<WorkflowSummary[]> {
    const lowerKeyword = keyword.toLowerCase();
    const summaries = await this.list();
    return summaries.filter(
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
  async checkWorkflowReferences(workflowId: string): Promise<WorkflowReferenceInfo> {
    const workflowExecutionRegistry = this.getWorkflowExecutionRegistry();
    if (!workflowExecutionRegistry) {
      throw new ExecutionError("WorkflowExecutionRegistry not available", undefined, workflowId, {
        operation: "check_workflow_references",
      });
    }
    return await checkWorkflowReferences(this, workflowExecutionRegistry, workflowId);
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
   * Check if the workflow can be safely deleted
   * @param workflowId Workflow ID
   * @param options: Deletion options
   * @returns: Whether the workflow can be deleted and detailed information
   */
  async canSafelyDelete(
    workflowId: string,
    options?: UnregisterOptions,
  ): Promise<{ canDelete: boolean; details: string }> {
    const referenceInfo = await this.checkWorkflowReferences(workflowId);
  
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
   * Unregister a workflow definition
   * @param workflowId: Workflow ID
   * @param options Options for deletion
   */
  async unregister(workflowId: string, options?: UnregisterOptions): Promise<void> {
    // Check if the workflow exists.
    if (!this.workflows.has(workflowId)) {
      throw new WorkflowNotFoundError(`Workflow '${workflowId}' not found`, workflowId);
    }

    const shouldCheck = options?.checkReferences !== false;

    if (shouldCheck) {
      const checkResult = await this.canSafelyDelete(workflowId, options);
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

    // Remove from storage (async, non-blocking)
    this.removeFromStorage(workflowId).catch(error => {
      logger.error("Failed to remove workflow from storage during unregister", {
        workflowId,
        error: getErrorMessage(error),
      });
    });

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
  validate(workflow: WorkflowTemplate): { valid: boolean; errors: string[] } {
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
  validateBatch(workflows: WorkflowTemplate[]): { valid: boolean; errors: string[] }[] {
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
      const workflow = JSON.parse(json) as WorkflowTemplate;
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

  // ============================================================
  // Storage Persistence Methods
  // ============================================================

  /**
   * Persist workflow to storage (if adapter is available)
   * @param workflow Workflow template to persist
   */
  private async persistToStorage(workflow: WorkflowTemplate): Promise<void> {
    if (!this.storageAdapter) {
      logger.debug("No storage adapter configured, skipping workflow persistence");
      return;
    }

    try {
      // Serialize workflow to bytes
      const encoder = new TextEncoder();
      const data = encoder.encode(JSON.stringify(workflow));

      // Create metadata matching WorkflowStorageMetadata interface
      const metadata = {
        workflowId: workflow.id,
        name: workflow.name,
        type: workflow.type,
        version: workflow.version,
        description: workflow.description,
        createdAt: workflow.createdAt,
        updatedAt: workflow.updatedAt,
        nodeCount: workflow.nodes.length,
        edgeCount: workflow.edges.length,
        enabled: true,
        tags: workflow.metadata?.tags,
        category: workflow.metadata?.category,
        author: workflow.metadata?.author,
      };

      // Save to storage
      await this.storageAdapter.save(workflow.id, data, metadata);
      logger.debug("Workflow persisted to storage", { workflowId: workflow.id });
    } catch (error) {
      // Log error but don't fail the registration
      logger.error("Failed to persist workflow to storage", {
        workflowId: workflow.id,
        error: getErrorMessage(error),
      });
    }
  }

  /**
   * Remove workflow from storage (if adapter is available)
   * @param workflowId Workflow ID to remove
   */
  private async removeFromStorage(workflowId: string): Promise<void> {
    if (!this.storageAdapter) {
      return;
    }

    try {
      await this.storageAdapter.delete(workflowId);
      logger.debug("Workflow removed from storage", { workflowId });
    } catch (error) {
      logger.error("Failed to remove workflow from storage", {
        workflowId,
        error: getErrorMessage(error),
      });
    }
  }

  /**
   * Load workflow from storage (if adapter is available)
   * @param workflowId Workflow ID to load
   * @returns Workflow template or null if not found
   */
  private async loadFromStorage(workflowId: string): Promise<WorkflowTemplate | null> {
    if (!this.storageAdapter) {
      return null;
    }

    try {
      const data = await this.storageAdapter.load(workflowId);
      if (!data) {
        return null;
      }

      const decoder = new TextDecoder();
      const json = decoder.decode(data);
      return JSON.parse(json) as WorkflowTemplate;
    } catch (error) {
      logger.error("Failed to load workflow from storage", {
        workflowId,
        error: getErrorMessage(error),
      });
      return null;
    }
  }

  /**
   * Initialize registry by loading all workflows from storage
   * Call this after construction if you want to pre-populate the cache
   */
  async initializeFromStorage(): Promise<void> {
    if (!this.storageAdapter) {
      logger.info("No storage adapter configured, skipping workflow initialization from storage");
      return;
    }

    try {
      const ids = await this.storageAdapter.list();
      logger.info("Loading workflows from storage", { count: ids.length });

      let loadedCount = 0;
      for (const id of ids) {
        if (!this.workflows.has(id)) {
          const workflow = await this.loadFromStorage(id);
          if (workflow) {
            this.workflows.set(id, workflow);
            loadedCount++;
          }
        }
      }

      logger.info("Successfully loaded workflows from storage", {
        total: ids.length,
        loaded: loadedCount,
      });
    } catch (error) {
      logger.error("Failed to initialize workflows from storage", {
        error: getErrorMessage(error),
      });
      // Don't throw - allow registry to work with empty cache
    }
  }
}
