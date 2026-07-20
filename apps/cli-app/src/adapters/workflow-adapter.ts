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
import type { WorkflowTemplate } from "@wf-agent/types";
import { SDKKit } from "@wf-agent/sdk-kit";

/**
 * Workflow Template with metadata fields
 */
interface WorkflowWithMetadata extends WorkflowTemplate {
  // type is already WorkflowTemplateType from WorkflowTemplate
}

/**
 * Workflow Adapter
 */
export class WorkflowAdapter extends BaseAdapter {
  private _sdkKit: SDKKit | null = null;

  /**
   * Get or create SDKKit instance for advanced operations
   */
  private getKit(): SDKKit {
    if (!this._sdkKit) {
      this._sdkKit = new SDKKit(this.sdk as any);
    }
    return this._sdkKit;
  }

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
      await api.create(workflow);

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
          await this.sdk.workflows.create(workflow);
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
   * Update an existing workflow from a file
   * @param id Workflow ID
   * @param filePath Configuration file path
   * @param parameters Runtime parameters (for template substitution)
   * @returns Updated workflow definition
   */
  async updateWorkflow(id: string, filePath: string, parameters?: Record<string, unknown>): Promise<WorkflowTemplate> {
    return this.executeWithErrorHandling(async () => {
      const fullPath = resolve(process.cwd(), filePath);
      const { content, format } = await loadConfigFile(fullPath);
      const workflow = await parseWorkflow(content, format, parameters);

      const api = this.sdk.workflows;
      const result = await api.update(id, workflow);

      if (result.result.isErr()) {
        throw new Error(result.result.error.message);
      }

      this.logOperation(`Workflow updated: ${id}`);
      return workflow;
    }, "Update workflow");
  }

  /**
   * Clone an existing workflow with a new ID
   * @param sourceId Source workflow ID
   * @param targetId Target workflow ID for the clone
   * @param options Optional name and description overrides
   * @returns The cloned workflow template
   */
  async cloneWorkflow(
    sourceId: string,
    targetId: string,
    options?: { name?: string; description?: string },
  ): Promise<WorkflowTemplate> {
    return this.executeWithErrorHandling(async () => {
      const kit = this.getKit();
      const result = await kit.resource().workflows().clone(sourceId, targetId);

      if (result.isErr()) {
        throw new Error(result.error.message);
      }

      // Fetch the cloned workflow using the SDK API (returns full WorkflowTemplate type)
      const api = this.sdk.workflows;
      const template = await api.get(targetId);
      if (!template) {
        throw new Error(`Cloned workflow not found: ${targetId}`);
      }

      // Apply name/description overrides if provided
      if (options?.name || options?.description) {
        const updates: Partial<WorkflowTemplate> = {};
        if (options.name) updates.name = options.name;
        if (options.description) updates.description = options.description;
        await api.update(targetId, updates);
        if (options.name) template.name = options.name;
        if (options.description) template.description = options.description;
      }

      this.logOperation(`Workflow cloned: ${sourceId} -> ${targetId}`);
      return template;
    }, "Clone workflow");
  }

  /**
   * Rollback a workflow to a previous version
   * @param id Workflow ID
   * @param version Target version to rollback to
   * @returns The rolled-back workflow template
   */
  async rollbackWorkflow(id: string, version: string): Promise<WorkflowTemplate> {
    return this.executeWithErrorHandling(async () => {
      const kit = this.getKit();
      const result = await kit.resource().workflows().rollback(id, version);

      if (result.isErr()) {
        throw new Error(result.error.message);
      }

      // Fetch the rolled-back workflow using the SDK API (returns full WorkflowTemplate type)
      const api = this.sdk.workflows;
      const template = await api.get(id);
      if (!template) {
        throw new Error(`Rolled-back workflow not found: ${id}`);
      }

      this.logOperation(`Workflow rolled back: ${id} to version ${version}`);
      return template;
    }, "Rollback workflow");
  }

  /**
   * List all versions of a workflow
   * @param id Workflow ID
   * @returns Array of workflow versions
   */
  async listWorkflowVersions(id: string): Promise<Array<{ version: string; createdAt: string; description?: string }>> {
    return this.executeWithErrorHandling(async () => {
      const kit = this.getKit();
      const result = await kit.resource().workflows().listVersions(id);

      if (result.isErr()) {
        throw new Error(result.error.message);
      }

      return result.value.map((v: any) => ({
        version: v.version || v.id || "N/A",
        createdAt: String(v.createdAt || v.timestamp || ""),
        description: v.description,
      }));
    }, "List workflow versions");
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

      return summaries;
    }, "List the workflow");
  }

  /**
   * Get workflow details
   */
  async getWorkflow(id: string): Promise<WorkflowWithMetadata> {
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
   * Delete workflow
   */
  async deleteWorkflow(id: string): Promise<void> {
    return this.executeWithErrorHandling(async () => {
      const api = this.sdk.workflows;
      const result = await api.delete(id);

      // Check if the delete operation was successful
      if (result && typeof result.result !== 'undefined' && result.result.isErr()) {
        throw new Error(result.result.error.message);
      }

      // Output to stdout for user visibility and test verification, also log for audit
      this.logOperation(`Workflow is deleted: ${id}`);
    }, "Delete the workflow");
  }
}
