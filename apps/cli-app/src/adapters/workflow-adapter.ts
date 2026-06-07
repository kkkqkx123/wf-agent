/**
 * Workflow Adapter
 * Wraps workflow-related SDK API calls
 */

import { BaseAdapter } from "./base-adapter.js";
import { resolve, join, extname } from "path";
import { CLINotFoundError } from "../types/cli-types.js";
import { getData, isFailure, getError, parseWorkflow } from "@wf-agent/sdk/api";
import { loadConfigFile } from "@wf-agent/config-processor";
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
      const result = await api.create(workflow);

      // Check if the operation was successful
      if (isFailure(result)) {
        const error = getError(result);
        throw error;
      }

      // Output to stdout for user visibility and test verification, also log for audit
      this.logOperation(`Workflow is registered: ${workflow.name} (${workflow.id})`);
      return workflow;
    }, "Register a workflow");
  }

  /**
   * Batch register workflows from directory
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
      const { readdir } = await import("fs/promises");

      const dir = options.configDir || "./configs/workflows";
      const files: string[] = [];

      const scanDir = async (currentDir: string) => {
        const entries = await readdir(currentDir, { withFileTypes: true });
        for (const entry of entries) {
          const fullPath = join(currentDir, entry.name);
          if (entry.isDirectory() && options.recursive !== false) {
            await scanDir(fullPath);
          } else if (entry.isFile()) {
            const ext = extname(entry.name).toLowerCase();
            if (ext === ".toml" || ext === ".json") {
              if (!options.filePattern || options.filePattern.test(entry.name)) {
                files.push(fullPath);
              }
            }
          }
        }
      };

      await scanDir(dir);

      const success: WorkflowTemplate[] = [];
      const failures: Array<{ filePath: string; error: string }> = [];

      const api = this.sdk.workflows;
      for (const file of files) {
        try {
          const { content, format } = await loadConfigFile(file);
          const workflow = await parseWorkflow(content, format, options.parameters);
          const createResult = await api.create(workflow);
          // Check if the operation was successful
          if (isFailure(createResult)) {
            const error = getError(createResult);
            throw error;
          }
          success.push(workflow);
          // Output to stdout for user visibility and test verification, also log for audit
          this.logOperation(`Workflow is registered: ${workflow.name} (${workflow.id})`);
        } catch (error) {
          failures.push({
            filePath: file,
            error: error instanceof Error ? error.message : String(error),
          });
          // Output to stderr for user visibility and test verification, also log for audit
          this.logOperationFailure(`Failed to register workflow: ${file}`);
        }
      }

      return { success, failures };
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
      
      const result = await api.getAll(workflowFilter);
      const workflows = getData(result) || [];

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
      const result = await api.get(id);
      const workflow = getData(result);

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
