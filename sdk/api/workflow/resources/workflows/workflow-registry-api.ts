/**
 * WorkflowRegistryAPI - Workflow Registry API
 * Inherits GenericResourceAPI and provides unified CRUD operations.
 */

import {
  validateRequiredFields,
  validateStringLength,
  validateArray,
  validatePattern,
} from "../../../shared/validation/validation-strategy.js";

import { now } from "@wf-agent/common-utils";
import { CrudResourceAPI } from "../../../shared/resources/generic-resource-api.js";
import type { WorkflowDefinition } from "@wf-agent/types";
import { WorkflowNotFoundError } from "@wf-agent/types";
import type { APIDependencyManager } from "../../../shared/core/sdk-dependencies.js";
import type { Timestamp } from "@wf-agent/types";

/**
 * Workflow filters
 */
export interface WorkflowFilter {
  /** List of workflow IDs */
  ids?: string[];
  /** Workflow name (fuzzy matching supported) */
  name?: string;
  /** tagged array */
  tags?: string[];
  /** author */
  author?: string;
  /** categorization */
  category?: string;
  /** releases */
  version?: string;
  /** Creation timeframe */
  createdAtRange?: { start?: Timestamp; end?: Timestamp };
  /** Updated timeframe */
  updatedAtRange?: { start?: Timestamp; end?: Timestamp };
}

/**
 * Summary of workflows
 */
export interface WorkflowSummary {
  /** Workflow ID */
  id: string;
  /** Workflow name */
  name: string;
  /** Workflow description */
  description?: string;
  /** releases */
  version: string;
  /** Number of nodes */
  nodeCount: number;
  /** algebraic quantity of a side */
  edgeCount: number;
  /** Creation time */
  createdAt: Timestamp;
  /** update time */
  updatedAt: Timestamp;
  /** tagged array */
  tags?: string[];
  /** categorization */
  category?: string;
}

/**
 * WorkflowRegistryAPI - Workflow Registry API
 */
export class WorkflowRegistryAPI extends CrudResourceAPI<
  WorkflowDefinition,
  string,
  WorkflowFilter
> {
  private dependencies: APIDependencyManager;

  constructor(dependencies: APIDependencyManager) {
    super();
    this.dependencies = dependencies;
  }

  // ============================================================================
  // Implementing Abstract Methods
  // ============================================================================

  /**
   * Getting workflows from the registry
   */
  protected async getResource(id: string): Promise<WorkflowDefinition | null> {
    return this.dependencies.getWorkflowRegistry().get(id) || null;
  }

  /**
   * Get all workflows from the registry
   */
  protected async getAllResources(): Promise<WorkflowDefinition[]> {
    const summaries = this.dependencies.getWorkflowRegistry().list();
    const workflows: WorkflowDefinition[] = [];
    for (const summary of summaries) {
      const workflow = this.dependencies.getWorkflowRegistry().get(summary.id);
      if (workflow) {
        workflows.push(workflow);
      }
    }
    return workflows;
  }

  /**
   * Creating Workflows
   */
  protected async createResource(workflow: WorkflowDefinition): Promise<void> {
    await this.dependencies.getWorkflowRegistry().registerAsync(workflow);
  }

  /**
   * Deleting workflows
   */
  protected async deleteResource(id: string): Promise<void> {
    this.dependencies.getWorkflowRegistry().unregister(id);
  }

  /**
   * Update Workflow - Create New Version Instance
   * Versioning by creating a new workflow instance based on the immutability principle
   */
  protected async updateResource(id: string, updates: Partial<WorkflowDefinition>): Promise<void> {
    // Call createVersionedUpdate directly to implement the update operation.
    await this.createVersionedUpdate(id, updates, {
      keepOriginal: false,
      force: false,
    });
  }

  /**
   * Creating Versioned Updates
   * Versioning updates by creating new workflow instances based on the immutability principle
   */
  async createVersionedUpdate(
    id: string,
    updates: Partial<WorkflowDefinition>,
    options?: {
      versionStrategy?: "patch" | "minor" | "major";
      keepOriginal?: boolean;
      force?: boolean;
    },
  ): Promise<string> {
    const existingWorkflow = await this.getResource(id);
    if (!existingWorkflow) {
      throw new WorkflowNotFoundError(`Workflow with ID '${id}' not found`, id);
    }

    // Automatic incremental versioning using the versioning toolkit
    const strategy = options?.versionStrategy ?? "patch";
    const newVersion = this.autoIncrementVersion(existingWorkflow.version, strategy);

    // Create a brand new workflow instance (completely unrelated to the previous one)
    const newWorkflow: WorkflowDefinition = {
      ...existingWorkflow,
      ...updates,
      version: newVersion,
      updatedAt: now(),
    };

    // Register for new versions of workflows
    await this.createResource(newWorkflow);

    // Optional: whether to keep the original version
    if (options?.keepOriginal === false) {
      // Call the workflowRegistry's unregister method directly to let it handle reference checking internally
      this.dependencies.getWorkflowRegistry().unregister(id, {
        force: options?.force,
        checkReferences: true,
      });
    }

    return newWorkflow.id;
  }

  /**
   * Auto-incrementing version number
   * @param currentVersion The current version number
   * @param strategy Versioning strategy
   * @returns The incremented version number.
   */
  private autoIncrementVersion(
    currentVersion: string,
    strategy: "patch" | "minor" | "major",
  ): string {
    const parts = currentVersion.split(".").map(Number);
    let major = parts[0] || 0;
    let minor = parts[1] || 0;
    let patch = parts[2] || 0;

    switch (strategy) {
      case "major":
        major += 1;
        minor = 0;
        patch = 0;
        break;
      case "minor":
        minor += 1;
        patch = 0;
        break;
      case "patch":
        patch += 1;
        break;
    }

    return `${major}.${minor}.${patch}`;
  }

  /**
   * Applying Filter Criteria
   */
  protected override applyFilter(
    workflows: WorkflowDefinition[],
    filter: WorkflowFilter,
  ): WorkflowDefinition[] {
    return workflows.filter(workflow => {
      if (filter.ids && !filter.ids.some(id => workflow.id.includes(id))) {
        return false;
      }
      if (filter.name && !workflow.name.includes(filter.name)) {
        return false;
      }
      if (filter.version && workflow.version !== filter.version) {
        return false;
      }
      if (filter.tags && workflow.metadata?.tags) {
        if (!filter.tags.every(tag => workflow.metadata?.tags?.includes(tag))) {
          return false;
        }
      }
      if (filter.category && workflow.metadata?.category !== filter.category) {
        return false;
      }
      if (filter.author && workflow.metadata?.author !== filter.author) {
        return false;
      }
      return true;
    });
  }

  /**
   * Empty all workflows
   */
  protected override async clearResources(): Promise<void> {
    this.dependencies.getWorkflowRegistry().clear();
  }

  /**
   * Validating Workflows
   * @param workflow Workflow definition
   * @returns Validation results
   */
  protected override async validateResource(
    workflow: WorkflowDefinition,
  ): Promise<{ valid: boolean; errors: string[] }> {
    const errors: string[] = [];

    // Validating Required Fields with Simplified Validation Tool
    const requiredResult = validateRequiredFields(workflow, ["id", "name", "version"], "workflow");
    if (requiredResult.isErr()) {
      errors.push(
        ...requiredResult.unwrapOrElse((err: unknown[]) =>
          err.map((error: unknown) => String(error)),
        ),
      );
    }

    // Validation Node Array
    const arrayResult = validateArray(workflow.nodes, "workflow node", 1);
    if (arrayResult.isErr()) {
      errors.push(
        ...arrayResult.unwrapOrElse((err: unknown[]) => err.map((error: unknown) => String(error))),
      );
    }

    // Verify ID length
    if (workflow.id) {
      const idResult = validateStringLength(workflow.id, "Workflow ID", 1, 100);
      if (idResult.isErr()) {
        errors.push(
          ...idResult.unwrapOrElse((err: unknown[]) => err.map((error: unknown) => String(error))),
        );
      }
    }

    // Verify name length
    if (workflow.name) {
      const nameResult = validateStringLength(workflow.name, "Workflow name", 1, 200);
      if (nameResult.isErr()) {
        errors.push(
          ...nameResult.unwrapOrElse((err: unknown[]) =>
            err.map((error: unknown) => String(error)),
          ),
        );
      }
    }

    // Validate version format (simple validation)
    if (workflow.version) {
      const versionPattern = /^[0-9]+\.[0-9]+\.[0-9]+$/;
      const versionResult = validatePattern(
        workflow.version,
        "Workflow version",
        versionPattern,
        "The workflow version is not in the correct format, it should be in x.y.z format",
      );
      if (versionResult.isErr()) {
        errors.push(
          ...versionResult.unwrapOrElse((err: unknown[]) =>
            err.map((error: unknown) => String(error)),
          ),
        );
      }
    }

    return {
      valid: errors.length === 0,
      errors,
    };
  }

  // ============================================================================
  // Workflow-specific methods
  // ============================================================================

  /**
   * Get a list of workflow summaries
   * @param filter filter criteria
   * @returns array of workflow summaries
   */
  async getWorkflowSummaries(filter?: WorkflowFilter): Promise<WorkflowSummary[]> {
    const summaries = this.dependencies.getWorkflowRegistry().list();

    if (!filter) {
      return summaries;
    }

    // Applying Filter Criteria
    return summaries.filter((summary: WorkflowSummary) => {
      if (filter.ids && !filter.ids.some(id => summary.id.includes(id))) {
        return false;
      }
      if (filter.name && !summary.name.includes(filter.name)) {
        return false;
      }
      if (filter.version && summary.version !== filter.version) {
        return false;
      }
      if (filter.tags && summary.tags) {
        if (!filter.tags.every(tag => summary.tags?.includes(tag))) {
          return false;
        }
      }
      if (filter.category && summary.category !== filter.category) {
        return false;
      }
      // Author filtering is not supported at this time because WorkflowSummary does not have an author field.
      return true;
    });
  }

  /**
   * Get workflow by name
   * @param name Workflow name
   * @returns Workflow definition, or null if it doesn't exist.
   */
  async getWorkflowByName(name: string): Promise<WorkflowDefinition | null> {
    const workflow = this.dependencies.getWorkflowRegistry().getByName(name);
    if (workflow) {
      // Cache update logic (if caching is needed, it can be implemented here)
    }
    return workflow || null;
  }

  /**
   * Get the workflow list by tags
   * @param tags: An array of tags
   * @returns: An array of workflow definitions
   */
  async getWorkflowsByTags(tags: string[]): Promise<WorkflowDefinition[]> {
    return this.dependencies.getWorkflowRegistry().getByTags(tags);
  }

  /**
   * Get the workflow list by category
   * @param category: Category
   * @returns: Array of workflow definitions
   */
  async getWorkflowsByCategory(category: string): Promise<WorkflowDefinition[]> {
    return this.dependencies.getWorkflowRegistry().getByCategory(category);
  }

  /**
   * Get the workflow list by author
   * @param author  Author
   * @returns Array of workflow definitions
   */
  async getWorkflowsByAuthor(author: string): Promise<WorkflowDefinition[]> {
    return this.dependencies.getWorkflowRegistry().getByAuthor(author);
  }

  /**
   * Search Workflow
   * @param keyword Search keyword
   * @returns Array of workflow summaries
   */
  async searchWorkflows(keyword: string): Promise<WorkflowSummary[]> {
    return this.dependencies.getWorkflowRegistry().search(keyword);
  }

  /**
   * Export workflow
   * @param workflowId: Workflow ID
   * @returns: JSON string
   */
  async exportWorkflow(workflowId: string): Promise<string> {
    return this.dependencies.getWorkflowRegistry().export(workflowId);
  }

  /**
   * Import Workflow
   * @param json JSON string
   * @returns Workflow ID
   */
  async importWorkflow(json: string): Promise<string> {
    const workflowId = this.dependencies.getWorkflowRegistry().import(json);
    // Update the cache
    const workflow = this.dependencies.getWorkflowRegistry().get(workflowId);
    if (workflow) {
      // Cache update logic (you can implement this here if caching is required)
    }
    return workflowId;
  }

  /**
   * Retrieve the processed workflow definition
   * @param workflowId: Workflow ID
   * @returns: The processed workflow definition; returns null if it does not exist
   */
  async getProcessedWorkflow(workflowId: string): Promise<unknown | null> {
    const processed = this.dependencies.getWorkflowGraphRegistry().get(workflowId);
    return processed || null;
  }

  /**
   * Preprocess and store the workflow
   * @param workflow: Workflow definition
   * @returns: Processed workflow definition
   */
  async preprocessAndStoreWorkflow(workflow: WorkflowDefinition): Promise<unknown> {
    // Use registerAsync for full validation and preprocessing
    await this.dependencies.getWorkflowRegistry().registerAsync(workflow);
    // Return the preprocessed image.
    return this.dependencies.getWorkflowGraphRegistry().get(workflow.id);
  }

  /**
   * Get the workflow diagram structure
   * @param workflowId: Workflow ID
   * @returns: Diagram structure; returns null if it does not exist
   */
  async getWorkflowGraph(workflowId: string): Promise<unknown | null> {
    try {
      // Get the preprocessed graph directly from graph-registry.
      const processed = this.dependencies.getWorkflowGraphRegistry().get(workflowId);
      // The PreprocessedGraph itself is a Graph, so it does not require the `.graph` attribute.
      return processed || null;
    } catch {
      return null;
    }
  }

  /**
   * Register subgraph relationship
   * @param parentWorkflowId: Parent workflow ID
   * @param subgraphNodeId: SUBGRAPH node ID
   * @param childWorkflowId: Child workflow ID
   */
  async registerSubgraphRelationship(
    parentWorkflowId: string,
    subgraphNodeId: string,
    childWorkflowId: string,
  ): Promise<void> {
    this.dependencies
      .getWorkflowRegistry()
      .registerSubgraphRelationship(parentWorkflowId, subgraphNodeId, childWorkflowId);
  }

  /**
   * Retrieve the workflow hierarchy structure
   * @param workflowId: Workflow ID
   * @returns: Hierarchy structure information
   */
  async getWorkflowHierarchy(workflowId: string): Promise<unknown> {
    return this.dependencies.getWorkflowRegistry().getWorkflowHierarchy(workflowId);
  }

  /**
   * Get the parent workflow
   * @param workflowId: Workflow ID
   * @returns: Parent workflow ID or null
   */
  async getParentWorkflow(workflowId: string): Promise<string | null> {
    return this.dependencies.getWorkflowRegistry().getParentWorkflow(workflowId);
  }

  /**
   * Get Subworkflows
   * @param workflowId: Workflow ID
   * @returns: Array of Subworkflow IDs
   */
  async getChildWorkflows(workflowId: string): Promise<string[]> {
    return this.dependencies.getWorkflowRegistry().getChildWorkflows(workflowId);
  }

  /**
   * Get the underlying WorkflowRegistry instance
   * @returns WorkflowRegistry instance
   */
  getRegistry() {
    return this.dependencies.getWorkflowRegistry();
  }
}
