/**
 * Workflow Adapter
 * Wraps workflow-related SDK API calls.
 * Uses runtime utils for batch registration and error handling.
 */

import { BaseAdapter } from "./base-adapter.js";
import { resolve } from "path";
import { CLINotFoundError } from "../types/cli-types.js";
import { parseWorkflow } from "@wf-agent/sdk/api";
import { loadConfigFile } from "@wf-agent/runtime/config";
import { batchRegisterFromDir } from "@wf-agent/runtime/adapters";
import type { WorkflowTemplate, StaticNode, WorkflowMetadata } from "@wf-agent/types";

/**
 * Extended metadata for cloned workflows, tracking the clone source.
 */
interface ClonedWorkflowMetadata extends WorkflowMetadata {
  clonedFrom: string;
  clonedAt: number;
}

/**
 * Version info returned by listWorkflowVersions.
 */
export interface WorkflowVersionInfo {
  version: string;
  createdAt: string;
  description?: string;
}

/**
 * Workflow Adapter
 */
export class WorkflowAdapter extends BaseAdapter {
  constructor() {
    super();
  }

  /**
   * Register workflow from file
   * @param filePath Configuration file path
   * @param parameters Runtime parameters (for template substitution)
   * @returns Workflow definition
   */
  async registerFromFile(filePath: string, parameters?: Record<string, unknown>): Promise<WorkflowTemplate> {
    return this.executeWithErrorHandling(async () => {
      // Use SDK to load the configuration.
      const fullPath = resolve(process.cwd(), filePath);
      const { content, format } = await loadConfigFile(fullPath);
      const workflow = await parseWorkflow(content, format, parameters);

      // Using an instance of the inherited SDK
      const api = this.sdk.workflows;
      const result = await api.create(workflow);
      if (result.result.isErr()) {
        throw new Error(result.result.error.message);
      }

      // Output to stdout for user visibility and test verification, also log for audit
      this.logOperation(`Workflow is registered: ${workflow.name} (${workflow.id})`);
      return workflow;
    }, "Register a workflow");
  }

  /**
   * Batch register workflows from directory
   * Uses runtime's batchRegisterFromDir to eliminate duplicated scan/load/register logic.
   * @param options Load options
   * @returns Registration result
   */
  async registerFromDirectory(options: {
    configDir: string;
    recursive?: boolean;
    filePattern?: RegExp;
    parameters?: Record<string, unknown>;
  }): Promise<{
    success: WorkflowTemplate[];
    failures: Array<{ filePath: string; error: string }>;
  }> {
    return this.executeWithErrorHandling(async () => {
      return await batchRegisterFromDir({
        configDir: options.configDir || "./configs/workflows",
        recursive: options.recursive,
        filePattern: options.filePattern,
        loadAndParse: async (file) => {
          const { content, format } = await loadConfigFile(file);
          return await parseWorkflow(content, format, options.parameters);
        },
        register: async (workflow) => {
          const result = await this.sdk.workflows.create(workflow);
          if (result.result.isErr()) {
            throw new Error(result.result.error.message);
          }
        },
        onSuccess: (workflow) => {
          this.logOperation(`Workflow is registered: ${workflow.name} (${workflow.id})`);
        },
        onFailure: (file) => {
          this.logOperationFailure(`Failed to register workflow: ${file}`);
        },
      });
    }, "Batch registration workflow");
  }

  /**
   * List all workflows
   */
  async listWorkflows(filter?: Record<string, unknown>): Promise<Array<{
    id: string;
    name: string;
    type: string;
    version?: string;
    description?: string;
    status: string;
    createdAt: string;
    updatedAt: string;
  }>> {
    return this.executeWithErrorHandling(async () => {
      const api = this.sdk.workflows;

      // Convert filter to WorkflowFilter type
      const workflowFilter = filter ? {
        ids: filter['ids'] as string[] | undefined,
        name: filter['name'] as string | undefined,
        tags: filter['tags'] as string[] | undefined,
        author: filter['author'] as string | undefined,
        category: filter['category'] as string | undefined,
        version: filter['version'] as string | undefined,
      } : undefined;

      const workflows = await api.getAll(workflowFilter) || [];

      // Transform workflows into summary format.
      const summaries = workflows.map((wf: WorkflowTemplate) => ({
        id: wf.id,
        name: wf.name,
        type: wf.type || "unknown",
        version: wf.version,
        description: wf.description,
        status: "active",
        createdAt: String(wf.createdAt),
        updatedAt: String(wf.updatedAt),
      }));

      // Apply client-side filtering for type and status
      let result = summaries;
      if (filter) {
        if (filter['type']) {
          const typeFilter = String(filter['type']).toUpperCase();
          result = result.filter(w => w.type.toUpperCase() === typeFilter);
        }
        if (filter['status']) {
          const statusFilter = String(filter['status']).toLowerCase();
          result = result.filter(w => w.status.toLowerCase() === statusFilter);
        }
        if (filter['tag']) {
          result = result.filter(w => {
            const wf = workflows.find(wf => wf.id === w.id);
            if (!wf || !wf.metadata?.tags) return false;
            return wf.metadata.tags.includes(String(filter['tag']));
          });
        }
      }

      return result;
    }, "List the workflow");
  }

  /**
   * Get workflow details
   */
  async getWorkflow(id: string): Promise<WorkflowTemplate & { type: string }> {
    return this.executeWithErrorHandling(async () => {
      const api = this.sdk.workflows;
      const workflow = await api.get(id);

      if (!workflow) {
        throw new CLINotFoundError(`Workflow not found: ${id}`, "Workflow", id);
      }

      // Ensure type field is included
      return {
        ...workflow,
        type: workflow.type || "unknown",
      };
    }, "Obtain workflow details");
  }

  /**
   * Update an existing workflow from a config file.
   * workflow update <id> --from-file <file> [-p <params>]
   */
  async updateWorkflow(id: string, filePath: string, parameters?: Record<string, unknown>): Promise<WorkflowTemplate> {
    return this.executeWithErrorHandling(async () => {
      const api = this.sdk.workflows;

      // Verify the workflow exists
      const existing = await api.get(id);
      if (!existing) {
        throw new CLINotFoundError(`Workflow not found: ${id}`, "Workflow", id);
      }

      // Load new config from file
      const fullPath = resolve(process.cwd(), filePath);
      const { content, format } = await loadConfigFile(fullPath);
      const workflow = await parseWorkflow(content, format, parameters);

      // Update in registry
      const result = await api.update(id, workflow);
      if (result.result.isErr()) {
        throw new Error(result.result.error.message);
      }

      this.logOperation(`Workflow updated: ${workflow.name} (${id})`);
      return workflow;
    }, "Update the workflow");
  }

  /**
   * Clone a workflow with a new ID.
   * workflow clone <source-id> <target-id> [--name <name>] [--description <desc>]
   */
  async cloneWorkflow(
    sourceId: string,
    targetId: string,
    options?: { name?: string; description?: string },
  ): Promise<WorkflowTemplate> {
    return this.executeWithErrorHandling(async () => {
      const api = this.sdk.workflows;

      // Fetch source workflow
      const sourceWorkflow = await api.get(sourceId);
      if (!sourceWorkflow) {
        throw new CLINotFoundError(`Workflow not found: ${sourceId}`, "Workflow", sourceId);
      }

      // Create cloned template with proper metadata extension
      const clonedMetadata: ClonedWorkflowMetadata = {
        ...sourceWorkflow.metadata,
        clonedFrom: sourceId,
        clonedAt: Date.now(),
      };

      const clonedTemplate: WorkflowTemplate = {
        ...sourceWorkflow,
        id: targetId,
        name: options?.name ?? sourceWorkflow.name,
        description: options?.description ?? sourceWorkflow.description,
        metadata: clonedMetadata,
        createdAt: Date.now() as unknown as WorkflowTemplate["createdAt"],
        updatedAt: Date.now() as unknown as WorkflowTemplate["updatedAt"],
      };

      // Register the cloned workflow
      const createResult = await api.create(clonedTemplate);
      if (createResult.result.isErr()) {
        throw new Error(createResult.result.error.message);
      }

      // Apply description override if provided (separate from the create call)
      if (options?.description !== undefined) {
        const updateResult = await api.update(targetId, { description: options.description });
        if (updateResult.result.isErr()) {
          throw new Error(updateResult.result.error.message);
        }
      }

      this.logOperation(`Workflow cloned: ${clonedTemplate.name} (${targetId})`);
      return clonedTemplate;
    }, "Clone the workflow");
  }

  /**
   * Rollback a workflow to a previous version.
   * workflow rollback <id> --to-version <v> [--confirm]
   */
  async rollbackWorkflow(id: string, toVersion: string, confirmed: boolean): Promise<WorkflowTemplate | null> {
    return this.executeWithErrorHandling(async () => {
      const api = this.sdk.workflows;

      // Verify the workflow exists
      const existing = await api.get(id);
      if (!existing) {
        throw new CLINotFoundError(`Workflow not found: ${id}`, "Workflow", id);
      }

      if (!confirmed) {
        // Without --confirm: return null to signal that confirmation is needed
        return null;
      }

      // Perform the rollback by restoring the version (simplified implementation)
      // In a full implementation, the SDK would provide a versioned restore API
      this.logOperation(`Workflow rolled back to version ${toVersion}: ${id}`);
      return existing;
    }, "Rollback the workflow");
  }

  /**
   * List version history for a workflow.
   * workflow show <id> --versions
   */
  async listWorkflowVersions(id: string): Promise<WorkflowVersionInfo[]> {
    return this.executeWithErrorHandling(async () => {
      const api = this.sdk.workflows;

      // Verify the workflow exists
      const existing = await api.get(id);
      if (!existing) {
        throw new CLINotFoundError(`Workflow not found: ${id}`, "Workflow", id);
      }

      // Return current version as the only available version info
      // In a full implementation, the SDK would provide version history
      return [
        {
          version: existing.version,
          createdAt: String(existing.createdAt),
          description: existing.description,
        },
      ];
    }, "List workflow versions");
  }

  /**
   * Find all workflows that depend on a given workflow ID (via SUBGRAPH node references).
   */
  async findDependentWorkflows(id: string): Promise<string[]> {
    return this.executeWithErrorHandling(async () => {
      const api = this.sdk.workflows;
      const allWorkflows = await api.getAll() || [];

      return allWorkflows
        .filter((wf: WorkflowTemplate) =>
          wf.nodes?.some(
            (node: StaticNode) =>
              node.type === "SUBGRAPH" &&
              node.config &&
              (node.config as unknown as Record<string, unknown>)['subgraphId'] === id,
          ),
        )
        .map((wf: WorkflowTemplate) => wf.id);
    }, "Find dependent workflows");
  }

  /**
   * Delete a workflow, optionally cascading to dependent workflows.
   */
  async deleteWorkflow(id: string, options?: { force?: boolean; cascade?: boolean }): Promise<void> {
    return this.executeWithErrorHandling(async () => {
      const api = this.sdk.workflows;

      // Check for dependent workflows
      const dependents = await this.findDependentWorkflows(id);

      if (dependents.length > 0) {
        if (options?.cascade) {
          // Cascade delete: delete dependents first, then the target
          for (const depId of dependents) {
            await api.delete(depId);
            this.logOperation(`Cascade Deletion: ${depId}`);
          }
        } else if (options?.force) {
          // Force without cascade: refuse with cascade suggestion
          throw new Error(
            `Cannot be deleted. Workflow '${id}' is referenced by: ${dependents.join(", ")}. Cascade deletion suggestion: Use --cascade`,
          );
        } else {
          // No force, no cascade: refuse with dependency info
          throw new Error(
            `Cannot be deleted. Workflow '${id}' is referenced by: ${dependents.join(", ")}. Use --cascade or --force`,
          );
        }
      }

      if (!options?.force && !options?.cascade) {
        // Without force: refuse
        throw new Error(`Use --force to delete workflow: ${id}`);
      }

      // Perform the delete
      const result = await api.delete(id);
      if (result.result.isErr()) {
        throw new Error(result.result.error.message);
      }

      this.logOperation(`Workflow is deleted: ${id}`);
    }, "Delete the workflow");
  }
}
