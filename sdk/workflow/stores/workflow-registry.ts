/**
 * WorkflowRegistry - Workflow Registry
 * Responsible for the registration, querying, and management of workflow definitions.
 *
 * Preprocessing, relationship management, and storage persistence are delegated to
 * specialized sub-modules (preprocessWorkflow, WorkflowRelationshipRegistry, WorkflowStorageAdapter).
 *
 * This module only exports class definitions; instances are managed uniformly through SingletonRegistry.
 */

import type {
  WorkflowTemplate,
  RegisterOptions,
  BatchRegisterOptions,
  UnregisterOptions,
  BatchUnregisterOptions,
  UpdateOptions,
} from "@wf-agent/types";
import type { WorkflowReferenceInfo } from "../types/reference.js";
import type { WorkflowSummary } from "../../api/workflow/resources/workflows/workflow-registry-api.js";
import type { WorkflowExecutionRegistry } from "./workflow-execution-registry.js";
import {
  ExecutionError,
  ConfigurationValidationError,
  WorkflowNotFoundError,
} from "@wf-agent/types";
import type { WorkflowRelationshipRegistry } from "./workflow-relationship-registry.js";
import { preprocessWorkflow } from "./utils/workflow-preprocessor.js";
import type { WorkflowGraphRegistry } from "./workflow-graph-registry.js";
import type { WorkflowStorageAdapter } from "@wf-agent/storage";
import {
  persistWorkflow,
  removeWorkflow,
  initializeWorkflowsFromStorage,
  loadWorkflow,
} from "./utils/workflow-storage-utils.js";
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
 *
 * Core responsibilities:
 * - Workflow definition CRUD (register, update, get, unregister)
 * - Query/search workflows by various criteria
 * - Basic validation
 * - Import/export
 *
 * Delegated to sub-modules:
 * - Graph preprocessing -> preprocessWorkflow
 * - Relationship/reference management -> WorkflowRelationshipRegistry
 * - Storage persistence -> WorkflowStorageAdapter
 */
export class WorkflowRegistry {
  private workflows: Map<string, WorkflowTemplate> = new Map();
  private activeWorkflows: Set<string> = new Set();
  constructor(
    private readonly storageAdapter: WorkflowStorageAdapter | null = null,
    private readonly workflowExecutionRegistry?: WorkflowExecutionRegistry,
    private readonly relationshipRegistry?: WorkflowRelationshipRegistry,
    private readonly graphRegistry?: WorkflowGraphRegistry,
  ) {}

  // ============================================================
  // Active Workflow Tracking
  // ============================================================

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
   * Get all active workflow IDs
   * @returns Array of active workflow IDs
   */
  getActiveWorkflows(): string[] {
    return Array.from(this.activeWorkflows);
  }

  // ============================================================
  // Workflow CRUD
  // ============================================================

  /**
   * Register workflow definition (synchronous, memory-only).
   *
   * Writes to the in-memory cache but does NOT persist to storage or
   * perform async preprocessing (graph resolution, relationship tracking).
   * Use for internal operations (e.g. predefined content registration,
   * batch registration) where the workflow is rebuilt on every bootstrap.
   *
   * For durable registration that persists to storage and performs full
   * preprocessing, use registerAsync() instead.
   *
   * @param workflow - The workflow definition
   * @param options - Registration options
   * @throws ValidationError - If the workflow definition is invalid or the ID already exists
   */
  register(workflow: WorkflowTemplate, options?: RegisterOptions): void {
    // Verify the workflow definition.
    const validationResult = this.validate(workflow);
    if (!validationResult.valid) {
      throw new ConfigurationValidationError(
        `Workflow validation failed: ${validationResult.errors.join(", ")}`,
        {
          configType: "workflow",
          configPath: "workflow.definition",
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

    // Persist to storage first (write-through: DB is source of truth)
    if (this.storageAdapter) {
      await persistWorkflow(workflow, this.storageAdapter);
    }

    // Save the workflow definition to memory cache after successful persistence.
    this.workflows.set(workflow.id, workflow);

    // Preprocess workflow asynchronously (delegated to preprocessWorkflow)
    try {
      await preprocessWorkflow(workflow, {
        workflowRegistry: this,
        graphRegistry: this.graphRegistry!,
        relationshipRegistry: this.relationshipRegistry!,
      });
    } catch (error) {
      // Remove the workflow from both memory and storage if preprocessing fails
      this.workflows.delete(workflow.id);
      if (this.storageAdapter) {
        try {
          await removeWorkflow(workflow.id, this.storageAdapter);
        } catch (removeError) {
          logger.error("Failed to remove workflow from storage after preprocessing failure", {
            workflowId: workflow.id,
            error: getErrorMessage(removeError),
          });
        }
      }
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
  async update(workflowId: string, updates: Partial<WorkflowTemplate>, options?: UpdateOptions): Promise<void> {
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

    // Persist to storage first (write-through: DB is source of truth)
    if (this.storageAdapter) {
      await persistWorkflow(updatedWorkflow, this.storageAdapter);
    }

    // Update the workflow in memory after successful persistence.
    this.workflows.set(workflowId, updatedWorkflow);
  }

  /**
   * Register or update the workflow definition (update if it exists, create if it doesn't).
   * @param workflow The workflow definition
   */
  async upsert(workflow: WorkflowTemplate): Promise<void> {
    if (this.workflows.has(workflow.id)) {
      await this.update(workflow.id, workflow);
    } else {
      this.register(workflow);
    }
  }

  // ============================================================
  // Query Methods
  // ============================================================

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
    const allWorkflows = new Map(this.workflows);

    // If storage adapter is available, fetch workflows not yet in memory
    if (this.storageAdapter) {
      try {
        const storageIds = await this.storageAdapter.list();
        const missingIds = storageIds.filter((id) => !allWorkflows.has(id));

        if (missingIds.length > 0) {
          logger.debug("Loading missing workflows from storage during list", {
            count: missingIds.length,
          });

          for (const id of missingIds) {
            try {
              const workflow = await loadWorkflow(id, this.storageAdapter);
              if (workflow) {
                // Populate both the local result and the memory cache for consistency
                allWorkflows.set(id, workflow);
                this.workflows.set(id, workflow);
              }
            } catch (loadError) {
              logger.error("Failed to load workflow from storage during list", {
                workflowId: id,
                error: getErrorMessage(loadError),
              });
            }
          }
        }
      } catch (error) {
        logger.error("Failed to list workflows from storage adapter", {
          error: getErrorMessage(error),
        });
        // Fall through to return in-memory workflows only
      }
    }

    return this.buildWorkflowSummaries(Array.from(allWorkflows.values()));
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

  // ============================================================
  // Reference Checking (delegated to WorkflowRelationshipRegistry)
  // ============================================================

  /**
   * Check workflow reference
   * @param workflowId Workflow ID
   * @returns Reference information
   */
  async checkWorkflowReferences(workflowId: string): Promise<WorkflowReferenceInfo> {
    const workflowExecutionRegistry = this.workflowExecutionRegistry;
    if (!workflowExecutionRegistry) {
      throw new ExecutionError("WorkflowExecutionRegistry not available", undefined, workflowId, {
        operation: "check_workflow_references",
      });
    }
    return await checkWorkflowReferences(this, workflowExecutionRegistry, workflowId);
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
   * Format citation details information
   * @param references List of references
   * @returns Formatted string
   */
  private formatReferenceDetails(
    references: import("../types/reference.js").WorkflowReference[],
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

  // ============================================================
  // Unregister
  // ============================================================

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

    // Remove from storage first (write-through: DB is source of truth)
    if (this.storageAdapter) {
      await removeWorkflow(workflowId, this.storageAdapter);
    }

    this.workflows.delete(workflowId);

    // Clean up reference relationships (delegated to WorkflowRelationshipRegistry)
    this.relationshipRegistry!.cleanupWorkflowReferences(workflowId);
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

  // ============================================================
  // Clear
  // ============================================================

  /**
   * Clear all workflow definitions.
   */
  clear(): void {
    this.workflows.clear();
    this.activeWorkflows.clear();
    this.relationshipRegistry!.clear();
  }

  // ============================================================
  // Validation
  // ============================================================

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

  // ============================================================
  // Utility Methods
  // ============================================================

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

  // ============================================================
  // Relationship Delegation Methods (delegated to WorkflowRelationshipRegistry)
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
    this.relationshipRegistry!.registerSubgraphRelationship(
      parentWorkflowId,
      subgraphNodeId,
      childWorkflowId,
    );
  }

  /**
   * Retrieve the workflow hierarchy structure
   * @param workflowId Workflow ID
   * @returns Hierarchy structure information
   */
  getWorkflowHierarchy(workflowId: string): import("../types/relationship.js").WorkflowHierarchy {
    return this.relationshipRegistry!.getWorkflowHierarchy(workflowId);
  }

  /**
   * Get the parent workflow
   * @param workflowId Workflow ID
   * @returns Parent workflow ID or null
   */
  getParentWorkflow(workflowId: string): string | null {
    return this.relationshipRegistry!.getParentWorkflow(workflowId);
  }

  /**
   * Get sub-workflows
   * @param workflowId Workflow ID
   * @returns Array of sub-workflow IDs
   */
  getChildWorkflows(workflowId: string): string[] {
    return this.relationshipRegistry!.getChildWorkflows(workflowId);
  }

  // ============================================================
  // Storage Initialization (delegated to WorkflowStorageAdapter)
  // ============================================================

  /**
   * Initialize workflows from storage
   * Loads all persisted workflow definitions into memory cache.
   */
  async initializeFromStorage(): Promise<void> {
    if (!this.storageAdapter) {
      return;
    }

    await initializeWorkflowsFromStorage(this.storageAdapter, this.workflows);
  }
}
