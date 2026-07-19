/**
 * Workflow Adapter
 * Wraps workflow-related SDK API calls.
 * Uses runtime utils for batch registration and error handling.
 */

import { BaseAdapter } from "./base-adapter.js";
import { resolve } from "path";
import { CLINotFoundError } from "../types/cli-types.js";
import { parseWorkflow } from "@wf-agent/sdk/api";
import { loadConfigFile } from "@wf-agent/config-processor";
import { batchRegisterFromDir } from "@wf-agent/runtime/adapters";
import type { WorkflowTemplate } from "@wf-agent/types";

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
        createdAt: new Date().toISOString(),
        updatedAt: new Date().toISOString(),
      }));

      return summaries;
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
   * Delete workflow
   */
  async deleteWorkflow(id: string): Promise<void> {
    return this.executeWithErrorHandling(async () => {
      const api = this.sdk.workflows;
      await api.delete(id);

      // Output to stdout for user visibility and test verification, also log for audit
      this.logOperation(`Workflow is deleted: ${id}`);
    }, "Delete the workflow");
  }
}
